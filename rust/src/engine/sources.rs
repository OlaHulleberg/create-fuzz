//! Generating kinetic sources' environment: water wheels' flow and the boilers that drive
//! steam engines. The native fluid host supplies the fluid states and their flow; boilers read
//! the heat under their tank from the world's block states.
use super::*;
use create_engine::kinetics::Axis;

impl Run {
    /// The cells every existing wheel samples for its flow, sorted and deduplicated, so the
    /// native host answers for them in the same frame as this tick's fluid tick.
    pub(super) fn wheel_samples(&self) -> Vec<Pos> {
        let mut out = Vec::new();
        for (&p, &id) in &self.world {
            let Some((block, info)) = self.w.table.get(id) else {
                continue;
            };
            let (axis, large) = match block {
                create_engine::Block::WaterWheel => match info.facing {
                    Some(f) => (f.axis(), false),
                    None => continue,
                },
                create_engine::Block::LargeWaterWheel => match info.axis {
                    Some(a) => (a, true),
                    None => continue,
                },
                _ => continue,
            };
            for o in offsets(axis, large) {
                out.push(add(p, o));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// `WaterWheelBlockEntity.determineAndApplyFlowScore` when a wheel's surroundings changed or
    /// its lazy tick came.
    pub(super) fn apply_wheel_flow(&mut self) {
        for (p, axis, large) in self.w.take_wheel_checks(0, self.persisted.clock as u64) {
            let samples: Vec<([i32; 3], [f64; 3])> = offsets(axis, large)
                .into_iter()
                .map(|o| (o, self.flow_at(add(p, o))))
                .collect();
            let score = create_engine::waterwheel::flow_score(axis, &samples);
            self.w.set_wheel_flow(0, p, score);
            self.wheel_scores.insert(p, score.clamp(-1, 1) as i8);
        }
    }

    /// `WaterWheelBlockEntity.getFlowVectorAtPosition`: a bubble column reads its drag, anything
    /// else the native flow the host reported for this tick.
    fn flow_at(&self, p: Pos) -> [f64; 3] {
        let name = self.name(self.at(p));
        if name.starts_with("minecraft:bubble_column") {
            let down = name.contains("drag=true");
            return [0.0, if down { -1.0 } else { 1.0 }, 0.0];
        }
        if let Some(flow) = self.wheel_fluid_flow.get(&p) {
            return *flow;
        }
        // No native peer: every cell the scenario placed is air or a non-fluid host state.
        [0.0; 3]
    }

    /// `BlazeBurnerBlock.useItemOn` on a lit burner: a creative blaze cake sets the next heat,
    /// a blaze cake burns special fuel for 3200 ticks (`BoilerHeaters.blazeBurner` reads the
    /// resulting `blaze` property). Other fuels are outside this adapter.
    pub(super) fn use_burner(&mut self, p: Pos, item: &str) -> bool {
        use create_engine::burner::{self, Fuel};
        let Some((block, info)) = self.w.table.get(self.at(p)).map(|(b, i)| (b, *i)) else {
            return false;
        };
        if block != create_engine::Block::BlazeBurner
            || info
                .heat
                .is_none_or(|h| h == create_engine::fan::Heat::None)
            || !self.w.dims[0].burners.contains_key(&p)
        {
            return false;
        }
        let heat = if item == "create:creative_blaze_cake" {
            let shown = info.heat.unwrap_or(create_engine::fan::Heat::Smouldering);
            let Some(next) = self.w.dims[0]
                .burners
                .get_mut(&p)
                .map(|b| b.creative_fuel(shown))
            else {
                return false;
            };
            next
        } else if item == "create:blaze_cake" {
            if !self.w.fuel_burner(0, p, Fuel::Special, 3200, true) {
                return false;
            }
            self.w.dims[0].burners[&p].heat()
        } else {
            return false;
        };
        let next = with_prop(&self.name(self.at(p)), "blaze", burner::heat_name(heat));
        self.set(p, self.id(&next));
        self.mark_column(p);
        true
    }

    /// `BlazeBurnerBlockEntity.tick` on the server: each burning burner's heat sets its block.
    pub(super) fn tick_burners(&mut self) {
        for (p, heat) in self.w.tick_burners(0) {
            let next = with_prop(
                &self.name(self.at(p)),
                "blaze",
                create_engine::burner::heat_name(heat),
            );
            self.set(p, self.id(&next));
        }
    }

    /// The glue's `tick_boilers`: engines counted, shafts in front of them powered or plain, heat
    /// read under the boiler, water supply sampled.
    pub(super) fn tick_boilers(&mut self) {
        self.w.evaluate_boilers(0);
        for (p, powered) in self.w.shaft_changes(0) {
            let (_, props) = parse_state(&self.name(self.at(p)));
            let get = |k: &str| {
                props
                    .iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.as_str())
            };
            let shaft = format!(
                "create:{}[axis={},waterlogged={}]",
                if powered { "powered_shaft" } else { "shaft" },
                get("axis").unwrap_or("x"),
                get("waterlogged").unwrap_or("false"),
            );
            self.set(p, self.id(&shaft));
        }
        for (tank, width) in self.w.take_heat_checks(0) {
            // `updateTemperature`: every block under the tank's bottom layer.
            let w = i32::from(width);
            let heats: Vec<i32> = (0..w)
                .flat_map(|x| (0..w).map(move |z| [tank[0] + x, tank[1] - 1, tank[2] + z]))
                .map(|p| self.boiler_heater(p))
                .collect();
            self.w.set_boiler_heat(0, tank, &heats);
        }
        self.w.tick_boilers(0);
    }

    /// `BoilerHeater.findHeat` under a boiler: a blaze burner by its heat level, another block in
    /// `create:passive_boiler_heaters` passive unless it is out (an unlit campfire), anything else
    /// none. Lava and fire are not representable as scenario host states.
    fn boiler_heater(&self, p: Pos) -> i32 {
        use create_engine::fan::Heat;
        use create_engine::steam::{NO_HEAT, PASSIVE_HEAT, blaze_burner_heat};
        let (name, props) = parse_state(&self.name(self.at(p)));
        let get = |k: &str| {
            props
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        match name.as_str() {
            "create:blaze_burner" => {
                let heat = match get("blaze") {
                    Some("smouldering") => Heat::Smouldering,
                    Some("fading") => Heat::Fading,
                    Some("kindled") => Heat::Kindled,
                    Some("seething") => Heat::Seething,
                    _ => Heat::None,
                };
                blaze_burner_heat(heat)
            }
            "create:lit_blaze_burner" | "minecraft:magma_block" => PASSIVE_HEAT,
            "minecraft:campfire" | "minecraft:soul_campfire" => {
                if get("lit") == Some("true") {
                    PASSIVE_HEAT
                } else {
                    NO_HEAT
                }
            }
            _ => NO_HEAT,
        }
    }
}

/// The offsets a wheel of this axis reads (`WaterWheelBlockEntity.getOffsetsToCheck`).
fn offsets(axis: Axis, large: bool) -> Vec<[i32; 3]> {
    if large {
        create_engine::waterwheel::large_offsets(axis)
    } else {
        create_engine::waterwheel::small_offsets(axis).to_vec()
    }
}
