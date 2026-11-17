//! The tank state updates and moving-valve phase from glue/fluids.
use super::*;
impl Run {
    /// fluids::Tanks::send_tank_changes, after logistics/tools/arms and before Solve.
    /// Drawing NBT is retained by the host, but is not a fluid load/restore input.
    pub(super) fn save_fluid_entities(&mut self) {
        let mut changed = std::mem::take(&mut self.w.dims[0].tank_changes);
        changed.sort();
        changed.dedup();
        for p in changed {
            self.w.dims[0].analog_checks.push(p);
            if self.w.block_at(0, p).is_some() {
                let m = self.w.dims[0].tanks.multis.get(&p).copied();
                self.persisted
                    .fluid_entities
                    .insert(p, crate::persistence::fluids::shown_tank(m));
            }
        }
    }
    pub(super) fn tick_fluids(&mut self) {
        let open: HashMap<_, _> = self.w.dims[0]
            .plumbing
            .valves
            .keys()
            .map(|&p| (p, self.w.table.get(self.at(p)).map(|(_, i)| i.enabled)))
            .collect();
        let changes = self.w.dims[0]
            .plumbing
            .tick_valves(|p| open.get(&p).copied().flatten());
        for (p, enabled) in changes {
            let next = with_prop(
                &self.name(self.at(p)),
                "enabled",
                if enabled { "true" } else { "false" },
            );
            self.switch(p, self.id(&next));
        }
        for p in self.w.take_tank_states(0) {
            if let Some(shape) = self.w.tank_shape(0, p) {
                let mut next = self.name(self.at(p));
                for (k, v) in [
                    ("top", if shape.top { "true" } else { "false" }),
                    ("bottom", if shape.bottom { "true" } else { "false" }),
                    ("shape", shape.shape.name()),
                ] {
                    next = with_prop(&next, k, v);
                }
                self.set(p, self.id(&next));
            }
        }
    }
    pub(super) fn fluid_use(&mut self, p: Pos, item: &str, sneaking: bool) -> bool {
        if self.w.block_at(0, p) != Some(create_engine::Block::FluidTank) {
            return false;
        }
        if item == "create:wrench" {
            if sneaking {
                self.set(p, AIR);
            } else {
                self.w.toggle_tank_windows(0, p);
            }
            return true;
        }
        false
    }
    pub(super) fn fluid_tank(&self, p: Pos) -> Option<crate::fluids::Tank> {
        let (controller, m) = self.w.tank(0, p)?;
        Some(crate::fluids::Tank {
            controller,
            width: m.width,
            height: m.height,
            capacity: m.capacity(),
            fluid: m.tank.fluid.map(|f| f.key()),
            amount: m.tank.amount,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{codec::Tag, fluids as saved};
    use create_engine::tank::{Fluid, Tank};

    fn tank_case() -> Scenario {
        serde_json::from_value(serde_json::json!({"version":3,"ticks":20,"blocks":[
            {"pos":[15,8,0],"state":"create:fluid_tank"},
            {"pos":[15,8,1],"state":"create:fluid_tank"},
            {"pos":[16,8,0],"state":"create:fluid_tank"},
            {"pos":[16,8,1],"state":"create:fluid_tank"}
        ]}))
        .unwrap()
    }

    #[test]
    fn filled_multiblock_survives_repeated_cross_column_reloads() {
        let sc = tank_case();
        let mut r = Run::new(&sc);
        for b in &sc.blocks {
            r.place(b);
        }
        let p = [15, 8, 0];
        assert_eq!(r.w.fill_tank(0, p, Fluid::Water, 17000), 17000);
        assert!(r.w.toggle_tank_windows(0, p));
        r.tick();
        for _ in 0..2 {
            r.reload();
            for b in &sc.blocks {
                let (c, m) = r.w.tank(0, b.pos).unwrap();
                assert_eq!(c, p);
                assert_eq!(
                    (m.width, m.height, m.window, m.tank),
                    (
                        2,
                        1,
                        false,
                        Tank {
                            fluid: Some(Fluid::Water),
                            amount: 17000
                        }
                    )
                );
            }
            r.tick();
            assert!(r.name(r.at([16, 8, 1])).contains("shape=plain"));
        }
        // A drained, single, windowed tank uses no column record; stale multi data must go.
        for p in [[15, 8, 1], [16, 8, 0], [16, 8, 1]] {
            r.set(p, AIR);
        }
        r.w.set_tank(0, p, Tank::default());
        r.w.toggle_tank_windows(0, p);
        r.tick();
        assert!(!r.persisted.fluid_columns.contains_key(&(0, 0)));
        r.reload();
        let (_, m) = r.w.tank(0, p).unwrap();
        assert_eq!(m, create_engine::tank::Multi::default());
    }

    #[test]
    fn reload_uses_published_column_not_live_or_drawing_data() {
        let sc = tank_case();
        let mut r = Run::new(&sc);
        for b in &sc.blocks {
            r.place(b);
        }
        let p = [15, 8, 0];
        r.w.fill_tank(0, p, Fluid::Water, 1000);
        r.tick();
        let shown = serde_json::to_vec(&r.persisted.fluid_entities[&p]).unwrap();
        r.w.fill_tank(0, p, Fluid::Water, 2000);
        // A reload is not an extra serialization operation; even drawing NBT can be newer
        // than the attachment, and KineticEntity::block_entity_on_load never reads Tank.
        r.save_fluid_entities();
        assert_ne!(
            serde_json::to_vec(&r.persisted.fluid_entities[&p]).unwrap(),
            shown
        );
        r.flush_persistence(); // no marked columns: the unsaved journal awaits Solve
        r.reload();
        assert_eq!(r.w.tank(0, p).unwrap().1.tank.amount, 1000);
        r.w.fill_tank(0, p, Fluid::Water, 2000);
        r.tick();
        r.reload();
        assert_eq!(r.w.tank(0, p).unwrap().1.tank.amount, 3000);
    }

    #[test]
    fn fluid_columns_keep_glue_machine_and_plumbing_save_semantics() {
        let mut r = Run::new(&tank_case());
        let d = &mut r.w.dims[0];
        d.plumbing.hose_saved.insert(
            [1, 8, 0],
            (
                2.1259,
                Tank {
                    fluid: Some(Fluid::Water),
                    amount: 1200,
                },
            ),
        );
        d.plumbing.filters.insert(
            [2, 8, 0],
            create_engine::fluids::SmartFilter {
                item: "minecraft:water_bucket".into(),
                fluid: Some(Fluid::Water),
            },
        );
        d.plumbing
            .creative_saved
            .insert([3, 8, 0], Some(Fluid::Lava));
        d.spouts.insert(
            [4, 8, 0],
            create_engine::spout::Spout {
                fluid: Some((Fluid::Water, 750)),
                processing: 8,
            },
        );
        d.drains.insert(
            [5, 8, 0],
            create_engine::drain::Drain {
                fluid: Some((Fluid::Water, 500)),
                processing: 7,
                held: Some(create_engine::drain::Held {
                    item: "minecraft:water_bucket".into(),
                    count: 1,
                    position: 0.375,
                    prev: 0.25,
                    from: create_engine::kinetics::Dir::East,
                }),
            },
        );
        d.open_ends.insert(
            ([6, 8, 0], create_engine::kinetics::Dir::North),
            create_engine::open_end::OpenEnd {
                tank: Some((Fluid::Water, 900)),
                pulling: true,
            },
        );
        let data = saved::save_column(&r.w, (0, 0));
        let encoded = serde_json::to_vec(&data).unwrap();
        r.w.dims[0] = Default::default();
        saved::load_column(&mut r.w, &serde_json::from_slice(&encoded).unwrap());
        let d = &r.w.dims[0];
        assert_eq!(d.plumbing.hose_saved[&[1, 8, 0]].0, 2.125);
        assert_eq!(
            d.plumbing.filters[&[2, 8, 0]].item,
            "minecraft:water_bucket"
        );
        assert_eq!(d.plumbing.creative_saved[&[3, 8, 0]], Some(Fluid::Lava));
        assert_eq!(d.spouts[&[4, 8, 0]].fluid, Some((Fluid::Water, 750)));
        assert_eq!(d.spouts[&[4, 8, 0]].processing, -1); // glue persists no spout cycle
        let dr = &d.drains[&[5, 8, 0]];
        assert_eq!(dr.processing, 7);
        assert_eq!(dr.held.as_ref().unwrap().prev, 0.375);
        assert_eq!(
            d.open_ends[&([6, 8, 0], create_engine::kinetics::Dir::North)].tank,
            Some((Fluid::Water, 900))
        );
        assert!(matches!(data.get("create_fluids"), Some(Tag::Compound(_))));
    }
    #[test]
    fn moving_valve_commits_a_registered_enabled_state() {
        let mut sc = crate::fluids::generate(8);
        sc.blocks.clear();
        sc.events.clear();
        let mut run = Run::new(&sc);
        let p = [0, 4, 0];
        let id = run.id("create:fluid_valve[axis_along_first=false,enabled=false,facing=north,waterlogged=false]");
        run.set(p, id);
        run.w.dims[0].plumbing.valve_speed(p, 64.);
        for _ in 0..6 {
            run.tick_fluids();
        }
        assert!(run.name(run.at(p)).contains("enabled=true"));
        assert_eq!(run.w.block_at(0, p), Some(create_engine::Block::FluidValve));
    }
}
