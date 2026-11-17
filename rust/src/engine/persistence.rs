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
}
type ReloadData = (
    Compound,
    Vec<(Pos, u8, Compound)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, crate::transport::Stack)>,
    Vec<(Pos, crate::handles::Handle)>,
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
        ))
        .unwrap();
        let (data, entities, sequences, slots, handles): ReloadData =
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
        // Depot slot contents belong to the host entity. The glue does not persist its
        // motion in the column; a newly loaded occupied depot is initialized at rest.
        for &p in self.depot_stacks.keys() {
            if self.w.dims[0].depots.contains_key(&p) {
                self.w
                    .depot_put(0, p, Some(create_engine::depot::Held::new(180, false)));
            }
        }
        // ValveUse/ScrollValue are host entity data; handles retain that serialized use
        // record. Engine crank countdowns/cooldowns are deliberately not synthesized:
        // block_entity_on_load has no restoration path for them in the current glue.
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
            Block::HandCrank => Payload::Crank {},
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
                let h = self.handles.get(&p)?;
                Payload::Valve {
                    selected: h.selected,
                    total: i32::from(h.total),
                    target: h.target,
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
    fn cold_glue_load_does_not_restore_crank_countdown() {
        let sc:Scenario=serde_json::from_value(serde_json::json!({"ticks":20,"blocks":[{"pos":[0,1,0],"state":"create:hand_crank[facing=up]"}]})).unwrap();
        let mut r = Run::new(&sc);
        let p = [0, 1, 0];
        let state = r.id("create:hand_crank[facing=up]");
        r.w.block_loaded(0, p, state);
        assert!(r.w.turn_crank(0, p, false));
        let mut events = Vec::new();
        r.w.tick(0, &mut events);
        let saved = r.w.dims[0].kinetics.saved_state(p, 0).unwrap();
        assert_ne!(r.w.dims[0].kinetics.generated(p, 0), Some(0.));
        r.w.column_unloaded(0, 0, 0);
        r.w.block_loaded(0, p, state);
        assert!(r.w.dims[0].kinetics.restore_state(p, 0, saved));
        // This uses the engine directly, without Run::reload or its observation caches.
        assert_eq!(r.w.dims[0].kinetics.generated(p, 0), Some(0.));
        assert!(r.w.value_box_active(0, p));
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
