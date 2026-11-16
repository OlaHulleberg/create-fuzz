//! Runs a scenario through create-horizon's engine the way its game glue does each tick
//! (`create/src/glue/kinetics.rs`): inputs (split shafts following redstone), then
//! `settle_belts`, the solve, and the blocks the solve breaks.
//!
//! The world outside the engine is a map of positions to state strings. Redstone is only what
//! the scenarios use: a `minecraft:redstone_block` powers the blocks beside it.

use crate::scenario::{BlockSnap, Kinetic, Placed, Pos, Scenario, Snapshot, key, parse_state, with_prop};
use create_engine::parts::{Settings, StateInfo, parts};
use create_engine::states;
use create_engine::world::{CreateWorld, StateTable};
use std::collections::{BTreeMap, HashMap};

/// Create's state ids start here; below are the few vanilla ones the scenarios use.
const FIRST: u32 = 16;
const AIR: u32 = 0;
const REDSTONE_BLOCK: u32 = 1;
const OTHER: u32 = 2;

const DIRS: [Pos; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

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
    let all: Vec<Placed> = sc.blocks.iter().cloned().chain(sc.events.iter().filter_map(|e| e.placed())).collect();
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
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let names = create_states(sc);
        let parsed: Vec<(String, Vec<(String, String)>)> = names.iter().map(|s| parse_state(s)).collect();
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
        // A player setting the motor's value board (`set_motor`).
        if let Some(rpm) = b.motor {
            let settings = Settings {
                motor_rpm: rpm as f32,
                ..Settings::default()
            };
            self.w.set_settings(0, b.pos, id, settings);
        }
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
                let next = with_prop(&self.name(state), "powered", if now { "true" } else { "false" });
                let id = self.id(&next);
                self.set(p, id);
            }
            if self.w.reads_signal(state) {
                self.w.set_chain_signal(0, p, if now { 15 } else { 0 });
            }
        }
        let shapes = self.w.take_chain_shapes(0);
        if !shapes.is_empty() {
            self.notes.push(format!("chain shapes not applied: {}", shapes.len()));
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
        let positions = sc.blocks.iter().map(|b| b.pos).chain(sc.events.iter().map(|e| e.pos));
        for p in positions {
            if blocks.contains_key(&key(p)) {
                continue;
            }
            let id = self.at(p);
            if id == AIR {
                continue;
            }
            let state = self.name(id);
            let kinetic = self.kinetic(p, id);
            blocks.insert(key(p), BlockSnap { state, kinetic, debug: None });
        }
        Snapshot {
            side: "rust".into(),
            blocks,
            notes: self.notes.clone(),
        }
    }

    fn kinetic(&self, p: Pos, id: u32) -> Option<Kinetic> {
        let k = &self.w.dims[0].kinetics;
        let (theoretical, stress, capacity, overstressed) = k.network(p, 0)?;
        let speed = k.speed(p, 0)?;
        let generated = k.generated(p, 0)?;
        let (block, info): (create_engine::Block, &StateInfo) = self.w.table.get(id)?;
        let part = parts(block, info, &Settings::default())[0]?;
        Some(Kinetic {
            speed,
            theoretical,
            overstressed,
            stress,
            capacity,
            impact: part.impact,
            added_capacity: part.capacity,
            generated,
        })
    }
}

pub fn run(sc: &Scenario) -> Snapshot {
    let mut r = Run::new(sc);
    for t in 0..sc.ticks {
        // A player places the blocks, one each tick.
        if let Some(b) = sc.blocks.get(t as usize) {
            r.place(b);
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
    r.snapshot(sc)
}
