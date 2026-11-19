//! Generating kinetic sources' environment: water wheels' flow. The native fluid host supplies
//! the fluid states and the flow vectors they carry.
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
}

/// The offsets a wheel of this axis reads (`WaterWheelBlockEntity.getOffsetsToCheck`).
fn offsets(axis: Axis, large: bool) -> Vec<[i32; 3]> {
    if large {
        create_engine::waterwheel::large_offsets(axis)
    } else {
        create_engine::waterwheel::small_offsets(axis).to_vec()
    }
}
