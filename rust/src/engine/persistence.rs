//! Host unload/load order: column attachments, block scan, per-entity kinetic/run data.
use super::*;
use crate::persistence::codec::{self, Compound, Tag};

#[derive(Default)]
pub(super) struct Store {
    pub clock: usize,
    pub phase: u8,
    pub kinetics: HashMap<(Pos, u8), Compound>,
    pub sequences: HashMap<Pos, Compound>,
    pub columns: Compound,
    pub handles: HashMap<Pos, Compound>,
    pub depots: HashMap<Pos, Compound>,
    pub belts: HashMap<Pos, Compound>,
    pub fluid_columns: BTreeMap<(i32, i32), Compound>,
    pub fluid_entities: HashMap<Pos, Compound>,
    pub chain_entities: HashMap<Pos, Compound>,
    pub dirty_columns: std::collections::BTreeSet<(i32, i32)>,
    pub brackets: HashMap<Pos, create_engine::decor::Bracket>,
    pub conveyors: HashMap<Pos, Vec<Pos>>,
}
type ReloadData = (
    Compound,
    Vec<(Pos, u8, Compound)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, crate::transport::Stack)>,
    Vec<(Pos, crate::handles::Handle)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, Compound)>,
    FluidReload,
    Vec<(Pos, Compound)>,
);

#[derive(serde::Serialize, serde::Deserialize)]
struct FluidReload {
    columns: Vec<((i32, i32), Compound)>,
    entities: Vec<(Pos, Compound)>,
}

impl Run {
    pub(super) fn save_sequence(&mut self, p: Pos) {
        if let Some(seq) = self.w.dims[0].sequencers.get(&p) {
            let old = self.persisted.sequences.get(&p).and_then(codec::run_of_tag);
            if old == Some(seq.saved_run()) {
                return;
            }
            let mut run = codec::sequencer_to_nbt(seq);
            run.remove("Instructions");
            self.persisted.sequences.entry(p).or_default().extend(run);
        }
    }
    pub(super) fn mark_column(&mut self, p: Pos) {
        self.persisted.dirty_columns.insert((p[0] >> 4, p[2] >> 4));
    }
    pub(super) fn save_depot_records(&mut self) {
        self.persisted
            .depots
            .retain(|p, _| self.w.saved_depot(0, *p).is_some());
        for &p in self.w.dims[0].depots.keys() {
            if let Some(saved) = self.w.saved_depot(0, p) {
                self.persisted.depots.insert(p, codec::depot_to_nbt(saved));
            }
        }
    }
    pub(super) fn save_belt_records(&mut self, changed: &[Pos]) {
        for c in changed {
            let members = self.w.dims[0]
                .belts
                .get(c)
                .map(|b| b.chain.clone())
                .unwrap_or_default();
            for p in members {
                if let Some(saved) = self.w.saved_belt_segment(0, p) {
                    self.persisted
                        .belts
                        .insert(p, codec::belt_segment_to_nbt(saved));
                }
            }
        }
    }
    pub(super) fn save_kinetic_records(&mut self) {
        self.persisted
            .handles
            .retain(|p, _| self.w.saved_handle(0, *p).is_some());
        for p in self.w.take_handle_changes(0) {
            if let Some(saved) = self.w.saved_handle(0, p) {
                self.persisted
                    .handles
                    .insert(p, codec::handle_to_nbt(saved));
            }
        }
        for p in self.w.dims[0].kinetics.take_saved_changes() {
            if self.w.dims[0].kinetics.awaiting_legacy_load(p) {
                continue;
            }
            for sub in 0..2 {
                if let Some(saved) = self.w.dims[0].kinetics.saved_state(p, sub) {
                    self.persisted
                        .kinetics
                        .insert((p, sub), codec::saved_to_nbt(saved));
                } else {
                    self.persisted.kinetics.remove(&(p, sub));
                }
            }
            if let Some(signal) = self.w.saved_chain_signal(0, p) {
                self.persisted
                    .chain_entities
                    .insert(p, Compound::from([("Signal".into(), Tag::Int(signal))]));
            }
        }
    }
    pub(super) fn mark_package_columns(&mut self) {
        for p in self.chains.take_changed() {
            self.mark_column(p);
        }
    }
    /// Subsystem::write_entities: after all mutations, before column attachment flush.
    pub(super) fn publish_entities(&mut self) {
        for c in self.w.take_belt_saved_changes(0) {
            self.mark_column(c);
        }
        self.save_depot_records();
        let positions: Vec<_> = self.w.dims[0].sequencers.keys().copied().collect();
        for p in positions {
            self.save_sequence(p);
        }
    }
    /// persist::flush: only marked column attachments, after entity publication.
    pub(super) fn flush_persistence(&mut self) {
        let columns = std::mem::take(&mut self.persisted.dirty_columns);
        if columns.is_empty() {
            return;
        }
        let outside = |p: Pos| !columns.contains(&(p[0] >> 4, p[2] >> 4));
        self.persisted.brackets.retain(|p, _| outside(*p));
        self.persisted.conveyors.retain(|p, _| outside(*p));
        self.persisted.brackets.extend(
            self.brackets
                .iter()
                .filter(|(p, _)| !outside(**p))
                .map(|(&p, &b)| (p, b)),
        );
        self.persisted.conveyors.extend(
            self.chains
                .conveyors
                .iter()
                .filter(|(p, _)| !outside(**p))
                .map(|(&p, c)| (p, c.connections.clone())),
        );
        let mut motors = match self.persisted.columns.get("motors") {
            Some(Tag::IntArray(v)) => v
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| ([v[0], v[1], v[2]], v[3]))
                .filter(|(p, _)| outside(*p))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        let mut belts = codec::belts_from_nbt(&self.persisted.columns)
            .into_iter()
            .filter(|(p, _)| outside(*p))
            .collect::<Vec<_>>();
        for (cx, cz) in columns {
            motors.extend(self.w.column_settings(0, cx, cz));
            belts.extend(self.w.column_belts(0, cx, cz));
            let data = crate::persistence::fluids::save_column(&self.w, (cx, cz));
            if data.is_empty() {
                self.persisted.fluid_columns.remove(&(cx, cz));
            } else {
                self.persisted.fluid_columns.insert((cx, cz), data);
            }
        }
        self.persisted.columns = Compound::new();
        if !motors.is_empty() {
            self.persisted.columns.insert(
                "motors".into(),
                Tag::IntArray(
                    motors
                        .iter()
                        .flat_map(|(p, v)| [p[0], p[1], p[2], *v])
                        .collect(),
                ),
            );
        }
        if !belts.is_empty() {
            self.persisted
                .columns
                .insert("belts".into(), codec::belts_to_nbt(&belts));
        }
    }
    pub(super) fn reload(&mut self) {
        // decor/saved.rs: column bracket entries; belt casing is host entity data.
        let brackets: Vec<[i32; 7]> = self
            .persisted
            .brackets
            .iter()
            .map(|(p, b)| {
                [
                    p[0],
                    p[1],
                    p[2],
                    i32::from(b.wooden),
                    b.facing as i32,
                    i32::from(b.along_first),
                    match b.kind {
                        create_engine::decor::BracketKind::Pipe => 0,
                        create_engine::decor::BracketKind::Cog => 1,
                        create_engine::decor::BracketKind::Shaft => 2,
                    },
                ]
            })
            .collect();
        let metadata =
            serde_json::to_vec(&(brackets, self.belt_casing.iter().collect::<Vec<_>>())).unwrap();
        let (brackets, casing): (Vec<[i32; 7]>, Vec<(Pos, String)>) =
            serde_json::from_slice(&metadata).unwrap();
        self.brackets = brackets
            .into_iter()
            .map(|v| {
                (
                    [v[0], v[1], v[2]],
                    create_engine::decor::Bracket {
                        wooden: v[3] != 0,
                        facing: create_engine::kinetics::Dir::ALL[v[4].clamp(0, 5) as usize],
                        along_first: v[5] != 0,
                        kind: match v[6] {
                            0 => create_engine::decor::BracketKind::Pipe,
                            1 => create_engine::decor::BracketKind::Cog,
                            _ => create_engine::decor::BracketKind::Shaft,
                        },
                    },
                )
            })
            .collect();
        self.belt_casing = casing.into_iter().collect();
        let mut blocks: Vec<_> = self.world.iter().map(|(&p, &s)| (p, s)).collect();
        blocks.sort_by_key(|(p, _)| *p);
        let columns: std::collections::BTreeSet<_> =
            blocks.iter().map(|(p, _)| (p[0] >> 4, p[2] >> 4)).collect();
        let connections: Vec<_> = self
            .persisted
            .conveyors
            .iter()
            .map(|(&p, c)| (p, c.clone()))
            .collect();
        let connections: Vec<(Pos, Vec<Pos>)> =
            serde_json::from_slice(&serde_json::to_vec(&connections).unwrap()).unwrap();
        let unloaded: Vec<_> = self.chains.conveyors.keys().copied().collect();
        for p in unloaded {
            self.chains.unload(p);
        }

        let mut entities = Vec::new();
        let mut sequences = Vec::new();
        for &(p, _) in &blocks {
            for sub in 0..2 {
                if let Some(data) = self.persisted.kinetics.get(&(p, sub)) {
                    entities.push((p, sub, data.clone()));
                }
            }
            if self.w.block_at(0, p) == Some(create_engine::Block::SequencedGearshift)
                && let Some(data) = self.persisted.sequences.get(&p)
            {
                sequences.push((p, data.clone()));
            }
        }
        // Exercise a real value-tree encoding boundary; no live kinetic state survives it.
        let data = self.persisted.columns.clone();
        let bytes = serde_json::to_vec(&(
            data,
            entities,
            sequences,
            self.depot_stacks.iter().collect::<Vec<_>>(),
            self.handles.iter().collect::<Vec<_>>(),
            self.persisted.handles.iter().collect::<Vec<_>>(),
            self.persisted.depots.iter().collect::<Vec<_>>(),
            self.persisted.belts.iter().collect::<Vec<_>>(),
            FluidReload {
                columns: self
                    .persisted
                    .fluid_columns
                    .iter()
                    .map(|(&p, d)| (p, d.clone()))
                    .collect(),
                entities: self
                    .persisted
                    .fluid_entities
                    .iter()
                    .map(|(&p, d)| (p, d.clone()))
                    .collect(),
            },
            self.persisted.chain_entities.iter().collect::<Vec<_>>(),
        ))
        .unwrap();
        let (
            data,
            entities,
            sequences,
            slots,
            handles,
            saved_handles,
            saved_depots,
            saved_belts,
            fluids,
            chains,
        ): ReloadData = serde_json::from_slice(&bytes).unwrap();
        self.depot_stacks = slots.into_iter().collect();
        self.handles = handles.into_iter().collect();
        for &(cx, cz) in &columns {
            self.w.column_unloaded(0, cx, cz);
        }
        let motors = match data.get("motors") {
            Some(Tag::IntArray(v)) => v
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| ([v[0], v[1], v[2]], v[3]))
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        self.w.load_settings(0, motors.into_iter());
        self.w
            .load_belt_items(0, codec::belts_from_nbt(&data).into_iter());
        // ensure_column runs every subsystem's chunk_load before the first entity callback.
        for (_, data) in &fluids.columns {
            crate::persistence::fluids::load_column(&mut self.w, data);
        }
        self.persisted.fluid_entities = fluids.entities.into_iter().collect();
        let chains: HashMap<_, _> = chains.into_iter().collect();
        let saved_belts: HashMap<_, _> = saved_belts.into_iter().collect();
        let saved_depots: HashMap<_, _> = saved_depots.into_iter().collect();
        let saved_handles: HashMap<_, _> = saved_handles.into_iter().collect();
        let sequences: HashMap<_, _> = sequences.into_iter().collect();
        let entities: HashMap<_, _> = entities
            .into_iter()
            .map(|(p, sub, data)| ((p, sub), data))
            .collect();
        let restored_depots: std::collections::HashSet<_> = saved_depots.keys().copied().collect();
        // Complete one block_entity_on_load callback before starting the next one.
        for &(p, s) in &blocks {
            self.w.block_loaded(0, p, s);
            let signal = match chains.get(&p).and_then(|data| data.get("Signal")) {
                Some(Tag::Int(signal)) => *signal,
                _ => 0,
            };
            self.w.restore_chain_signal(0, p, signal);
            // KineticEntity::block_entity_on_load does not consume Tank/Hose drawing NBT.
            // Fluid state was staged by chunk_load and is consumed by block_loaded above.
            if let Some(saved) = saved_belts.get(&p).and_then(codec::belt_segment_from_nbt) {
                self.w.restore_belt_segment(0, p, saved);
            }
            if let Some(data) = saved_depots.get(&p) {
                self.w.restore_depot(0, p, codec::depot_from_nbt(data));
            }
            if let Some(data) = saved_handles.get(&p) {
                self.w.restore_handle(0, p, codec::handle_from_nbt(data));
            }
            for sub in 0..2 {
                if let Some(saved) = entities.get(&(p, sub)).and_then(codec::saved_from_nbt) {
                    assert!(
                        self.w.dims[0].kinetics.restore_state(p, sub, saved),
                        "saved kinetic part missing at {p:?} port {sub}"
                    );
                }
            }
            if self.w.block_at(0, p) == Some(create_engine::Block::SequencedGearshift)
                && let Some(data) = sequences.get(&p)
            {
                let instructions = codec::of_tag(data);
                if !instructions.is_empty() {
                    self.w.set_sequencer_instructions(0, p, instructions, false);
                }
                if let Some(run) = codec::run_of_tag(data) {
                    self.w.restore_sequencer(0, p, run);
                }
            }
        }
        for (&p, h) in &mut self.handles {
            if let Some(saved) = self.w.saved_handle(0, p) {
                h.remaining = saved.in_use;
                h.total = saved.total_use_ticks;
                h.back = saved.backwards;
                h.target = saved.target_angle;
            }
        }
        for (p, links) in connections {
            self.chains.add(p, links);
        }
        self.sync_conveyors();
        // Only legacy host slots without DepotState initialize at rest. An explicit
        // empty record must not resurrect an old held transport state.
        for &p in self.depot_stacks.keys() {
            if !restored_depots.contains(&p) && self.w.dims[0].depots.contains_key(&p) {
                self.w
                    .depot_put(0, p, Some(create_engine::depot::Held::new(180, false)));
            }
        }
    }
}

impl Run {
    pub(super) fn saved_payload(&self, p: Pos) -> Option<crate::persistence::payload::Payload> {
        use crate::persistence::payload::{Inventory, Item, Payload};
        use create_engine::Block;
        let block = self.w.block_at(0, p)?;
        Some(match block {
            Block::SequencedGearshift => {
                let data = self
                    .persisted
                    .sequences
                    .get(&p)
                    .cloned()
                    .unwrap_or_default();
                let run = codec::run_of_tag(&data).unwrap_or(create_engine::sequencer::Run {
                    current: -1,
                    duration: -1,
                    progress: 0.,
                    timer: 0,
                    powered_before: false,
                });
                let mut saved_instructions = codec::of_tag(&data);
                if saved_instructions.is_empty() {
                    saved_instructions = create_engine::sequencer::default_instructions();
                }
                let instructions=saved_instructions.iter().map(|i|serde_json::from_value(serde_json::json!({"instruction":i.kind.name(),"modifier":i.speed.name(),"value":i.value})).unwrap()).collect();
                Payload::Sequencer {
                    instructions,
                    current: run.current,
                    duration: run.duration,
                    progress: run.progress,
                    timer: run.timer,
                    powered: run.powered_before,
                }
            }
            Block::HandCrank => {
                let h = self
                    .persisted
                    .handles
                    .get(&p)
                    .map(codec::handle_from_nbt)
                    .unwrap_or_default();
                Payload::Crank {
                    in_use: h.in_use,
                    backwards: h.backwards,
                }
            }
            Block::Depot => Payload::Depot {
                held: self.depot_stacks.get(&p).cloned(),
            },
            Block::Belt => {
                let saved = self
                    .persisted
                    .belts
                    .get(&p)
                    .and_then(codec::belt_segment_from_nbt);
                let (controller, index, length) =
                    saved.map_or((p, 0, 0), |s| (s.controller, s.index, s.length));
                let inventory = (saved.is_some() && p == controller).then(|| {
                    let inv = codec::belts_from_nbt(&self.persisted.columns)
                        .into_iter()
                        .find(|(c, _)| *c == controller)
                        .map(|(_, inv)| inv)
                        .unwrap_or_default();
                    Inventory {
                        positive: inv.positive,
                        items: inv
                            .items
                            .iter()
                            .map(|i| Item {
                                stack: crate::transport::Stack {
                                    item: i.item.clone(),
                                    count: i.count,
                                },
                                position: i.position,
                                locked: i.locked,
                            })
                            .collect(),
                    }
                });
                Payload::Belt {
                    controller,
                    length,
                    index,
                    casing: self
                        .belt_casing
                        .get(&p)
                        .cloned()
                        .unwrap_or_else(|| "none".into()),
                    inventory,
                }
            }
            b if create_engine::dye::is_valve_handle(b) => {
                let h = self
                    .persisted
                    .handles
                    .get(&p)
                    .map(codec::handle_from_nbt)
                    .unwrap_or_default();
                Payload::Valve {
                    selected: self.handles.get(&p)?.selected,
                    total: i32::from(h.total_use_ticks),
                    target: h.target_angle,
                    in_use: h.in_use,
                    backwards: h.backwards,
                    start_angle: h.start_angle,
                }
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crank_and_valve_use_round_trip_with_expiry_and_no_cooldown() {
        for valve in [false, true] {
            for back in [false, true] {
                let state = if valve {
                    "create:copper_valve_handle[facing=up]"
                } else {
                    "create:hand_crank[facing=up]"
                };
                let sc: Scenario = serde_json::from_value(
                    serde_json::json!({"ticks":20,"blocks":[{"pos":[0,1,0],"state":state}]}),
                )
                .unwrap();
                let mut r = Run::new(&sc);
                let p = [0, 1, 0];
                let id = r.id(state);
                r.world.insert(p, id);
                r.w.block_loaded(0, p, id);
                if valve {
                    assert!(r.w.use_valve(0, p, back, -45));
                } else {
                    assert!(r.w.turn_crank(0, p, back));
                }
                r.w.tick_cranks(0);
                r.w.tick(0, &mut Vec::new());
                r.save_kinetic_records();
                let saved = r.w.saved_handle(0, p).unwrap();
                assert_eq!(saved.in_use, if valve { 6 } else { 9 });
                let rpm = if back ^ valve { -32. } else { 32. };
                r.reload();
                assert_eq!(r.w.saved_handle(0, p), Some(saved));
                assert_eq!(r.w.dims[0].kinetics.generated(p, 0), Some(rpm));
                r.w.tick(0, &mut Vec::new());
                let net = r.w.dims[0]
                    .kinetics
                    .saved_state(p, 0)
                    .unwrap()
                    .network
                    .unwrap();
                assert_eq!((net.capacity, net.size), (256., 1));
                // Retained old generation remains in the registry, while the saved
                // per-entity cached size remains one until Create prunes/syncs it.
                assert_eq!(r.w.dims[0].kinetics.network_membership(p, 0).unwrap().1, 2);
                for _ in 1..saved.in_use {
                    r.w.tick_cranks(0);
                    r.save_kinetic_records();
                }
                assert_eq!(r.w.dims[0].kinetics.generated(p, 0), Some(rpm));
                r.w.tick_cranks(0);
                r.w.tick(0, &mut Vec::new());
                r.save_kinetic_records();
                assert_eq!(r.w.saved_handle(0, p).unwrap().in_use, 0);
                assert_eq!(r.w.dims[0].kinetics.generated(p, 0), Some(0.));
                if valve {
                    assert!(r.w.use_valve(0, p, false, 45));
                }
            }
        }
    }
    #[test]
    fn continuing_sequencer_fields_require_end_of_tick_publication() {
        use create_engine::sequencer::{Instruction, Kind, Sequencer, Speed};
        let mut seq = Sequencer {
            instructions: vec![Instruction {
                kind: Kind::TurnAngle,
                speed: Speed::Forward,
                value: 90,
            }],
            speed: 32.,
            ..Default::default()
        };
        seq.run(0);
        let written = codec::sequencer_to_nbt(&seq);
        // A continuing instruction emits no transition. The post-subsystem
        // publisher must nevertheless capture its progress and timer.
        assert!(seq.tick().is_none());
        let persisted = codec::run_of_tag(&written).unwrap();
        let current = codec::run_of_tag(&codec::sequencer_to_nbt(&seq)).unwrap();
        assert_eq!((persisted.progress, persisted.timer), (0., 0));
        assert!(current.progress > persisted.progress);
        assert_eq!(current.timer, 1);
    }
    #[test]
    fn handle_records_follow_countdowns_and_replacement() {
        let sc:Scenario=serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[{"pos":[0,1,0],"state":"create:hand_crank[facing=up]"}]})).unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 1, 0];
        r.place(&sc.blocks[0]);
        r.w.turn_crank(0, p, false);
        for remaining in (0..10).rev() {
            r.tick();
            assert_eq!(
                codec::handle_from_nbt(&r.persisted.handles[&p]).in_use,
                remaining
            );
        }
        r.set(p, AIR);
        assert!(!r.persisted.handles.contains_key(&p));
        r.place(&sc.blocks[0]);
        r.tick();
        assert!(!r.persisted.handles.contains_key(&p));
        assert_eq!(r.w.saved_handle(0, p).unwrap().in_use, 0);
    }
    #[test]
    fn depot_motion_byte_round_trip_matches_java_after_reload() {
        let sc:Scenario=serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[{"pos":[0,8,5],"state":"create:depot[waterlogged=false]"}]})).unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 8, 5];
        r.place(&sc.blocks[0]);
        r.depot_stacks.insert(
            p,
            crate::transport::Stack {
                item: "minecraft:diamond".into(),
                count: 2,
            },
        );
        r.w.depot_put(
            0,
            p,
            Some(create_engine::depot::Held::put_by_player(
                90,
                false,
                create_engine::depot::Side::West,
            )),
        );
        for _ in 0..9 {
            r.w.tick_depots(0, |_| false);
            r.save_depot_records();
        }
        let saved = r.w.saved_depot(0, p).unwrap();
        r.reload();
        assert_eq!(r.w.saved_depot(0, p), Some(saved));
        for expected in [0.48592162, 0.48944122, 0.49208093] {
            r.w.tick_depots(0, |_| false);
            r.save_depot_records();
            let actual = codec::depot_from_nbt(&r.persisted.depots[&p])
                .held
                .unwrap()
                .position;
            assert!((actual - expected).abs() < 0.0000001);
        }
        r.w.depot_put(0, p, None);
        r.depot_stacks.remove(&p);
        r.save_depot_records();
        assert!(r.persisted.depots[&p].is_empty());
        r.reload();
        assert_eq!(r.w.saved_depot(0, p).unwrap().held, None);
        r.set(p, AIR);
        assert!(!r.persisted.depots.contains_key(&p));
    }
    #[test]
    fn only_missing_depot_records_use_legacy_slot_initialization() {
        let sc:Scenario=serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[{"pos":[0,1,0],"state":"create:depot[waterlogged=false]"}]})).unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 1, 0];
        r.place(&sc.blocks[0]);
        r.depot_stacks.insert(
            p,
            crate::transport::Stack {
                item: "minecraft:diamond".into(),
                count: 1,
            },
        );
        r.reload();
        assert_eq!(r.w.saved_depot(0, p).unwrap().held.unwrap().position, 0.5);
        r.w.depot_put(0, p, None);
        r.save_depot_records();
        r.reload();
        assert_eq!(r.w.saved_depot(0, p).unwrap().held, None);
    }
    #[test]
    fn cold_belt_load_forms_controller_before_first_inventory_tick() {
        let sc:Scenario=serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[{"pos":[0,8,0],"state":"create:belt[casing=false,facing=east,part=start,slope=horizontal,waterlogged=false]"}]})).unwrap();
        let mut r = Run::new(&sc);
        let c = [0, 8, 0];
        let mut inv = create_engine::belt::Inventory::default();
        let mut item = create_engine::belt::Item::new("minecraft:iron_ingot", 4, 180, false);
        item.position = 1.4;
        inv.add(item);
        r.w.load_belt_items(0, std::iter::once((c, inv.to_save())));
        for (x, part) in [(0, "start"), (1, "middle"), (2, "end")] {
            let state = r.id(&format!(
                "create:belt[casing=false,facing=east,part={part},slope=horizontal,waterlogged=false]"
            ));
            r.w.block_loaded(0, [x, 8, 0], state);
        }
        assert!(!r.w.dims[0].kinetics.has_part(c, 0));
        let mut phase = r.w.belt_ticks(0);
        assert_eq!(phase.next(&mut r.w), Some(c));
        assert_eq!(r.w.dims[0].belts[&c].items.items[0].position, 1.4);
        assert!(r.w.dims[0].kinetics.has_part(c, 0));
    }
    #[test]
    fn saved_membership_and_insertion_survive_two_belt_reloads() {
        let sc: Scenario =
            serde_json::from_str(include_str!("../persistence/belt-reload-case.json")).unwrap();
        let mut r = Run::new(&sc);
        let c = [0, 8, 0];
        let mut reloads = 0;
        for t in 0_u32..46 {
            if let Some(b) = sc.blocks.get(t as usize) {
                r.place(b);
            }
            if let Some(b) = t.checked_sub(1).and_then(|i| sc.blocks.get(i as usize)) {
                r.set_motor(b);
            }
            for e in sc.events.iter().filter(|e| e.tick == t) {
                if let Some(a) = &e.action {
                    let clock = r.persisted.clock;
                    r.action(e.pos, a);
                    if matches!(a, crate::actions::Action::Reload) {
                        reloads += 1;
                        assert_eq!(r.persisted.clock, clock);
                        for &p in &r.w.dims[0].belts[&c].chain {
                            let saved =
                                codec::belt_segment_from_nbt(&r.persisted.belts[&p]).unwrap();
                            assert_eq!(r.w.saved_belt_segment(0, p), Some(saved));
                            assert!(r.w.dims[0].kinetics.has_part(p, 0));
                        }
                        let item = &r.w.dims[0].belts[&c].items.items[0];
                        assert_eq!(item.inserted_at, 1);
                        assert_eq!(item.inserted_from, Some(create_engine::kinetics::Dir::Up));
                    }
                }
            }
            r.tick();
            if t == 41 {
                assert!((r.w.dims[0].belts[&c].items.items[0].position - 1.4291666).abs() < 1e-6);
            }
        }
        assert_eq!(reloads, 2);
    }
    #[test]
    fn empty_belt_direction_survives_two_zero_speed_reloads() {
        let mut sc = crate::transport::generate(63);
        sc.events.retain(|e| e.tick <= 20);
        let mut r = Run::new(&sc);
        let c = [0, 8, 0];
        for t in 0_u32..30 {
            if let Some(b) = sc.blocks.get(t as usize) {
                r.place(b);
            }
            if let Some(b) = t.checked_sub(1).and_then(|i| sc.blocks.get(i as usize)) {
                r.set_motor(b);
            }
            for e in sc.events.iter().filter(|e| e.tick == t) {
                if let Some(a) = &e.action {
                    r.action(e.pos, a);
                }
            }
            r.tick();
        }
        assert!(r.w.dims[0].belts[&c].items.positive);
        r.set([-1, 8, 0], AIR);
        r.tick();
        r.tick();
        for _ in 0..2 {
            assert_eq!(r.w.speed(0, c), Some(0.));
            let saved = codec::belts_from_nbt(&r.persisted.columns);
            let inv = &saved.iter().find(|(p, _)| *p == c).unwrap().1;
            assert!(inv.is_empty() && inv.positive);
            r.reload();
            let b = &r.w.dims[0].belts[&c];
            assert!(b.items.is_empty() && b.items.positive);
            assert_eq!(b.chain.len(), 5);
            r.tick();
        }
    }
    #[test]
    fn reload_histories_run_without_reusing_live_belt_entities() {
        for seed in [41, 43, 47] {
            let sc = crate::generate::generate(seed);
            let snap = crate::engine::run(&sc);
            assert_eq!(snap.checkpoints.len(), sc.checkpoints.len());
            assert!(!snap.blocks.is_empty());
        }
    }
}

#[cfg(test)]
mod write_phase_tests {
    use super::*;
    #[test]
    fn depot_handoff_is_published_after_all_subsystems_before_reload() {
        let mut sc = crate::transport::generate(63);
        sc.events.retain(|e| e.tick <= 40);
        let mut r = Run::new(&sc);
        let p = [0, 8, 5];
        for t in 0_u32..150 {
            if let Some(b) = sc.blocks.get(t as usize) {
                r.place(b);
            }
            if let Some(b) = t.checked_sub(1).and_then(|i| sc.blocks.get(i as usize)) {
                r.set_motor(b);
            }
            for e in sc.events.iter().filter(|e| e.tick == t) {
                if let Some(a) = &e.action {
                    r.action(e.pos, a);
                }
            }
            r.tick();
            if r.depot_stacks.contains_key(&p) {
                assert_eq!(t + 1, 90);
                assert_eq!(r.w.saved_depot(0, p).unwrap().held.unwrap().position, 0.);
                assert_eq!(
                    codec::depot_from_nbt(&r.persisted.depots[&p]),
                    r.w.saved_depot(0, p).unwrap()
                );
                r.reload();
                assert_eq!(r.w.saved_depot(0, p).unwrap().held.unwrap().position, 0.);
                r.tick();
                assert_eq!(r.w.saved_depot(0, p).unwrap().held.unwrap().position, 0.125);
                return;
            }
        }
        panic!("no handoff");
    }
    #[test]
    fn kinetic_cache_calculation_without_dirty_event_is_not_written() {
        let sc: Scenario = serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[
            {"pos":[0,7,0],"state":"minecraft:stone"},
            {"pos":[0,8,0],"state":"create:hand_crank[facing=down]"},
            {"pos":[0,9,0],"state":"create:creative_motor[facing=down]","motor":32}]}))
        .unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 8, 0];
        for b in &sc.blocks {
            r.place(b);
            r.set_motor(b);
        }
        r.tick();
        let written = serde_json::to_vec(&r.persisted.kinetics[&(p, 0)]).unwrap();
        assert_eq!(
            r.w.dims[0]
                .kinetics
                .saved_state(p, 0)
                .unwrap()
                .network
                .unwrap()
                .added_capacity,
            0.
        );
        assert_eq!(
            r.w.dims[0].kinetics.calculate_added_stress_capacity(p, 0),
            Some(8.)
        );
        r.save_kinetic_records();
        assert_eq!(
            serde_json::to_vec(&r.persisted.kinetics[&(p, 0)]).unwrap(),
            written
        );
        assert_eq!(
            r.w.dims[0]
                .kinetics
                .saved_state(p, 0)
                .unwrap()
                .network
                .unwrap()
                .added_capacity,
            8.
        );
    }
    #[test]
    fn sequence_run_publication_follows_progress_even_without_a_step() {
        let sc = crate::transmission::sequence(11);
        let mut r = Run::new(&sc);
        let p = [0, 8, 0];
        for t in 0_u32..22 {
            if let Some(b) = sc.blocks.get(t as usize) {
                r.place(b);
            }
            if let Some(b) = t.checked_sub(1).and_then(|i| sc.blocks.get(i as usize)) {
                r.set_motor(b);
            }
            for e in sc.events.iter().filter(|e| e.tick == t) {
                if let Some(a) = &e.action {
                    r.action(e.pos, a);
                }
            }
            r.tick();
            if let Some(live) = r.w.dims[0].sequencers.get(&p) {
                assert_eq!(
                    codec::run_of_tag(&r.persisted.sequences[&p]),
                    Some(live.saved_run())
                );
            }
        }
        assert!(r.w.dims[0].sequencers[&p].progress > 0.);
    }
    #[test]
    fn column_flush_cannot_capture_unmarked_live_inventory_changes() {
        let sc:Scenario=serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[{"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32}]})).unwrap();
        let mut r = Run::new(&sc);
        r.place(&sc.blocks[0]);
        r.set_motor(&sc.blocks[0]);
        r.tick();
        let written = serde_json::to_vec(&r.persisted.columns).unwrap();
        r.w.board_motor(
            0,
            [0, 8, 0],
            create_engine::board::Settings { row: 1, value: 64 },
        );
        // World changes become eligible when kinetics::Solve drains take_unsaved,
        // rather than through an unconditional late getter scan.
        r.flush_persistence();
        assert_eq!(serde_json::to_vec(&r.persisted.columns).unwrap(), written);
        r.tick();
        assert_ne!(serde_json::to_vec(&r.persisted.columns).unwrap(), written);
    }
}

#[cfg(test)]
mod reload_order_tests {
    use super::*;
    #[test]
    fn chain_signal_journal_round_trips_before_attachment_and_defaults_to_zero() {
        let sc: Scenario = serde_json::from_value(serde_json::json!({"version":3,"ticks":20,"blocks":[
            {"pos":[0,8,-1],"state":"create:creative_motor[facing=south]","motor":32},
            {"pos":[0,8,0],"state":"create:adjustable_chain_gearshift[axis=z,axis_along_first=true,part=start,powered=false]"},
            {"pos":[1,8,0],"state":"create:encased_chain_drive[axis=z,axis_along_first=true,part=end]"}
        ]})).unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 8, 0];
        for b in &sc.blocks {
            r.place(b);
            r.set_motor(b);
        }
        r.tick();
        r.set([0, 9, 0], 6); // controlled signal 3
        r.tick();
        assert!(matches!(
            r.persisted.chain_entities[&p].get("Signal"),
            Some(Tag::Int(3))
        ));
        for _ in 0..2 {
            r.reload();
            assert_eq!(r.w.saved_chain_signal(0, p), Some(3));
            r.tick();
            assert_eq!(r.w.speed(0, p), Some(32.));
            assert_eq!(r.w.speed(0, [1, 8, 0]), Some(40.));
        }
        r.set([0, 9, 0], 18); // external power is newer than the stored entity record
        r.reload();
        assert_eq!(r.w.saved_chain_signal(0, p), Some(3));
        r.persisted.chain_entities.clear();
        r.reload();
        assert_eq!(r.w.saved_chain_signal(0, p), Some(0));
        r.persisted
            .chain_entities
            .insert(p, Compound::from([("Signal".into(), Tag::Int(3))]));
        r.set(p, AIR);
        assert!(!r.persisted.chain_entities.contains_key(&p));
    }

    #[test]
    fn processed_signal_is_saved_without_a_speed_change() {
        let sc: Scenario = serde_json::from_value(serde_json::json!({"version":3,"ticks":20,"blocks":[
            {"pos":[0,8,0],"state":"create:adjustable_chain_gearshift[axis=z,axis_along_first=true,part=none,powered=false]"}
        ]})).unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 8, 0];
        r.place(&sc.blocks[0]);
        r.tick();
        for signal in [3, 5] {
            r.set([0, 9, 0], 3 + signal);
            r.tick();
            assert_eq!(r.w.speed(0, p), Some(0.));
            assert!(
                matches!(r.persisted.chain_entities[&p].get("Signal"),Some(Tag::Int(s)) if *s==signal as i32)
            );
        }
    }
    fn before_reload() -> Run {
        let sc: Scenario =
            serde_json::from_str(include_str!("../persistence/sequencer-reload-case.json"))
                .unwrap();
        let mut r = Run::new(&sc);
        for t in 0_u32..22 {
            if let Some(b) = sc.blocks.get(t as usize) {
                r.place(b);
            }
            if let Some(b) = t.checked_sub(1).and_then(|i| sc.blocks.get(i as usize)) {
                r.set_motor(b);
            }
            for e in sc.events.iter().filter(|e| e.tick == t) {
                if let Some(a) = &e.action {
                    r.action(e.pos, a);
                }
            }
            r.tick();
        }
        r
    }
    #[test]
    fn reload_matches_each_glue_load_callback_before_the_next_entity() {
        let sc: Scenario =
            serde_json::from_str(include_str!("../persistence/sequencer-reload-case.json"))
                .unwrap();
        let actual = {
            let mut r = before_reload();
            r.reload();
            r.snapshot(&sc)
        };
        let expected = {
            let mut r = before_reload();
            let mut blocks: Vec<_> = r.world.iter().map(|(&p, &s)| (p, s)).collect();
            blocks.sort_by_key(|(p, _)| *p);
            let columns: std::collections::BTreeSet<_> =
                blocks.iter().map(|(p, _)| (p[0] >> 4, p[2] >> 4)).collect();
            let kinetic = r.persisted.kinetics.clone();
            let sequence = r.persisted.sequences.clone();
            let motors = match r.persisted.columns.get("motors") {
                Some(Tag::IntArray(v)) => v
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|v| ([v[0], v[1], v[2]], v[3]))
                    .collect::<Vec<_>>(),
                _ => vec![],
            };
            for (cx, cz) in columns {
                r.w.column_unloaded(0, cx, cz);
            }
            r.w.load_settings(0, motors.into_iter());
            // Independent fixture reference for kinetics.rs::block_entity_on_load:
            // this case has kinetic entities and one sequencer, no belt/handle/depot.
            for (p, s) in blocks {
                r.w.block_loaded(0, p, s);
                for sub in 0..2 {
                    if let Some(saved) = kinetic.get(&(p, sub)).and_then(codec::saved_from_nbt) {
                        assert!(r.w.dims[0].kinetics.restore_state(p, sub, saved));
                    }
                }
                if let Some(data) = sequence.get(&p) {
                    let instructions = codec::of_tag(data);
                    if !instructions.is_empty() {
                        r.w.set_sequencer_instructions(0, p, instructions, false);
                    }
                    if let Some(run) = codec::run_of_tag(data) {
                        r.w.restore_sequencer(0, p, run);
                    }
                }
            }
            r.snapshot(&sc)
        };
        assert_eq!(actual, expected);
    }
}
