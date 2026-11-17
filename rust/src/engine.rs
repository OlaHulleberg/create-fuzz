//! Runs a scenario through create-horizon's engine the way its game glue does each tick
//! (`create/src/glue/kinetics.rs`): inputs (split shafts following redstone), then
//! `settle_belts`, the solve, and the blocks the solve breaks.
//!
//! The world outside the engine is a map of positions to state strings. Redstone is only what
//! the scenarios use: a `minecraft:redstone_block` powers the blocks beside it.

use crate::scenario::{
    BlockSnap, Kinetic, Placed, Pos, Scenario, Snapshot, key, parse_state, with_prop,
};
use create_engine::parts::{Settings, StateInfo, parts};
use create_engine::states;
use create_engine::world::{CreateWorld, StateTable};
use std::collections::{BTreeMap, HashMap};

/// Create's state ids start here; below are the few vanilla ones the scenarios use.
const FIRST: u32 = 16;
const AIR: u32 = 0;
const REDSTONE_BLOCK: u32 = 1;
const OTHER: u32 = 2;

const DIRS: [Pos; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// Every Create state the scenario can reach, with both `powered` values for the states that
/// have it (the driver flips them as redstone changes).
fn create_states(sc: &Scenario) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: String| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    let all: Vec<Placed> = sc
        .blocks
        .iter()
        .cloned()
        .chain(sc.events.iter().filter_map(|e| e.placed()))
        .collect();
    for b in &all {
        if !b.state.starts_with("create:") {
            continue;
        }
        let (_, props) = parse_state(&b.state);
        push(normal(&b.state));
        if props.iter().any(|(k, _)| k == "powered") {
            push(normal(&with_prop(&b.state, "powered", "true")));
            push(normal(&with_prop(&b.state, "powered", "false")));
        }
    }
    out
}

/// A state string with its properties sorted, so equal states compare equal.
fn normal(s: &str) -> String {
    let (name, props) = parse_state(s);
    crate::scenario::format_state(&name, &props)
}

pub struct Run {
    w: CreateWorld,
    ids: HashMap<String, u32>,
    names: Vec<String>,
    world: HashMap<Pos, u32>,
    notes: Vec<String>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

/// The engine's state range is one static per process: one run at a time.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

impl Run {
    pub fn new(sc: &Scenario) -> Run {
        crate::catalogue::scenario(sc);
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let names = create_states(sc);
        let parsed: Vec<(String, Vec<(String, String)>)> =
            names.iter().map(|s| parse_state(s)).collect();
        states::set_range(FIRST, names.len() as u32);
        let table = StateTable::new(
            FIRST,
            parsed
                .iter()
                .map(|(n, p)| (n.as_str(), p.iter().map(|(k, v)| (k.as_str(), v.as_str())))),
        );
        let ids = names
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), FIRST + i as u32))
            .collect();
        Run {
            w: CreateWorld::new(table),
            ids,
            names,
            world: HashMap::new(),
            notes: Vec::new(),
            _guard: guard,
        }
    }

    fn id(&self, state: &str) -> u32 {
        if state == "minecraft:air" {
            return AIR;
        }
        if state == "minecraft:redstone_block" {
            return REDSTONE_BLOCK;
        }
        self.ids.get(&normal(state)).copied().unwrap_or(OTHER)
    }

    fn name(&self, id: u32) -> String {
        match id {
            AIR => "minecraft:air".into(),
            REDSTONE_BLOCK => "minecraft:redstone_block".into(),
            OTHER => "minecraft:stone".into(),
            _ => self.names[(id - FIRST) as usize].clone(),
        }
    }

    fn at(&self, p: Pos) -> u32 {
        self.world.get(&p).copied().unwrap_or(AIR)
    }

    /// A committed block change, then its neighbours told (`neighborChanged`).
    fn set(&mut self, p: Pos, new: u32) {
        let old = self.at(p);
        if old == new {
            return;
        }
        if new == AIR {
            self.world.remove(&p);
        } else {
            self.world.insert(p, new);
        }
        self.w.block_changed(0, p, old, new);
        let mut fallen = Vec::new();
        for d in DIRS {
            let q = add(p, d);
            let s = self.at(q);
            if s != AIR {
                self.w.neighbor_changed(0, q, s);
                if new == AIR && self.hangs_on(q, s, p) {
                    fallen.push(q);
                }
            }
        }
        for q in fallen {
            self.set(q, AIR);
        }
    }

    /// The glue's `HandCrankBlock.neighborChanged` (`glue/sources/handles.rs`): a crank whose
    /// block behind it is gone breaks. Not the engine's rule, so mirrored here.
    fn hangs_on(&self, q: Pos, state: u32, behind: Pos) -> bool {
        let name = self.name(state);
        let (block, props) = parse_state(&name);
        if block != "create:hand_crank" && !block.ends_with("_valve_handle") {
            return false;
        }
        let Some((_, facing)) = props.iter().find(|(k, _)| k == "facing") else {
            return false;
        };
        let back = match facing.as_str() {
            "up" => [0, -1, 0],
            "down" => [0, 1, 0],
            "north" => [0, 0, 1],
            "south" => [0, 0, -1],
            "east" => [-1, 0, 0],
            _ => [1, 0, 0],
        };
        add(q, back) == behind
    }

    fn place(&mut self, b: &Placed) {
        let id = self.id(&b.state);
        if id == OTHER && b.state.starts_with("create:") {
            self.notes.push(format!("unknown state {}", b.state));
        }
        self.set(b.pos, id);
    }

    /// A player setting the motor's value board (`set_motor`), the tick after placing it.
    fn set_motor(&mut self, b: &Placed) {
        let Some(rpm) = b.motor else { return };
        let id = self.at(b.pos);
        if id != self.id(&b.state) {
            return;
        }
        let settings = Settings {
            motor_rpm: rpm as f32,
            ..Settings::default()
        };
        self.w.set_settings(0, b.pos, id, settings);
    }

    fn powered_by_neighbour(&self, p: Pos) -> bool {
        DIRS.iter().any(|d| self.at(add(p, *d)) == REDSTONE_BLOCK)
    }

    /// One level tick: inputs, then the solve and what it breaks.
    fn tick(&mut self) {
        self.w.tick_cranks(0);
        for p in self.w.take_redstone(0) {
            let state = self.at(p);
            let Some(was) = self.w.powered(state) else {
                continue;
            };
            let now = self.powered_by_neighbour(p);
            if now != was {
                let next = with_prop(
                    &self.name(state),
                    "powered",
                    if now { "true" } else { "false" },
                );
                let id = self.id(&next);
                self.set(p, id);
            }
            if self.w.reads_signal(state) {
                self.w.set_chain_signal(0, p, if now { 15 } else { 0 });
            }
        }
        let shapes = self.w.take_chain_shapes(0);
        if !shapes.is_empty() {
            self.notes
                .push(format!("chain shapes not applied: {}", shapes.len()));
        }
        let mut events = Vec::new();
        self.w.settle_belts(0);
        self.w.tick(0, &mut events);
        self.w.dims[0].kinetics.take_removed();
        for e in events {
            if let create_engine::kinetics::Event::Break { pos } = e {
                self.set(pos, AIR);
            }
        }
    }

    pub fn snapshot(&self, sc: &Scenario) -> Snapshot {
        let mut blocks = BTreeMap::new();
        let mut groups: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        let mut sizes = BTreeMap::new();
        for p in sc.region() {
            let id = self.at(p);
            let state = self.name(id);
            let kinetic = self.kinetic(p, id);
            if kinetic.is_some()
                && let Some((network, size)) = self.w.dims[0]
                    .kinetics
                    .network_membership(p, self.observed_sub(p))
            {
                groups.entry(network).or_default().push(key(p));
                sizes.insert(
                    key(p),
                    i32::try_from(size).expect("network size fits Java int"),
                );
            }
            blocks.insert(
                key(p),
                BlockSnap {
                    state,
                    kinetic,
                    debug: None,
                },
            );
        }
        let mut partitions: Vec<_> = groups.into_values().collect();
        for group in &mut partitions {
            group.sort();
        }
        partitions.sort();
        Snapshot {
            side: "rust".into(),
            unavailable: Vec::new(),
            networks: Some(crate::observation::Networks { partitions, sizes }),
            blocks,
            notes: self.notes.clone(),
            checkpoints: BTreeMap::new(),
        }
    }

    /// Match the Java block entity's driven half for split shafts. Use the same
    /// part for kinetic figures, network membership and network size observations.
    fn observed_sub(&self, p: Pos) -> u8 {
        let k = &self.w.dims[0].kinetics;
        u8::from(
            k.network(p, 0).is_some_and(|n| n.2 == 0.0)
                && k.network(p, 1).is_some_and(|n| n.2 > 0.0),
        )
    }

    fn kinetic(&self, p: Pos, id: u32) -> Option<Kinetic> {
        let k = &self.w.dims[0].kinetics;
        let (block, info): (create_engine::Block, &StateInfo) = self.w.table.get(id)?;
        let motor_rpm = self
            .w
            .column_settings(0, p[0] >> 4, p[2] >> 4)
            .into_iter()
            .find(|(q, _)| *q == p)
            .map_or(16.0, |(_, rpm)| rpm as f32);
        let settings = Settings {
            motor_rpm,
            steam: self.w.shaft_steam(0, p),
            ..Settings::default()
        };
        let both = parts(block, info, &settings);
        // A split shaft is one block entity in Create, holding the figures of the half its
        // source drives: here, the half whose network has a generator.
        let sub = self.observed_sub(p);
        let (theoretical, stress, capacity, overstressed) = k.network(p, sub)?;
        let speed = k.speed(p, sub)?;
        let generated = k.generated(p, sub)?;
        let part = both[sub as usize]?;
        let faces = both[1].is_some().then(|| {
            let mut m = BTreeMap::new();
            for d in create_engine::kinetics::Dir::ALL {
                for (s, half) in both.iter().enumerate() {
                    if half.is_some_and(|h| h.shafts & d.bit() != 0) {
                        m.insert(
                            format!("{d:?}").to_lowercase(),
                            k.speed(p, s as u8).unwrap_or(0.0),
                        );
                    }
                }
            }
            m
        });
        Some(Kinetic {
            speed,
            theoretical,
            overstressed,
            stress,
            capacity,
            impact: part.impact,
            added_capacity: part.capacity,
            generated,
            has_source: k.driven_elsewhere(p, sub),
            comparator: self.w.dims[0]
                .gauges
                .get(&p)
                .map(|g| create_engine::gauge::analog_output(g.shown.dial)),
            sequence_angle: k.sequence_limit(p, sub, false),
            sequence_distance: k.sequence_limit(p, sub, true),
            faces,
        })
    }
}

pub fn run(sc: &Scenario) -> Snapshot {
    let mut r = Run::new(sc);
    let mut checkpoints = BTreeMap::new();
    for t in 0..sc.ticks {
        if sc.checkpoints.contains(&t) {
            checkpoints.insert(t, r.snapshot(sc));
        }
        // A player places the blocks, one each tick.
        if let Some(b) = sc.blocks.get(t as usize) {
            r.place(b);
        }
        if let Some(b) = t.checked_sub(1).and_then(|p| sc.blocks.get(p as usize)) {
            r.set_motor(b);
        }
        for e in sc.events.iter().filter(|e| e.tick + 1 == t) {
            if let Some(b) = e.placed() {
                r.set_motor(&b);
            }
        }
        for e in sc.events.iter().filter(|e| e.tick == t) {
            match (e.placed(), e.turn) {
                (Some(b), _) => r.place(&b),
                // `HandCrankBlock.useWithoutItem`: `turn(sneaking)`.
                (None, Some(back)) => {
                    r.w.turn_crank(0, e.pos, back);
                }
                (None, None) => {}
            }
        }
        r.tick();
    }
    if sc.checkpoints.contains(&sc.ticks) {
        checkpoints.insert(sc.ticks, r.snapshot(sc));
    }
    let mut out = r.snapshot(sc);
    out.checkpoints = checkpoints;
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn network_observation_uses_driven_positive_split_half() {
        let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
            "ticks": 80,
            "blocks": [
                {"pos":[1,8,0],"state":"create:creative_motor[facing=west]","motor":32},
                {"pos":[0,9,0],"state":"minecraft:redstone_block"},
                {"pos":[0,8,0],"state":"create:clutch[axis=x,powered=true]"},
                {"pos":[-1,8,0],"state":"create:shaft[axis=x]"}
            ]
        }))
        .unwrap();
        let snapshot = super::run(&sc);
        let networks = snapshot.networks.unwrap();
        assert_eq!(networks.partitions, vec![vec!["0,8,0", "1,8,0"]]);
        assert_eq!(networks.sizes["0,8,0"], 2);
        assert_eq!(
            snapshot.blocks["0,8,0"].kinetic.as_ref().unwrap().speed,
            -32.0
        );
    }
    #[test]
    fn network_observations_group_identity_and_follow_removal() {
        let mut sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
            "ticks": 80,
            "blocks": [
                {"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32},
                {"pos":[1,8,0],"state":"create:shaft[axis=x]"},
                {"pos":[0,8,4],"state":"create:creative_motor[facing=east]","motor":32},
                {"pos":[1,8,4],"state":"create:shaft[axis=x]"},
                {"pos":[4,8,0],"state":"create:shaft[axis=x]"}
            ]
        }))
        .unwrap();
        let snapshot = super::run(&sc);
        assert!(snapshot.unavailable.is_empty());
        let networks = snapshot.networks.unwrap();
        assert_eq!(
            networks.partitions,
            vec![vec!["0,8,0", "1,8,0"], vec!["0,8,4", "1,8,4"]]
        );
        assert_eq!(networks.sizes.len(), 4);
        assert!(networks.sizes.values().all(|size| *size == 2));
        sc.events
            .push(crate::scenario::Change::set(10, [0, 8, 0], "minecraft:air"));
        let networks = super::run(&sc).networks.unwrap();
        assert_eq!(networks.partitions, vec![vec!["0,8,4", "1,8,4"]]);
        assert_eq!(networks.sizes.len(), 2);
    }
}
