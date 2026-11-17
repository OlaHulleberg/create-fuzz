//! Runs a scenario through create-horizon's engine the way its game glue does each tick
//! (`create/src/glue/kinetics.rs`): inputs (split shafts following redstone), then
//! `settle_belts`, the solve, and the blocks the solve breaks.
//!
//! The world outside the engine is a map of positions to state strings. Redstone is only what
//! the scenarios use: adjacent redstone blocks or controlled 0..15 signal sources.

use crate::scenario::{
    BlockSnap, Kinetic, Placed, Pos, Scenario, Snapshot, key, parse_state, with_prop,
};
use create_engine::parts::{Settings, StateInfo, parts};
use create_engine::states;
use create_engine::world::{CreateWorld, StateTable};
use std::collections::{BTreeMap, HashMap};

/// Create's state ids start here; below are the few vanilla ones the scenarios use.
const FIRST: u32 = 32;
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
    for entry in crate::catalogue::entries()
        .iter()
        .filter(|e| e.id == "create:sequenced_gearshift" || e.id.ends_with("_valve_handle"))
    {
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
    handles: HashMap<Pos, crate::handles::Handle>,
    chain_signals: HashMap<Pos, i32>,
    brackets: HashMap<Pos, create_engine::decor::Bracket>,
    measured_trig: HashMap<Pos, crate::allow::MeasuredTrigState>,
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
            handles: HashMap::new(),
            chain_signals: HashMap::new(),
            brackets: HashMap::new(),
            measured_trig: HashMap::new(),
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
        if old == new {
            return;
        }
        if new == AIR {
            self.world.remove(&p);
        } else {
            self.world.insert(p, new);
        }
        self.brackets.remove(&p);
        self.measured_trig.remove(&p);
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
        self.w.block_changed(0, p, old, new);
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
    fn action(&mut self, p: Pos, action: &crate::actions::Action) {
        use crate::actions::Action;
        match action {
            Action::SpeedSetting { row, value } => {
                self.w.board_motor(
                    0,
                    p,
                    create_engine::board::Settings {
                        row: *row,
                        value: *value,
                    },
                );
            }
            Action::ValveSetting { row, value } => {
                if let Some(h) = self.handles.get_mut(&p) {
                    h.selected = create_engine::board::kinetic_value(
                        create_engine::board::Settings {
                            row: *row,
                            value: *value,
                        },
                        180,
                    );
                }
            }
            Action::Program { instructions } => {
                // sequencer::client_message rejects late screen packets after replacement.
                if self.w.block_at(0, p) != Some(create_engine::Block::SequencedGearshift) {
                    return;
                }
                if let Some(step) = self.w.set_sequencer_instructions(
                    0,
                    p,
                    instructions.iter().map(|i| i.engine()).collect(),
                    true,
                ) {
                    self.sequencer_step(p, step);
                }
            }
            Action::Remove => self.set(p, AIR),
            Action::Signal { strength } => self.set(p, 3 + u32::from(*strength)),
            Action::Use {
                item,
                face,
                hit,
                yaw,
                pitch,
                sneaking,
            } => self.use_item(p, item, face.dir(), *hit, *yaw, *pitch, *sneaking),
        }
    }

    /// Glue order: RightClickBlock, block use_item_on, then the item's use_on.
    #[allow(clippy::too_many_arguments)]
    fn use_item(
        &mut self,
        p: Pos,
        item: &str,
        face: create_engine::kinetics::Dir,
        hit: [f64; 3],
        yaw: f32,
        pitch: f32,
        sneaking: bool,
    ) {
        use create_engine::{Block, board, place};
        let state = self.at(p);
        let own = self.w.table.get(state).map(|(b, i)| (b, *i));
        if let Some((block, info)) = own.filter(|(b, _)| create_engine::dye::is_valve_handle(*b)) {
            // interaction/values.rs: a box takes the click only while its behaviour is active.
            if !sneaking
                && self.w.value_box_active(0, p)
                && let Some(slot) = info.facing.and_then(|f| board::valve_slot(f, face))
                && board::hits_valve_box(slot, hit)
            {
                return;
            }
            if !(sneaking && item == "create:wrench") {
                if let Some(to) = create_engine::dye::DyeColor::of_item(item)
                    .and_then(|c| create_engine::dye::recoloured(block, c))
                {
                    let (_, props) = parse_state(&self.name(state));
                    let next =
                        crate::scenario::format_state(&format!("create:{}", to.name()), &props);
                    self.set(p, self.id(&next));
                } else {
                    let selected = self.handles.get(&p).map_or(45, |h| h.selected);
                    if self.w.use_valve(0, p, sneaking, selected) {
                        self.handles.entry(p).or_default().started(sneaking);
                    }
                }
                return;
            }
        }
        if item == "minecraft:air" && own.is_some_and(|(b, _)| b == Block::HandCrank) {
            self.w.turn_crank(0, p, sneaking);
            return;
        }
        if let Some((block, _)) = own {
            let name = format!("create:{}", block.name());
            if !crate::actions::BLOCKS.contains(&name.as_str())
                && name != "create:andesite_encased_shaft"
                && name != "create:brass_encased_shaft"
                && name != "create:brass_casing"
                && name != "create:andesite_casing"
            {
                self.notes.push(format!("adapter gap: RightClickBlock/value-box and block-use handlers for {name} at {p:?} are outside the supported subset; input retained"));
                return;
            }
        }
        // interaction/values.rs: the motor's active value box consumes the click first.
        if !sneaking
            && let Some((Block::CreativeMotor, info)) = own
            && self.w.value_box_active(0, p)
            && let Some(slot) = info.facing.and_then(|f| board::motor_slot(f, face))
            && board::hits_box(slot, hit)
        {
            return;
        }
        if !sneaking {
            // decor/brackets.rs runs before the wrench item's use_on.
            if item == "create:wrench" && self.brackets.remove(&p).is_some() {
                return;
            }
            // transmission.rs Clicked: casing, then the shaft placement helper.
            if let Some((Block::Shaft, info)) = own {
                let axis = info.axis.expect("shaft axis");
                if item == "create:brass_casing" || item == "create:andesite_casing" {
                    let next = format!(
                        "create:{}_encased_shaft[axis={}]",
                        if item == "create:brass_casing" {
                            "brass"
                        } else {
                            "andesite"
                        },
                        format!("{axis:?}").to_lowercase()
                    );
                    self.switch(p, self.id(&next));
                    return;
                }
                if item == "create:shaft" {
                    let at = place::pole_helper(
                        p,
                        axis,
                        hit.map(|v| v - 0.5),
                        |q| {
                            self.w
                                .table
                                .get(self.at(q))
                                .is_some_and(|(b, i)| b == Block::Shaft && i.axis == Some(axis))
                        },
                        |q| self.at(q) == AIR,
                    );
                    if let Some(q) = at {
                        let next = format!(
                            "create:shaft[axis={},waterlogged=false]",
                            format!("{axis:?}").to_lowercase()
                        );
                        self.set(q, self.id(&next));
                        return;
                    }
                }
            }
        }
        // Item use_on: bracket, wrench (including subsystem casing removal), BlockItem.
        if item == "create:metal_bracket" || item == "create:wooden_bracket" {
            if let Some((Block::Shaft, info)) = own {
                let toward = place::ordered_by_nearest(yaw, pitch)[0].opposite();
                if let Some(b) = create_engine::decor::bracket_for_click(
                    item == "create:wooden_bracket",
                    info.axis,
                    create_engine::decor::BracketKind::Shaft,
                    face,
                    toward,
                ) {
                    self.brackets.insert(p, b);
                }
            } else if own.is_some() {
                self.notes.push(format!(
                    "adapter gap: bracket target outside shaft subset at {p:?}"
                ));
            }
            return;
        }
        if item == "create:wrench" {
            let (name, props) = parse_state(&self.name(state));
            if name == "minecraft:air" || name == "minecraft:stone" {
                return;
            }
            if sneaking {
                if name == "create:andesite_encased_shaft" || name == "create:brass_encased_shaft" {
                    let axis = props.iter().find(|(k, _)| k == "axis").unwrap().1.clone();
                    self.switch(
                        p,
                        self.id(&format!("create:shaft[axis={axis},waterlogged=false]")),
                    );
                } else {
                    self.set(p, AIR);
                }
                return;
            }
            if !crate::actions::BLOCKS.contains(&name.as_str())
                && !name.ends_with("_encased_shaft")
                && !name.ends_with("_casing")
            {
                self.notes.push(format!(
                    "adapter gap: unsupported wrench override at {p:?}: {name}"
                ));
                return;
            }
            let measured = self.measured_trig.get(&p).cloned();
            let rotated = create_engine::wrench::rotated(&props.into_iter().collect(), false, face);
            let next =
                crate::scenario::format_state(&name, &rotated.into_iter().collect::<Vec<_>>());
            self.switch(p, self.id(&next));
            if let Some(m) = measured {
                let (name, props) = parse_state(&m.java);
                let rotated =
                    create_engine::wrench::rotated(&props.into_iter().collect(), false, face);
                self.measured_trig.insert(
                    p,
                    crate::allow::MeasuredTrigState {
                        java: crate::scenario::format_state(
                            &name,
                            &rotated.into_iter().collect::<Vec<_>>(),
                        ),
                        port: normal(&next),
                    },
                );
            }
            return;
        }
        if item == "minecraft:air" || create_engine::dye::DyeColor::of_item(item).is_some() {
            return;
        }
        // No override of the clicked position: air replaces itself; solid clicks place adjacent.
        let p = if state == AIR {
            p
        } else {
            add(p, face.offset())
        };
        if self.at(p) != AIR {
            return;
        }
        self.place_player(p, item, yaw, pitch, face, sneaking);
    }

    fn switch(&mut self, p: Pos, id: u32) {
        self.w.switching_block_state(0, p, id);
        self.set(p, id);
    }
    fn place_player(
        &mut self,
        p: Pos,
        block: &str,
        yaw: f32,
        pitch: f32,
        face: create_engine::kinetics::Dir,
        sneaking: bool,
    ) {
        if block == "create:brass_casing" || block == "create:andesite_casing" {
            self.set(p, self.id(block));
            return;
        }
        let b = create_engine::Block::ALL
            .iter()
            .copied()
            .find(|b| format!("create:{}", b.name()) == block)
            .expect("block");
        let ctx = create_engine::place::Context {
            yaw,
            pitch,
            sneaking,
            clicked_face: face,
        };
        let shaft = |d: create_engine::kinetics::Dir| {
            self.w
                .has_shaft_towards(0, add(p, d.offset()), d.opposite())
        };
        let mut next = if let Some(f) = create_engine::place::facing(b, ctx, shaft) {
            format!("{block}[facing={}]", format!("{f:?}").to_lowercase())
        } else {
            let (a, _) = create_engine::place::axis(
                b,
                ctx,
                shaft,
                create_engine::place::Nearby {
                    large_cog_above: self.w.large_cog_axis(0, add(p, [0, 1, 0])),
                    ..Default::default()
                },
            )
            .expect("placement axis");
            format!("{block}[axis={}]", format!("{a:?}").to_lowercase())
        };
        if block.contains("chain") {
            next = with_prop(&next, "part", "none");
            next = with_prop(&next, "axis_along_first", "false");
        }
        if block == "create:shaft" {
            next = with_prop(&next, "waterlogged", "false");
        }
        if block == "create:clutch"
            || block == "create:gearshift"
            || block == "create:adjustable_chain_gearshift"
        {
            next = with_prop(&next, "powered", "false");
            if self.signal(p) > 0 {
                self.notes.push(
                    "API_GAPS: redstone unavailable during state_for_placement; corrected on tick"
                        .into(),
                );
            }
        }
        let measured = block == "create:shaft"
            && yaw == 45.0
            && pitch == 0.0
            && create_engine::place::preferred(false, shaft).is_none();
        self.set(p, self.id(&next));
        if measured {
            self.measured_trig.insert(
                p,
                crate::allow::MeasuredTrigState {
                    java: normal(&with_prop(&next, "axis", "z")),
                    port: normal(&next),
                },
            );
        }
    }

    /// One level tick: inputs, then the solve and what it breaks.
    fn tick(&mut self) {
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
    fn sequencer_step(&mut self, p: [i32; 3], step: create_engine::sequencer::Step) {
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
                    measured_trig: self.measured_trig.get(&p).cloned(),
                    bracket: self.brackets.get(&p).map(|b| {
                        format!(
                            "{}[axis_along_first={},facing={},type={}]",
                            b.block(),
                            b.along_first,
                            format!("{:?}", b.facing).to_lowercase(),
                            b.kind.name()
                        )
                    }),
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

    fn context_limit(&self, p: Pos, sub: u8, distance: bool) -> Option<f32> {
        let k = &self.w.dims[0].kinetics;
        if self.w.block_at(0, p) == Some(create_engine::Block::SequencedGearshift) {
            // Java's single entity holds the outgoing context but reads it at the
            // driven shaft's theoretical speed. The engine stores it on the far half.
            let driven = k.theoretical(p, sub)?.0;
            for half in [0, 1] {
                if let Some(value) = k.sequence_limit(p, half, distance) {
                    let speed = k.theoretical(p, half)?.0;
                    return Some(if speed == 0. {
                        0.
                    } else {
                        value * (driven / speed).abs()
                    });
                }
            }
            None
        } else {
            k.sequence_limit(p, sub, distance)
        }
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
            sequence_angle: self.context_limit(p, sub, false),
            sequence_distance: self.context_limit(p, sub, true),
            valve: self.handles.get(&p).map(|h| {
                let mut observed = h.observe(info.facing.is_some_and(|f| f.positive()));
                if h.remaining == 0 && k.driven_elsewhere(p, sub) && speed != 0. {
                    observed.angle = None;
                    observed.angle_unavailable =
                        Some("externally driven angle requires client renderer".into());
                }
                observed
            }),
            sequencer: (block == create_engine::Block::SequencedGearshift)
                .then(|| self.w.sequencer_run(0, p))
                .flatten()
                .map(|r| crate::observation::SequenceRun {
                    current: r.current,
                    duration: r.duration,
                    progress: r.progress,
                    timer: r.timer,
                    powered_before: r.powered_before,
                }),
            chain_modifier: (block == create_engine::Block::AdjustableChainGearshift).then(|| {
                create_engine::chain::modifier_for_signal(
                    self.chain_signals.get(&p).copied().unwrap_or(0),
                )
            }),
            speed_setting: self.w.motor_setting(0, p),
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
    fn busy_valve_box_passes_a_centred_dye_to_the_block() {
        // interaction/values.rs: `ValveHandleBlockEntity.showValue` hides the box while in use.
        let sc: crate::scenario::Scenario =
            serde_json::from_value(serde_json::json!({"ticks":80,"blocks":[
            {"pos":[0,7,0],"state":"minecraft:stone"},
            {"pos":[0,8,0],"state":"create:copper_valve_handle[facing=up,waterlogged=false]"}]}))
            .unwrap();
        let mut r = super::Run::new(&sc);
        for b in &sc.blocks {
            r.place(b);
        }
        let p = [0, 8, 0];
        let face = create_engine::kinetics::Dir::Up;
        r.use_item(p, "minecraft:air", face, [0.1, 0.5, 0.1], 0., 0., false);
        assert!(r.handles[&p].remaining > 0);
        r.use_item(p, "minecraft:red_dye", face, [0.5, 0.5, 0.5], 0., 0., false);
        assert_eq!(
            r.w.block_at(0, p),
            Some(create_engine::Block::RedValveHandle)
        );
    }

    #[test]
    fn late_program_packet_cannot_reconfigure_a_replacement_motor() {
        let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({"ticks":80,
            "blocks":[{"pos":[0,8,0],"state":"create:sequenced_gearshift[axis=x,vertical=false,state=0]"}],
            "events":[{"tick":5,"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32}]})).unwrap();
        let mut r = super::Run::new(&sc);
        let p = [0, 8, 0];
        r.place(&sc.blocks[0]);
        r.place(&sc.events[0].placed().unwrap());
        let previous = r.w.sequencer_instructions(0, p);
        r.action(
            p,
            &crate::actions::Action::Program {
                instructions: vec![crate::program::Instruction {
                    instruction: crate::program::Kind::Delay,
                    modifier: crate::program::Modifier::Forward,
                    value: 5,
                }],
            },
        );
        assert_eq!(r.w.sequencer_instructions(0, p), previous);
        assert_eq!(
            r.w.block_at(0, p),
            Some(create_engine::Block::CreativeMotor)
        );
    }

    #[test]
    fn replaced_sequencer_has_only_the_current_blocks_observations() {
        let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
            "ticks":80,"blocks":[{"pos":[0,8,0],"state":"create:sequenced_gearshift[axis=x,vertical=false,state=0]"}],
            "events":[{"tick":5,"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32}]})).unwrap();
        let snap = super::run(&sc);
        let k = snap.blocks["0,8,0"].kinetic.as_ref().unwrap();
        assert_eq!(k.speed_setting, Some(32.));
        assert!(k.sequencer.is_none());
    }

    #[test]
    fn client_only_power_commit_does_not_enqueue_neighbour_inputs() {
        let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
            "ticks":80,"blocks":[
            {"pos":[0,8,0],"state":"create:adjustable_chain_gearshift[axis=x,axis_along_first=false,part=none,powered=false]"},
            {"pos":[0,8,1],"state":"create:adjustable_chain_gearshift[axis=x,axis_along_first=false,part=none,powered=false]"}]})).unwrap();
        let mut r = super::Run::new(&sc);
        for b in &sc.blocks {
            r.place(b);
        }
        // Complete the placement-time initial signal read before changing power.
        for block in &sc.blocks {
            r.w.set_chain_signal(0, block.pos, 0);
        }
        r.w.take_redstone(0);
        let p = [0, 8, 0];
        let powered = r.id(&crate::scenario::with_prop(
            &sc.blocks[0].state,
            "powered",
            "true",
        ));
        r.commit(p, powered, false);
        assert!(r.w.take_redstone(0).is_empty());
        r.set(p, r.id(&sc.blocks[0].state));
        assert_eq!(r.w.take_redstone(0), vec![[0, 8, 1]]);
    }

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
