//! Host unload/load order: column attachments, block scan, per-entity kinetic/run data.
use super::*;
use crate::persistence::codec::{self, Compound, Tag};

type ReloadData = (
    Compound,
    Vec<(Pos, u8, Compound)>,
    Vec<(Pos, Compound)>,
    Vec<(Pos, crate::transport::Stack)>,
    Vec<(Pos, crate::handles::Handle)>,
);

impl Run {
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
        let mut motors = Vec::new();
        let mut belts = Vec::new();
        for &(cx, cz) in &columns {
            motors.extend(self.w.column_settings(0, cx, cz));
            belts.extend(self.w.column_belts(0, cx, cz));
        }
        let mut entities = Vec::new();
        let mut sequences = Vec::new();
        for &(p, _) in &blocks {
            for sub in 0..2 {
                if let Some(saved) = self.w.dims[0].kinetics.saved_state(p, sub) {
                    entities.push((p, sub, codec::saved_to_nbt(saved)));
                }
            }
            if self.w.block_at(0, p) == Some(create_engine::Block::SequencedGearshift) {
                let seq = &self.w.dims[0].sequencers[&p];
                sequences.push((p, codec::sequencer_to_nbt(seq)));
            }
        }
        // Exercise a real value-tree encoding boundary; no live kinetic state survives it.
        let data = Compound::from([
            ("belts".into(), codec::belts_to_nbt(&belts)),
            (
                "motors".into(),
                Tag::IntArray(
                    motors
                        .iter()
                        .flat_map(|(p, v)| [p[0], p[1], p[2], *v])
                        .collect(),
                ),
            ),
        ]);
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
        let motors = match &data["motors"] {
            Tag::IntArray(v) => v
                .chunks_exact(4)
                .map(|v| ([v[0], v[1], v[2]], v[3]))
                .collect::<Vec<_>>(),
            _ => unreachable!(),
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
                assert!(self.w.dims[0].kinetics.restore_state(p, sub, saved));
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
