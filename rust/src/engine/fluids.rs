//! The tank state updates and moving-valve phase from glue/fluids.
use super::*;
impl Run {
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
