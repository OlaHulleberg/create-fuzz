//! Weighted ejectors, mirroring glue/logistics/arms.rs: their redstone (`tick_arm_signals`),
//! their tick (`tick_ejectors` with `EjectorWorld`: `EjectorBlockEntity.tick`), what lands on
//! their target (`land`), their depot merging onto what they hold (`depot_insert`), and their
//! stack size on a value board (interaction/values.rs).
use super::*;
use crate::transport::Stack;
use create_engine::{Block, belt, depot, ejector, kinetics::Dir};

impl Run {
    pub(super) fn is_ejector(&self, p: Pos) -> bool {
        self.w.dims[0].ejectors.contains_key(&p)
    }
    /// `depot_insert` for the ejector at `p`: only while charged, merging onto what it holds
    /// up to its stack size (`enableMerging`, `getRemainingSpace`); the count left.
    pub(super) fn ejector_insert(&mut self, p: Pos, s: &Stack, simulate: bool) -> u8 {
        let Some(e) = self.w.dims[0].ejectors.get(&p) else {
            return s.count;
        };
        let held = self.depot_stacks.get(&p).cloned();
        if e.state != ejector::State::Charged || held.as_ref().is_some_and(|h| h.item != s.item) {
            return s.count;
        }
        let limit = if e.max_stack == 0 { 64 } else { e.max_stack }.min(s.limit());
        let present: u32 = held.as_ref().map_or(0, |h| u32::from(h.count))
            + self
                .depot_outputs
                .get(&p)
                .map_or(0, |o| o.iter().map(|x| u32::from(x.count)).sum());
        let room = u32::from(limit).saturating_sub(present).min(255) as u8;
        let n = s.count.min(room);
        if n == 0 {
            return s.count;
        }
        if !simulate {
            let had = held.as_ref().map_or(0, |h| h.count);
            self.depot_stacks.insert(
                p,
                Stack {
                    item: s.item.clone(),
                    count: had + n,
                },
            );
            if held.is_none() {
                self.w.depot_put(0, p, Some(depot::Held::new(180, false)));
            } else {
                self.w.depot_changed(0, p);
            }
        }
        s.count - n
    }
    /// `handlers::DepotItems.insert` into an ejector: only while charged, into the held slot up
    /// to its stack size less its outputs; a new held stack arrives at the edge
    /// (`depot_contents_changed`). The count left.
    pub(super) fn ejector_handler_insert(&mut self, p: Pos, s: &Stack, simulate: bool) -> u8 {
        let Some(e) = self.w.dims[0].ejectors.get(&p) else {
            return s.count;
        };
        if e.state != ejector::State::Charged {
            return s.count;
        }
        let limit = u32::from(if e.max_stack == 0 { 64 } else { e.max_stack }.min(s.limit()));
        let others: u32 = self
            .depot_outputs
            .get(&p)
            .map_or(0, |o| o.iter().map(|x| u32::from(x.count)).sum());
        let room = limit.saturating_sub(others);
        let held = self.depot_stacks.get(&p).cloned();
        if held.as_ref().is_some_and(|h| h.item != s.item) {
            return s.count;
        }
        let now = held.as_ref().map_or(0, |h| u32::from(h.count));
        let n = u32::from(s.count).min(room.saturating_sub(now)) as u8;
        if n == 0 {
            return s.count;
        }
        if !simulate {
            self.depot_stacks.insert(
                p,
                Stack {
                    item: s.item.clone(),
                    count: now as u8 + n,
                },
            );
            if held.is_none() {
                self.w.depot_put(
                    0,
                    p,
                    Some(depot::Held {
                        position: 0.,
                        ..depot::Held::new(180, false)
                    }),
                );
            } else {
                self.w.depot_changed(0, p);
            }
        }
        s.count - n
    }
    /// `land`: a stack thrown at `target` into what takes items there from above: an ejector
    /// merges it, a depot takes it while empty, the others by their `handleInsertion(UP)`, a
    /// belt on its segment (`put_on_from_above`). None when nothing there takes items.
    fn ejector_land(&mut self, target: Pos, s: &Stack, simulate: bool) -> Option<u8> {
        if self.is_ejector(target) {
            return Some(self.ejector_insert(target, s, simulate));
        }
        if self.w.dims[0].depots.contains_key(&target) {
            let empty = !self.depot_stacks.contains_key(&target)
                && self.depot_outputs.get(&target).is_none_or(|o| o.is_empty());
            if !empty {
                return Some(s.count);
            }
            let n = s.count.min(s.limit());
            if !simulate {
                self.depot_stacks.insert(
                    target,
                    Stack {
                        item: s.item.clone(),
                        count: n,
                    },
                );
                self.w
                    .depot_put(0, target, Some(depot::Held::new(180, false)));
            }
            return Some(s.count - n);
        }
        let item = belt::Item::new(&s.item, s.count, 180, false);
        if self.belt_location(target).is_none() && self.direct_input_at(target) {
            return self.handle_insertion(target, Dir::Up, &item, false, simulate);
        }
        if simulate {
            return self.put_on_from_above_room(target, s);
        }
        self.put_on_from_above(target, s)
    }
    /// What `put_on_from_above` would leave, without putting it.
    fn put_on_from_above_room(&self, p: Pos, s: &Stack) -> Option<u8> {
        let (c, index, seg) = self.belt_location(p)?;
        if !seg.transports() {
            return None;
        }
        let rpm = self.w.speed(0, c).unwrap_or(0.);
        let b = self.w.dims[0].belts.get(&c)?;
        let room = b.items.can_insert_at_from_side(
            index,
            Dir::Up,
            belt::movement_facing(rpm, b.segment.facing),
        );
        Some(if room { 0 } else { s.count })
    }
    /// `has_collision` for the ejector's obstacle scan: any block but air.
    fn collides(&self, p: Pos) -> bool {
        self.at(p) != AIR && !self.replaceable(p)
    }
    /// `tick_arm_signals` for ejectors: those whose redstone may have changed read it.
    fn ejector_signals(&mut self) {
        let checks = std::mem::take(&mut self.w.dims[0].arm_signals);
        for p in checks {
            let powered = self.signal(p) > 0;
            let d = &mut self.w.dims[0];
            if let Some(e) = d.ejectors.get_mut(&p)
                && e.powered != powered
            {
                e.powered = powered;
                if !d.ejecting.contains(&p) {
                    d.ejecting.push(p);
                }
            }
        }
    }
    /// `tick_ejectors` (the arms subsystem's level tick, after logistics).
    pub(super) fn tick_ejectors(&mut self) {
        self.ejector_signals();
        let list = self.w.dims[0].ejecting.clone();
        for pos in list {
            let Some(facing) = self.w.table.get(self.at(pos)).and_then(|(_, i)| i.facing) else {
                continue;
            };
            let Some(mut e) = self.w.dims[0].ejectors.remove(&pos) else {
                continue;
            };
            let speed = self.w.speed(0, pos).unwrap_or(0.);
            let flying = e.flying.len();
            let before = e.state;
            let changed = e.tick(pos, facing, speed, &mut EjectorSide { run: self, pos });
            if before == ejector::State::Charged && e.state == ejector::State::Launching {
                // `launch_entities`: the driver's worlds hold no entities on a plate.
            }
            let has_items = self.depot_stacks.contains_key(&pos)
                || self.depot_outputs.get(&pos).is_some_and(|o| !o.is_empty());
            let busy = e.busy(speed, has_items);
            let d = &mut self.w.dims[0];
            d.ejectors.insert(pos, e);
            if changed || flying > 0 {
                d.unsaved.push((pos[0] >> 4, pos[2] >> 4));
            }
            if !busy {
                d.ejecting.retain(|p| *p != pos);
            }
        }
        self.w.dims[0].ejector_changes.clear();
    }
    /// interaction/values.rs `B::WeightedEjector`: row 0's value is `maxStackSize`.
    pub(super) fn ejector_stack_size(&mut self, p: Pos, pick: create_engine::board::Settings) {
        let d = &mut self.w.dims[0];
        let Some(e) = d.ejectors.get_mut(&p) else {
            return;
        };
        e.max_stack = pick.value.clamp(0, 64) as u8;
        self.w.mark_unsaved(0, p);
        let d = &mut self.w.dims[0];
        d.ejecting.retain(|q| *q != p);
        d.ejecting.push(p);
    }
    pub(super) fn ejector_observation(&self, p: Pos) -> Option<crate::logistics::Ejector> {
        if self.w.block_at(0, p) != Some(Block::WeightedEjector) {
            return None;
        }
        let e = self.w.dims[0].ejectors.get(&p)?;
        Some(crate::logistics::Ejector {
            state: match e.state {
                ejector::State::Charged => "charged",
                ejector::State::Launching => "launching",
                ejector::State::Retracting => "retracting",
            }
            .into(),
            powered: e.powered,
            lid: e.lid.value,
            h: e.launcher.h,
            v: e.launcher.v,
            max_stack: i32::from(e.max_stack),
            flying: e
                .flying
                .iter()
                .map(|f| crate::logistics::Flying {
                    time: f.time,
                    stack: Some(Stack {
                        item: f.item.clone(),
                        count: f.count,
                    }),
                })
                .collect(),
            early_time: e.early.map(|x| x.time),
        })
    }
}

/// `EjectorWorld`: what an ejector's tick reaches.
struct EjectorSide<'a> {
    run: &'a mut Run,
    pos: Pos,
}

impl ejector::World for EjectorSide<'_> {
    fn present(&mut self) -> ejector::Present {
        let held = self.run.depot_stacks.get(&self.pos);
        let outputs: u32 = self
            .run
            .depot_outputs
            .get(&self.pos)
            .map_or(0, |o| o.iter().map(|x| u32::from(x.count)).sum());
        let position = self.run.w.dims[0]
            .depots
            .get(&self.pos)
            .copied()
            .flatten()
            .map_or(0.5, |h| h.position);
        ejector::Present {
            count: held.map_or(0, |h| u32::from(h.count)) + outputs,
            held_position: held.is_some().then_some(position),
        }
    }
    fn target_refuses(&mut self, target: [i32; 3]) -> bool {
        let Some(held) = self.run.depot_stacks.get(&self.pos).cloned() else {
            return false;
        };
        self.run.ejector_land(target, &held, true) == Some(held.count)
    }
    fn funnel_on_top_takes(&mut self) -> Option<bool> {
        let above = add(self.pos, [0, 1, 0]);
        let facing = self.run.w.table.get(self.run.at(self.pos))?.1.facing?;
        let (_, info) = self.run.funnel_at(above)?;
        if info.facing != Some(facing.opposite()) {
            return None;
        }
        self.run.notes.push(format!(
            "adapter gap: belt funnel on the ejector at {:?}",
            self.pos
        ));
        None
    }
    fn take_all(&mut self) -> Vec<(String, u8)> {
        let mut taken: Vec<(String, u8)> = self
            .run
            .depot_stacks
            .remove(&self.pos)
            .into_iter()
            .map(|s| (s.item, s.count))
            .collect();
        taken.extend(
            self.run
                .depot_outputs
                .remove(&self.pos)
                .unwrap_or_default()
                .into_iter()
                .map(|s| (s.item, s.count)),
        );
        if !taken.is_empty() {
            self.run.w.depot_put(0, self.pos, None);
        }
        taken
    }
    fn insert(&mut self, target: [i32; 3], item: &str, count: u8) -> Option<u8> {
        let s = Stack {
            item: item.into(),
            count,
        };
        self.run.ejector_land(target, &s, false)
    }
    fn drop_item(&mut self, at: [f64; 3], _motion: [f64; 3], item: &str, count: u8) {
        self.run.notes.push(format!(
            "adapter gap: ejector at {:?} drops {count} {item} at {at:?} (native item flight)",
            self.pos
        ));
    }
    fn clip(&mut self, from: [f64; 3], to: [f64; 3]) -> Option<([f64; 3], [i32; 3], Dir)> {
        let own = self.pos;
        let run = &*self.run;
        ejector::clip_blocks(from, to, |p| p != own && run.collides(p))
    }
    fn extracting_funnel(&mut self, pos: [i32; 3]) -> bool {
        self.run.funnel_at(pos).is_some_and(|(b, i)| {
            matches!(b, Block::AndesiteFunnel | Block::BrassFunnel) && i.extracting
        })
    }
}
