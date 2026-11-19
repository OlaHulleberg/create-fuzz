//! Runs a scenario through create-horizon's engine the way its game glue does each tick
//! (`create/src/glue/kinetics.rs`): inputs (split shafts following redstone), then
//! `settle_belts`, the solve, and the blocks the solve breaks.
//!
//! The world outside the engine is a map of positions to state strings. Redstone is only what
//! the scenarios use: adjacent redstone blocks or controlled 0..15 signal sources.

mod belts;
mod conveyors;
mod fluid_machines;
mod fluid_world;
mod fluids;
mod interaction;
mod kinetics;
mod observation;
mod persistence;
mod pipes;
mod pumps;
mod sources;
mod transport;

use crate::scenario::{
    BlockSnap, Kinetic, Placed, Pos, Scenario, Snapshot, key, parse_state, with_prop,
};
use create_engine::parts::{Settings, StateInfo, parts};
use create_engine::states;
use create_engine::world::{CreateWorld, StateTable};
use std::collections::{BTreeMap, HashMap};

/// Create's state ids start here; below are the few vanilla ones the scenarios use.
const FIRST: u32 = 256;
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
    // Register the full reachable orientation closure for supported player blocks.
    for entry in crate::catalogue::entries()
        .iter()
        .filter(|e| crate::actions::BLOCKS.contains(&e.id.as_str()))
    {
        for spec in entry.specifications() {
            push(spec.state);
        }
    }
    for entry in &crate::fluids::contract().blocks {
        for state in entry.specifications() {
            push(state);
        }
    }
    for water in ["false", "true"] {
        push(format!("create:depot[waterlogged={water}]"));
    }
    push("create:brass_casing".into());
    push("create:andesite_casing".into());
    for axis in ["x", "y", "z"] {
        for material in ["brass", "andesite"] {
            push(format!("create:{material}_encased_shaft[axis={axis}]"));
        }
    }
    for face in ["up", "down", "north", "south", "east", "west"] {
        push(format!("create:creative_motor[facing={face}]"));
    }
    // Engines turn their shaft into a powered shaft (`PoweredShaftBlock.getEquivalent`).
    for axis in ["x", "y", "z"] {
        for water in ["true", "false"] {
            push(format!(
                "create:powered_shaft[axis={axis},waterlogged={water}]"
            ));
        }
    }
    for entry in crate::catalogue::entries().iter().filter(|e| {
        e.id == "create:belt"
            || e.id == "create:fluid_valve"
            || e.id == "create:sequenced_gearshift"
            || e.id.ends_with("_valve_handle")
    }) {
        for spec in entry.specifications() {
            push(spec.state);
        }
    }
    for b in &all {
        if !b.state.starts_with("create:") {
            continue;
        }
        let (_, props) = parse_state(&b.state);
        push(normal(&b.state));
        if props.iter().any(|(k, _)| k == "waterlogged") {
            push(normal(&with_prop(&b.state, "waterlogged", "true")));
            push(normal(&with_prop(&b.state, "waterlogged", "false")));
        }
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
    persisted: persistence::Store,
    ids: HashMap<String, u32>,
    names: Vec<String>,
    world: HashMap<Pos, u32>,
    entity_order: Vec<Pos>,
    notes: Vec<String>,
    depot_stacks: HashMap<Pos, crate::transport::Stack>,
    depot_outputs: HashMap<Pos, Vec<crate::transport::Stack>>,
    item_outcomes: Vec<crate::transport::Outcome>,
    chains: create_engine::chain_conveyor::Chains<()>,
    connector: Option<Pos>,
    belt_casing: HashMap<Pos, String>,
    handles: HashMap<Pos, crate::handles::Handle>,
    brackets: HashMap<Pos, create_engine::decor::Bracket>,
    measured_trig: HashMap<Pos, String>,
    java_ties: bool,
    native_origin: Pos,
    creative: bool,
    fluid_host: Option<crate::fluid_host::Host>,
    fluid_host_enabled: bool,
    native_failed: bool,
    fluid_rules: crate::fluid_world::Rules,
    fluid_facts: HashMap<Pos, (String, crate::fluid_host::Facts)>,
    fluid_host_changes: Vec<crate::fluid_host::State>,
    fluid_native_schedule: Vec<Pos>,
    fluid_water_due: BTreeMap<usize, Vec<Pos>>,
    fluid_domain: std::collections::BTreeSet<Pos>,
    /// `FluidState.getFlow` at every wheel's sample cells, as the native host reported it.
    wheel_fluid_flow: HashMap<Pos, [f64; 3]>,
    /// Each wheel's flow score (`WaterWheelBlockEntity.flowScore`), for observations.
    wheel_scores: HashMap<Pos, i8>,
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
            persisted: Default::default(),
            ids,
            names,
            world: HashMap::new(),
            entity_order: Vec::new(),
            notes: Vec::new(),
            depot_stacks: HashMap::new(),
            depot_outputs: HashMap::new(),
            item_outcomes: Vec::new(),
            chains: Default::default(),
            connector: None,
            belt_casing: HashMap::new(),
            handles: HashMap::new(),
            brackets: HashMap::new(),
            measured_trig: HashMap::new(),
            java_ties: false,
            native_origin: [0; 3],
            creative: false,
            fluid_host: None,
            fluid_host_enabled: crate::fluid_world::enabled(sc),
            native_failed: false,
            fluid_rules: crate::fluid_world::Rules::default(),
            fluid_facts: HashMap::new(),
            fluid_host_changes: Vec::new(),
            fluid_native_schedule: Vec::new(),
            fluid_water_due: BTreeMap::new(),
            fluid_domain: sc.region().into_iter().collect(),
            wheel_fluid_flow: HashMap::new(),
            wheel_scores: HashMap::new(),
            _guard: guard,
        }
    }

    fn id(&self, state: &str) -> u32 {
        if let Some(v) = state
            .strip_prefix("createfuzz:signal[strength=")
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| s.parse::<u32>().ok())
        {
            return 3 + v;
        }
        if let Some(id) = crate::fluid_world::id(state) {
            return id;
        }
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
            3..=18 => format!("createfuzz:signal[strength={}]", id - 3),
            19..=255 => crate::fluid_world::name(id).expect("registered external state"),
            _ => self.names[(id - FIRST) as usize].clone(),
        }
    }

    fn at(&self, p: Pos) -> u32 {
        self.world.get(&p).copied().unwrap_or(AIR)
    }

    /// A committed block change, then its neighbours told (`neighborChanged`).
    fn set(&mut self, p: Pos, new: u32) {
        self.commit(p, new, true);
    }

    /// UPDATE_CLIENTS changes the state without notifying neighbours; UPDATE_ALL does both.
    fn commit(&mut self, p: Pos, new: u32, notify: bool) {
        let old = self.at(p);
        if self.w.table.get(old).map(|(b, _)| b) != self.w.table.get(new).map(|(b, _)| b) {
            self.persisted.kinetics.remove(&(p, 0));
            self.persisted.kinetics.remove(&(p, 1));
            self.persisted.sequences.remove(&p);
            self.persisted.depots.remove(&p);
            self.persisted.belts.remove(&p);
            self.persisted.fluid_entities.remove(&p);
            self.persisted.chain_entities.remove(&p);
            if !self
                .w
                .table
                .get(old)
                .is_some_and(|(b, _)| create_engine::dye::is_valve_handle(b))
                || !self
                    .w
                    .table
                    .get(new)
                    .is_some_and(|(b, _)| create_engine::dye::is_valve_handle(b))
            {
                self.persisted.handles.remove(&p);
            }
        }
        if old == new {
            return;
        }
        // Native block-entity registrations survive state changes and recolouring,
        // while replacement/removal registers a new ticker at the end of the level.
        let old_block = self.w.table.get(old).map(|(block, _)| block);
        let new_block = self.w.table.get(new).map(|(block, _)| block);
        let funnel = |block| match block {
            Some(
                create_engine::Block::AndesiteFunnel | create_engine::Block::AndesiteBeltFunnel,
            ) => Some(false),
            Some(create_engine::Block::BrassFunnel | create_engine::Block::BrassBeltFunnel) => {
                Some(true)
            }
            _ => None,
        };
        let retains = old_block == new_block
            || old_block.zip(new_block).is_some_and(|(old, new)| {
                create_engine::dye::is_valve_handle(old) && create_engine::dye::is_valve_handle(new)
            })
            || funnel(old_block).is_some_and(|material| funnel(new_block) == Some(material));
        if !retains {
            self.entity_order.retain(|pos| *pos != p);
        }
        if new_block.is_some_and(|block| block.has_block_entity())
            && !self.entity_order.contains(&p)
        {
            self.entity_order.push(p);
        }
        if new == AIR {
            self.world.remove(&p);
        } else {
            self.world.insert(p, new);
        }
        if self
            .w
            .table
            .get(old)
            .is_some_and(|(b, _)| b == create_engine::Block::Depot)
            && !self
                .w
                .table
                .get(new)
                .is_some_and(|(b, _)| b == create_engine::Block::Depot)
        {
            self.depot_stacks.remove(&p);
            self.depot_outputs.remove(&p);
        }
        if self.brackets.remove(&p).is_some() {
            self.mark_column(p);
        }
        if self.w.table.get(old).map(|(b, _)| b) != self.w.table.get(new).map(|(b, _)| b) {
            self.measured_trig.remove(&p);
        }
        let valve = self
            .w
            .table
            .get(new)
            .is_some_and(|(b, _)| create_engine::dye::is_valve_handle(b));
        if valve {
            self.handles.entry(p).or_default();
        } else {
            self.handles.remove(&p);
        }
        if !self
            .w
            .table
            .get(new)
            .is_some_and(|(b, _)| b == create_engine::Block::Belt)
        {
            self.belt_casing.remove(&p);
        }
        self.w.block_changed(0, p, old, new);
        if self.w.table.get(new).map(|(b, _)| b)
            == Some(create_engine::Block::AdjustableChainGearshift)
            && self.w.table.get(old).map(|(b, _)| b)
                != Some(create_engine::Block::AdjustableChainGearshift)
        {
            // A freshly constructed entity receives the same on-load read as a saved
            // one, with missing Signal defaulting to zero, before its first attachment.
            self.w.restore_chain_signal(0, p, 0);
        }
        self.conveyor_changed(p, old, new);
        self.fluid_world_changed(p);
        if !notify {
            return;
        }
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
        self.measured_trig.remove(&b.pos);
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
        self.w.board_motor(
            0,
            b.pos,
            create_engine::board::Settings {
                row: i32::from(rpm >= 0),
                value: rpm.abs(),
            },
        );
        self.publish_interaction();
    }

    fn signal(&self, p: Pos) -> u8 {
        DIRS.iter()
            .map(|d| match self.at(add(p, *d)) {
                REDSTONE_BLOCK => 15,
                id @ 3..=18 => (id - 3) as u8,
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
pub fn run(sc: &Scenario) -> Snapshot {
    run_at_phase(sc, 0)
}

pub fn run_at_phase(sc: &Scenario, phase: u8) -> Snapshot {
    crate::trig::run(sc, phase, [0; 3])
}
pub fn run_like(sc: &Scenario, reference: &Snapshot) -> Snapshot {
    crate::trig::run(sc, reference.save_phase, reference.origin)
}
#[cfg(test)]
pub(crate) fn run_trace(sc: &Scenario, java_ties: bool) -> Snapshot {
    run_trace_at(sc, java_ties, 0, [0; 3])
}
pub(crate) fn run_trace_at(sc: &Scenario, java_ties: bool, phase: u8, origin: Pos) -> Snapshot {
    let mut r = Run::new(sc);
    r.java_ties = java_ties;
    r.native_origin = origin;
    r.persisted.clock = usize::from(phase);
    r.persisted.phase = phase;
    execute(sc, r)
}

fn execute(sc: &Scenario, mut r: Run) -> Snapshot {
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
            if let Some(a) = &e.action {
                r.action(e.pos, a);
            }
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
        if r.native_failed {
            break;
        }
    }
    if !r.native_failed && sc.checkpoints.contains(&sc.ticks) {
        checkpoints.insert(sc.ticks, r.snapshot(sc));
    }
    let mut out = r.snapshot(sc);
    out.checkpoints = checkpoints;
    out
}

#[cfg(test)]
mod tests;
