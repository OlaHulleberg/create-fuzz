use super::*;

impl Run {
    /// One level tick: inputs, then the solve and what it breaks.
    pub(super) fn tick(&mut self) {
        self.w.tick_cranks(0);
        for h in self.handles.values_mut() {
            h.remaining = h.remaining.saturating_sub(1);
        }
        for p in self.w.take_redstone(0) {
            let state = self.at(p);
            let Some(was) = self.w.powered(state) else {
                continue;
            };
            let now = self.signal(p) > 0;
            if now != was {
                let next = with_prop(
                    &self.name(state),
                    "powered",
                    if now { "true" } else { "false" },
                );
                let id = self.id(&next);
                // The glue's `tick_inputs`: GearshiftBlock.neighborChanged detaches first.
                self.w.transmission_power_changing(0, p);
                // ChainGearshiftBlock.neighborChanged sets with UPDATE_KNOWN_SHAPE.
                if self.w.reads_signal(state) {
                    self.w.setting_known_shape(0, p, id);
                }
                self.commit(p, id, false);
            }
            if self.w.reads_signal(state) {
                let signal = i32::from(self.signal(p));
                self.w.set_chain_signal(0, p, signal);
                self.chain_signals.insert(p, signal);
            }
        }
        for (p, drive) in self.w.take_chain_shapes(0) {
            let next = with_prop(
                &self.name(self.at(p)),
                "axis_along_first",
                if drive.along_first { "true" } else { "false" },
            );
            self.set(p, self.id(&next));
            let next = with_prop(&self.name(self.at(p)), "part", drive.part.name());
            self.set(p, self.id(&next));
        }
        let mut steps = Vec::new();
        for (p, running) in self.w.take_sequencer_checks(0) {
            let powered = self.signal(p) > 0;
            if let Some(step) = self.w.sequencer_redstone(0, p, powered, running) {
                steps.push((p, step));
            }
        }
        for (p, step) in steps {
            self.sequencer_step(p, step);
        }
        let mut events = Vec::new();
        self.belt_breaks();
        self.tick_transport();
        self.w.settle_belts(0);
        // CreateWorld::tick: the solve, then sequencer instructions (committed after it).
        let (_, steps) = self.w.tick(0, &mut events);
        for (p, step) in steps {
            self.sequencer_step(p, step);
        }
        self.w.dims[0].kinetics.take_removed();
        for e in events {
            if let create_engine::kinetics::Event::Break { pos } = e {
                self.set(pos, AIR);
            }
        }
    }

    /// The glue's commit of a sequencer step: its `state` (kept while powered at the end).
    pub(super) fn sequencer_step(&mut self, p: [i32; 3], step: create_engine::sequencer::Step) {
        use create_engine::sequencer::Step;
        let n = match step {
            Step::Run(n) => n,
            Step::Idle => 0,
            Step::Ended if self.signal(p) > 0 => return,
            Step::Ended => 0,
        };
        let next = with_prop(&self.name(self.at(p)), "state", &n.to_string());
        self.set(p, self.id(&next));
    }
}
