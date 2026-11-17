//! Pump/flow phase from glue/fluids/{pipes,flows}. The engine keeps endpoint reach and tank
//! contents; this adapter supplies host callbacks and the glue's transient flow cache.
use super::*;
use create_engine::{
    Block,
    fluids::{self, Cell, End, FlowAt, Step},
    kinetics::Dir,
    tank::Fluid,
};
#[derive(Clone)]
pub(super) struct Running {
    pub fluid: Fluid,
    pub start: usize,
    pub stop: Option<usize>,
    pub flows: Vec<FlowAt>,
}
impl Run {
    pub(super) fn pipe_cell_toward(&self, p: Pos, _toward: Dir) -> Cell {
        if self.w.table.get(self.at(p)).is_some() {
            return self.pipe_cell(p);
        }
        let state = self.name(self.at(p));
        let (name, props) = parse_state(&state);
        if name.ends_with("cauldron") || props.iter().any(|(k, _)| k == "honey_level") {
            return Cell::Open;
        }
        if self.sturdy(p, _toward.opposite()) {
            return Cell::Solid;
        }
        if self.replaceable(p) || self.waterloggable(p) {
            Cell::Open
        } else {
            Cell::Solid
        }
    }
    pub(super) fn tick_pumps(&mut self) {
        let changed = std::mem::replace(&mut self.w.dims[0].pipes_changed, false);
        let pumps = self.w.dims[0].pumping.clone();
        self.fluid_pressures.clear();
        for &pos in &pumps {
            self.pump(pos, changed);
        }
        self.fluid_runs.retain(|p, _| pumps.contains(p));
        let now = self.persisted.clock;
        self.fluid_runs.retain(|_, r| {
            r.stop
                .is_none_or(|stop| r.flows.iter().any(|f| now <= stop + f.order as usize))
        });
        let mut positions: Vec<_> = self
            .fluid_runs
            .values()
            .flat_map(|r| r.flows.iter().filter(|f| f.inbound).map(|f| f.pos))
            .collect();
        positions.sort();
        positions.dedup();
        for p in positions {
            let mut sides = self.fluid_provided(p);
            sides.sort_by_key(|(d, _)| Dir::ALL.iter().position(|x| x == d));
            if let Some((_, first)) = sides.first().copied()
                && let Some((_, other)) = sides.iter().find(|(_, f)| *f != first).copied()
            {
                self.set(p, AIR);
                if let Some(state) = create_engine::open_end::flow_collision(first, other) {
                    self.set(p, self.id(state));
                }
            }
        }
    }
    fn pump(&mut self, pos: Pos, changed: bool) {
        let speed = self.w.speed(0, pos).unwrap_or(0.);
        let Some(facing) = self.w.table.get(self.at(pos)).and_then(|(_, i)| i.facing) else {
            return;
        };
        let Some(mut pump) = self.w.dims[0].pumps.get(&pos).cloned() else {
            return;
        };
        if changed || !pump.found {
            let cell = |p, d| self.pipe_cell_toward(p, d);
            let pull = fluids::reach_toward(pos, facing.opposite(), speed, cell);
            let push = fluids::reach_toward(pos, facing, speed, cell);
            let ways_pull = fluids::ways_toward(pos, facing.opposite(), cell);
            let ways_push = fluids::ways_toward(pos, facing, cell);
            let same = pump.found
                && pump.pull == pull
                && pump.push == push
                && pump.ways_pull == ways_pull
                && pump.ways_push == ways_push;
            if !same {
                pump.pull = pull;
                pump.push = push;
                pump.ways_pull = ways_pull;
                pump.ways_push = ways_push;
                pump.pumped = 0;
                self.fluid_runs.remove(&pos);
            }
            pump.found = true;
        }
        self.pressure_paths(pos, facing, speed, &pump);
        let from = pump.pull.iter().find(|e| !e.blocked).copied();
        let mut flowing = false;
        if let Some(from) = from
            && !pump.push.is_empty()
        {
            let amount = fluids::transfer_speed(from.pressure);
            let offer = self
                .fluid_offer(&from, amount)
                .filter(|(f, _)| from.passes(*f));
            if let Some((fluid, n)) = offer {
                flowing = true;
                if self
                    .fluid_runs
                    .get(&pos)
                    .filter(|r| r.stop.is_none())
                    .map(|r| r.fluid)
                    != Some(fluid)
                {
                    pump.pumped = 0;
                    let pull = pump
                        .ways_pull
                        .iter()
                        .find(|(end, _)| *end == from.pos)
                        .map_or(&[][..], |(_, w)| w.as_slice());
                    let push: Vec<(End, Vec<Step>)> = pump
                        .push
                        .iter()
                        .filter_map(|e| {
                            Some((
                                *e,
                                pump.ways_push
                                    .iter()
                                    .find(|(end, _)| *end == e.pos)?
                                    .1
                                    .clone(),
                            ))
                        })
                        .collect();
                    let flows = fluids::flows(pos, facing, speed, &from, pull, &push, fluid);
                    self.fluid_runs.insert(
                        pos,
                        Running {
                            fluid,
                            start: self.persisted.clock,
                            stop: None,
                            flows,
                        },
                    );
                }
                pump.pumped = pump.pumped.saturating_add(1);
                let push = std::mem::take(&mut pump.push);
                let pumped = pump.pumped;
                let reached = |to: &&End| {
                    to.passes(fluid) && pumped >= fluids::arrival_ticks(&from, to, speed)
                };
                let count = push.iter().filter(reached).count();
                let moved = fluids::share(count, n, |k, give| {
                    push.iter()
                        .filter(reached)
                        .nth(k)
                        .map_or(0, |to| self.fluid_fill(to, fluid, give))
                });
                if moved > 0 {
                    self.fluid_take(&from, fluid, moved);
                }
                pump.push = push;
            }
        }
        if !flowing {
            pump.pumped = 0;
            if let Some(run) = self.fluid_runs.get_mut(&pos)
                && run.stop.is_none()
            {
                run.stop = Some(self.persisted.clock);
            }
        }
        self.w.dims[0].pumps.insert(pos, pump);
    }
    pub(super) fn fluid_offer(&mut self, from: &End, amount: u32) -> Option<(Fluid, u32)> {
        if !from.tank {
            return self.open_end(from, |open, front| open.drain(front, amount, None, false));
        }
        if let Some(fluid) = self.w.dims[0].plumbing.creative_fluid(from.pos) {
            return fluid.map(|f| (f, amount));
        }
        if self.w.is_boiler(0, from.pos) {
            return None;
        }
        if let Some(d) = self.w.dims[0].drains.get(&from.pos) {
            return d.fluid.map(|(f, n)| (f, n.min(amount)));
        }
        self.w
            .tank(0, from.pos)
            .and_then(|(_, m)| m.tank.fluid.map(|f| (f, m.tank.amount.min(amount))))
    }
    pub(super) fn fluid_take(&mut self, from: &End, fluid: Fluid, amount: u32) {
        if !from.tank {
            self.open_end(from, |open, front| {
                open.drain(front, amount, Some(fluid), true)
            });
            return;
        }
        if self.w.dims[0].plumbing.creative_fluid(from.pos).is_some() {
            return;
        }
        if let Some(d) = self.w.dims[0].drains.get_mut(&from.pos) {
            create_engine::drain::drain(d, amount);
            return;
        }
        self.w.drain_tank(0, from.pos, amount);
    }
    pub(super) fn fluid_fill(&mut self, to: &End, fluid: Fluid, amount: u32) -> u32 {
        if !to.tank {
            return self.open_end(to, |open, front| open.fill(front, fluid, amount, true));
        }
        if self.w.dims[0].plumbing.creative.contains_key(&to.pos) {
            return amount;
        }
        if let Some(took) = self.w.fill_boiler(0, to.pos, fluid, amount) {
            return took;
        }
        if let Some(s) = self.w.dims[0].spouts.get_mut(&to.pos) {
            return create_engine::spout::fill(s, fluid, amount);
        }
        self.w.fill_tank(0, to.pos, fluid, amount)
    }
    fn pressure_paths(
        &mut self,
        pos: Pos,
        facing: Dir,
        speed: f32,
        pump: &create_engine::world::Pump,
    ) {
        // The port retains endpoint pressures, not Java's per-connection pressure objects.
        // Project endpoint pressures onto the first breadth-first path; raw stored
        // connection pressures do not exist in the port's model.
        // Functional flow/transfer differences are triaged separately from this abstraction gap.
        self.fluid_pressures
            .insert((pos, facing.opposite()), [speed.abs(), 0.]);
        self.fluid_pressures
            .insert((pos, facing), [0., speed.abs()]);
        for (ends, ways, pull) in [
            (&pump.pull, &pump.ways_pull, true),
            (&pump.push, &pump.ways_push, false),
        ] {
            for end in ends {
                if let Some((_, way)) = ways.iter().find(|(p, _)| *p == end.pos) {
                    for step in way {
                        for (side, inbound) in [(step.on, pull), (step.back, !pull)] {
                            let entry = self.fluid_pressures.entry((step.pos, side)).or_default();
                            let channel = &mut entry[usize::from(!inbound)];
                            if *channel == 0. {
                                *channel = end.pressure;
                            }
                        }
                    }
                }
            }
        }
    }
    pub(super) fn fluid_provided(&self, p: Pos) -> Vec<(Dir, Fluid)> {
        let mut runs: Vec<_> = self.fluid_runs.iter().collect();
        runs.sort_by_key(|(p, _)| **p);
        let mut seen = 0;
        let mut out = Vec::new();
        for (_, run) in runs {
            for f in run.flows.iter().filter(|f| f.pos == p) {
                if seen & f.side.bit() != 0 {
                    continue;
                }
                seen |= f.side.bit();
                if f.inbound
                    && run
                        .stop
                        .is_none_or(|stop| self.persisted.clock < stop + f.order as usize)
                    && f.progress((self.persisted.clock - run.start) as f32) >= 1.
                {
                    out.push((f.side, run.fluid));
                }
            }
        }
        out
    }
    pub(super) fn fluid_network(&self, p: Pos) -> Option<crate::fluid_network::Observation> {
        use crate::fluid_network::{Connection, Flight, Observation};
        let block = self.w.block_at(0, p)?;
        if !matches!(
            block,
            Block::FluidPipe
                | Block::EncasedFluidPipe
                | Block::GlassFluidPipe
                | Block::MechanicalPump
                | Block::FluidValve
                | Block::SmartFluidPipe
        ) {
            return None;
        }
        let mut connections = BTreeMap::new();
        for side in Dir::ALL {
            {
                connections.insert(
                    format!("{side:?}").to_lowercase(),
                    Connection {
                        pressure: self
                            .fluid_pressures
                            .get(&(p, side))
                            .copied()
                            .unwrap_or_default(),
                        flow: None,
                        buffer: self.w.dims[0]
                            .open_ends
                            .get(&(p, side))
                            .and_then(|e| e.tank)
                            .map(|(f, n)| crate::fluid_network::Buffer {
                                fluids: BTreeMap::from([(f.key(), n)]),
                            })
                            .unwrap_or_default(),
                    },
                );
            }
        }
        let mut runs: Vec<_> = self.fluid_runs.iter().collect();
        runs.sort_by_key(|(p, _)| **p);
        let mut seen = 0;
        for (_, run) in runs {
            for f in run.flows.iter().filter(|f| f.pos == p) {
                if seen & f.side.bit() != 0 {
                    continue;
                }
                seen |= f.side.bit();
                let elapsed = (self.persisted.clock - run.start) as f32;
                if elapsed < f.start
                    || run
                        .stop
                        .is_some_and(|stop| self.persisted.clock >= stop + f.order as usize)
                {
                    continue;
                }
                let progress = f.progress(elapsed);
                connections
                    .entry(format!("{:?}", f.side).to_lowercase())
                    .or_default()
                    .flow = Some(Flight {
                    fluid: run.fluid.key(),
                    inbound: f.inbound,
                    progress,
                    complete: progress >= 1.,
                });
            }
        }
        let pump_pressure = (block == Block::MechanicalPump).then(|| {
            connections
                .values()
                .flat_map(|c| c.pressure)
                .fold(0., f32::max)
        });
        Some(Observation {
            pressure_model: "endpoint_paths".into(),
            connections,
            pump_pressure,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cauldron_transfer_conserves_fluid_without_native_peer() {
        for reload in [false, true] {
            let mut sc = crate::fluid_pumps::generate(18);
            if reload {
                sc.events.push(crate::scenario::Change {
                    tick: 99,
                    pos: [2, 4, 0],
                    state: None,
                    motor: None,
                    turn: None,
                    action: Some(crate::actions::Action::Reload),
                });
            }
            let mut run = Run::new(&sc);
            run.fluid_host_enabled = false;
            let snapshot = super::super::execute(&sc, run);
            assert!(snapshot.notes.is_empty());
            let mut first = None;
            for (&tick, snap) in &snapshot.checkpoints {
                assert!(
                    snap.blocks.values().all(|b| b.world_fluid.is_none()),
                    "this fixture has no native fluid dynamics"
                );
                if tick < 61 {
                    continue;
                }
                let source = if snap.blocks["0,4,0"].state == "minecraft:water_cauldron[level=3]" {
                    1000
                } else {
                    0
                };
                let held = snap.blocks["4,4,0"].fluid_tank.as_ref().unwrap().amount;
                let buffer = snap.blocks["1,4,0"]
                    .fluid_network
                    .as_ref()
                    .unwrap()
                    .connections["west"]
                    .buffer
                    .amount();
                assert_eq!(
                    source + held + buffer,
                    1000,
                    "conservation at {tick}, reload={reload}"
                );
                if held > 0 && first.is_none() {
                    first = Some(tick);
                }
            }
            assert_eq!(
                snapshot.blocks["4,4,0"].fluid_tank.as_ref().unwrap().amount,
                1000
            );
            if !reload {
                let p = [2, 4, 0];
                let sides = Dir::West.bit() | Dir::East.bit();
                let cell = |pos: Pos, _| match pos {
                    [0, 4, 0] => Cell::Open,
                    [1, 4, 0] | [3, 4, 0] => Cell::Pipe(sides),
                    [2, 4, 0] => Cell::Pump(Dir::East),
                    [4, 4, 0] => Cell::Tank,
                    _ => Cell::Solid,
                };
                let from = fluids::reach_toward(p, Dir::West, 32., cell)[0];
                let to = fluids::reach_toward(p, Dir::East, 32., cell)[0];
                let pull = fluids::ways_toward(p, Dir::West, cell);
                let push = fluids::ways_toward(p, Dir::East, cell);
                let raw = fluids::flows(
                    p,
                    Dir::East,
                    32.,
                    &from,
                    &pull[0].1,
                    &[(to, push[0].1.clone())],
                    Fluid::Water,
                );
                let last = raw
                    .iter()
                    .find(|f| f.pos == [3, 4, 0] && f.side == Dir::East)
                    .unwrap();
                let at = first.unwrap();
                let progress = last.progress((at - 61) as f32);
                let observed = snapshot.checkpoints[&at].blocks["3,4,0"]
                    .fluid_network
                    .as_ref()
                    .unwrap()
                    .connections["east"]
                    .flow
                    .as_ref()
                    .unwrap()
                    .progress;
                assert!((observed - progress).abs() < 1e-6);
                println!(
                    "Raw engine arrival {} ticks; first fill {at}; raw/observed last-half progress {progress}",
                    fluids::arrival_ticks(&from, &to, 32.)
                );
            }
            println!(
                "Rust-only cauldron replay: reload={reload}; first fill {first:?}; final 1000 mB; conserved at every checkpoint; no native peer"
            );
        }
    }
}

#[cfg(test)]
mod conversion_fidelity {
    use super::*;
    #[test]
    fn live_conversion_buffer_follows_world_block_change_api() {
        let sc = crate::fluid_pumps::generate(1554);
        let fill=sc.events.iter().find(|e|matches!(&e.action,Some(crate::actions::Action::Use{item,..})if item=="minecraft:water_bucket")).unwrap().tick;
        let mut run = Run::new(&sc);
        run.fluid_host_enabled = false;
        let snapshot = super::super::execute(&sc, run);
        let before = &snapshot.checkpoints[&(fill + 40)].blocks["1,4,0"]
            .fluid_network
            .as_ref()
            .unwrap()
            .connections["west"]
            .buffer;
        let after = &snapshot.checkpoints[&(fill + 41)].blocks["1,4,0"]
            .fluid_network
            .as_ref()
            .unwrap()
            .connections["west"]
            .buffer;
        assert!(before.amount() > 0);
        let mut raw = Run::new(&sc);
        let regular=raw.id("create:fluid_pipe[down=false,east=true,north=false,south=false,up=false,waterlogged=false,west=true]");
        let glass = raw.id("create:glass_fluid_pipe[alt=false,axis=x,waterlogged=false]");
        raw.w.block_changed(0, [1, 4, 0], AIR, regular);
        raw.w.dims[0].open_ends.insert(
            ([1, 4, 0], Dir::West),
            create_engine::open_end::OpenEnd {
                tank: Some((Fluid::Water, before.amount())),
                pulling: true,
            },
        );
        raw.w.block_changed(0, [1, 4, 0], regular, glass);
        let expected = raw.w.dims[0]
            .open_ends
            .get(&([1, 4, 0], Dir::West))
            .and_then(|e| e.tank)
            .map_or(0, |(_, n)| n);
        let tank_before = snapshot.checkpoints[&(fill + 40)].blocks["4,4,0"]
            .fluid_tank
            .as_ref()
            .unwrap()
            .amount;
        let tank_after = snapshot.checkpoints[&(fill + 41)].blocks["4,4,0"]
            .fluid_tank
            .as_ref()
            .unwrap()
            .amount;
        assert_eq!(after.amount() + tank_after - tank_before, expected);
        println!(
            "Rust-only live conversion: buffer before {}, after {} (direct engine block-change API {}); final tank {} mB",
            before.amount(),
            after.amount(),
            expected,
            snapshot.blocks["4,4,0"].fluid_tank.as_ref().unwrap().amount
        );
    }
}
