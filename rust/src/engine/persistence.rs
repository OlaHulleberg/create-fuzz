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
}
type ReloadData = (
    Compound,
    Vec<(Pos, u8, Compound)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, crate::transport::Stack)>,
    Vec<(Pos, crate::handles::Handle)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, Compound)>,
);

impl Run {
    pub(super) fn save_sequence(&mut self, p: Pos) {
        if let Some(s) = self.w.dims[0].sequencers.get(&p) {
            self.persisted
                .sequences
                .insert(p, codec::sequencer_to_nbt(s));
        }
    }
    pub(super) fn flush_persistence(&mut self) {
        self.persisted
            .depots
            .retain(|p, _| self.w.saved_depot(0, *p).is_some());
        for &p in self.w.dims[0].depots.keys() {
            if let Some(saved) = self.w.saved_depot(0, p) {
                self.persisted.depots.insert(p, codec::depot_to_nbt(saved));
            }
        }
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
            for sub in 0..2 {
                if let Some(saved) = self.w.dims[0].kinetics.saved_state(p, sub) {
                    self.persisted
                        .kinetics
                        .insert((p, sub), codec::saved_to_nbt(saved));
                } else {
                    self.persisted.kinetics.remove(&(p, sub));
                }
            }
        }
        let columns: std::collections::BTreeSet<_> =
            self.world.keys().map(|p| (p[0] >> 4, p[2] >> 4)).collect();
        let mut motors = Vec::new();
        let mut belts = Vec::new();
        for (cx, cz) in columns {
            motors.extend(self.w.column_settings(0, cx, cz));
            belts.extend(self.w.column_belts(0, cx, cz));
        }
        self.persisted.columns = Compound::from([
            (
                "motors".into(),
                Tag::IntArray(
                    motors
                        .iter()
                        .flat_map(|(p, v)| [p[0], p[1], p[2], *v])
                        .collect(),
                ),
            ),
            ("belts".into(), codec::belts_to_nbt(&belts)),
        ]);
    }
    pub(super) fn reload(&mut self) {
        // decor/saved.rs: column bracket entries; belt casing is host entity data.
        let brackets: Vec<[i32; 7]> = self
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
            .chains
            .conveyors
            .iter()
            .map(|(&p, c)| (p, c.connections.clone()))
            .collect();
        let connections: Vec<(Pos, Vec<Pos>)> =
            serde_json::from_slice(&serde_json::to_vec(&connections).unwrap()).unwrap();
        for (p, _) in &connections {
            self.chains.unload(*p);
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
        ))
        .unwrap();
        let (data, entities, sequences, slots, handles, saved_handles, saved_depots): ReloadData =
            serde_json::from_slice(&bytes).unwrap();
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
        for &(p, s) in &blocks {
            self.w.block_loaded(0, p, s);
        }
        let restored_depots: std::collections::HashSet<_> =
            saved_depots.iter().map(|(p, _)| *p).collect();
        for (p, data) in saved_depots {
            self.w.restore_depot(0, p, codec::depot_from_nbt(&data));
        }
        for (p, data) in saved_handles {
            self.w.restore_handle(0, p, codec::handle_from_nbt(&data));
        }
        for (&p, h) in &mut self.handles {
            if let Some(saved) = self.w.saved_handle(0, p) {
                h.remaining = saved.in_use;
                h.total = saved.total_use_ticks;
                h.back = saved.backwards;
                h.target = saved.target_angle;
            }
        }
        // block_entity_on_load: the scan may already have registered this block.
        for (p, sub, data) in entities {
            if let Some(saved) = codec::saved_from_nbt(&data) {
                self.w.dims[0].kinetics.restore_state(p, sub, saved);
            }
        }
        for (p, data) in sequences {
            let instructions = codec::of_tag(&data);
            if !instructions.is_empty() {
                self.w.set_sequencer_instructions(0, p, instructions, false);
            }
            if let Some(run) = codec::run_of_tag(&data) {
                self.w.restore_sequencer(0, p, run);
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
                let live = &self.w.dims[0].sequencers[&p];
                let data = self
                    .persisted
                    .sequences
                    .get(&p)
                    .cloned()
                    .unwrap_or_else(|| codec::sequencer_to_nbt(live));
                let run = codec::run_of_tag(&data).unwrap();
                let saved_instructions = codec::of_tag(&data);
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
                let controller = *self.w.dims[0].belt_of.get(&p)?;
                let b = &self.w.dims[0].belts[&controller];
                let index = b.chain.iter().position(|q| *q == p)? as i32;
                let inventory = (p == controller).then(|| {
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
                    length: b.chain.len() as i32,
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
                r.flush_persistence();
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
                for _ in 1..saved.in_use {
                    r.w.tick_cranks(0);
                    r.flush_persistence();
                }
                assert_eq!(r.w.dims[0].kinetics.generated(p, 0), Some(rpm));
                r.w.tick_cranks(0);
                r.w.tick(0, &mut Vec::new());
                r.flush_persistence();
                assert_eq!(r.w.saved_handle(0, p).unwrap().in_use, 0);
                assert_eq!(r.w.dims[0].kinetics.generated(p, 0), Some(0.));
                if valve {
                    assert!(r.w.use_valve(0, p, false, 45));
                }
            }
        }
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
            r.flush_persistence();
        }
        let saved = r.w.saved_depot(0, p).unwrap();
        r.reload();
        assert_eq!(r.w.saved_depot(0, p), Some(saved));
        for expected in [0.48592162, 0.48944122, 0.49208093] {
            r.w.tick_depots(0, |_| false);
            r.flush_persistence();
            let actual = codec::depot_from_nbt(&r.persisted.depots[&p])
                .held
                .unwrap()
                .position;
            assert!((actual - expected).abs() < 0.0000001);
        }
        r.w.depot_put(0, p, None);
        r.depot_stacks.remove(&p);
        r.flush_persistence();
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
        r.flush_persistence();
        r.reload();
        assert_eq!(r.w.saved_depot(0, p).unwrap().held, None);
    }
    #[test]
    fn cold_belt_load_forms_controller_after_the_logistics_phase() {
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
        assert_eq!(phase.next(&mut r.w), None);
        r.w.settle_belts(0);
        assert_eq!(r.w.dims[0].belts[&c].items.items[0].position, 1.4);
        assert!(r.w.dims[0].kinetics.has_part(c, 0));
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
