//! Funnels that move stacks out of the inventory behind them, mirroring glue/logistics/funnel.rs
//! `tick_funnels`: a belt funnel pushing onto its belt or onto the depot under it
//! (`activateExtractingBeltFunnel`), a belt funnel over a depot taking its stacks
//! (`handleBeltFunnelOutput`), and redstone pausing them. The inventory behind is a depot
//! (`DepotItems`); others, extracting funnels (whose stacks fly off as item entities), brass
//! funnels' amounts and machines below are adapter gaps with a note.
use super::*;
use crate::transport::Stack;
use create_engine::{
    Block, belt, depot,
    funnel::{BeltShape, EXTRACTION_COOLDOWN, Mode, belt_mode},
    kinetics::Dir,
};

/// What a funnel's turn did (`extract_in_front`'s answers): a stack left, something blocks it,
/// or the inventory gave nothing.
enum Turn {
    Moved,
    Blocked,
    Empty,
}

impl Run {
    fn note_once(&mut self, note: String) {
        if !self.notes.contains(&note) {
            self.notes.push(note);
        }
    }
    /// `belt_movement_below`: the way the belt that carries items below `p` moves.
    fn belt_movement_below(&self, p: Pos) -> Option<Dir> {
        let (c, _, seg) = self.belt_location([p[0], p[1] - 1, p[2]])?;
        if !seg.transports() {
            return None;
        }
        let rpm = self.w.speed(0, c).unwrap_or(0.);
        Some(belt::movement_facing(rpm, seg.facing))
    }
    /// The funnel's mode (`determineCurrentMode`).
    fn funnel_mode(&self, p: Pos) -> Option<(Block, create_engine::parts::StateInfo, Mode)> {
        let (block, info) = self.funnel_at(p)?;
        let facing = info.facing?;
        let mode = match block {
            Block::AndesiteBeltFunnel | Block::BrassBeltFunnel => belt_mode(
                info.powered,
                info.belt_funnel.unwrap_or_default(),
                self.belt_movement_below(p),
                facing,
            ),
            _ if info.powered => Mode::Paused,
            _ if info.extracting => Mode::Extract,
            _ => Mode::Collect,
        };
        Some((block, info, mode))
    }
    /// `take_filtered` from a depot behind (`DepotItems.extract`, slot by slot): up to `amount`.
    fn take_from_depot(&mut self, behind: Pos, amount: u8) -> Option<Stack> {
        if let Some(held) = self.depot_stacks.get(&behind).cloned() {
            let n = amount.min(held.count);
            if n == held.count {
                self.depot_stacks.remove(&behind);
                self.w.depot_put(0, behind, None);
            } else if let Some(h) = self.depot_stacks.get_mut(&behind) {
                h.count -= n;
                self.w.depot_changed(0, behind);
            }
            return Some(Stack {
                item: held.item,
                count: n,
            });
        }
        let outputs = self.depot_outputs.get_mut(&behind)?;
        let first = outputs.first_mut()?;
        let n = amount.min(first.count);
        let taken = Stack {
            item: first.item.clone(),
            count: n,
        };
        first.count -= n;
        outputs.retain(|s| s.count > 0);
        Some(taken)
    }
    /// The inventory behind the funnel at `p` facing `facing`, when the driver models it.
    fn funnel_inventory(&mut self, p: Pos, facing: Dir) -> Option<Pos> {
        let behind = create_engine::funnel::behind(p, facing);
        match self.w.table.get(self.at(behind)).map(|(b, _)| b) {
            Some(Block::Depot) => Some(behind),
            Some(b) => {
                self.note_once(format!(
                    "adapter gap: funnel inventory create:{} at {behind:?}",
                    b.name()
                ));
                None
            }
            None => None,
        }
    }
    /// `tick_funnels`: the funnels due now act as their mode says; each is due again the next
    /// tick, after the cooldown once a stack left (`startCooldown`), or never while dormant.
    pub(super) fn tick_funnels(&mut self) {
        let now = u64::from(self.now);
        let d = &mut self.w.dims[0];
        if d.funnel_timers.is_empty() {
            return;
        }
        let funnels = &d.funnels;
        let due = d
            .funnel_timers
            .due(now, |p, t| funnels.get(&p).is_some_and(|due| *due == t));
        for pos in due {
            let Some((block, info, mode)) = self.funnel_mode(pos) else {
                self.funnel_schedule(pos, u64::MAX);
                continue;
            };
            let facing = info.facing.unwrap_or(Dir::Up);
            let belt_funnel = matches!(block, Block::AndesiteBeltFunnel | Block::BrassBeltFunnel);
            let brass = matches!(block, Block::BrassFunnel | Block::BrassBeltFunnel);
            let below = [pos[0], pos[1] - 1, pos[2]];
            let depot_below = matches!(
                self.w.table.get(self.at(below)).map(|(b, _)| b),
                Some(Block::Depot | Block::WeightedEjector)
            );
            let next = match mode {
                Mode::Extract => {
                    self.note_once(format!(
                        "adapter gap: extracting funnel at {pos:?} (native item flight)"
                    ));
                    None
                }
                Mode::PushingToBelt | Mode::TakingFromBelt if brass => {
                    self.note_once(format!(
                        "adapter gap: brass funnel filter and amount at {pos:?}"
                    ));
                    None
                }
                Mode::PushingToBelt if depot_below => {
                    // `funnel_extraction`: a stack unless it pushes or a belt is below.
                    let one = info.belt_funnel == Some(BeltShape::Pushing);
                    self.push_onto_depot(pos, facing, if one { 1 } else { 64 })
                }
                Mode::PushingToBelt => self.push_onto_belt(pos, facing),
                Mode::TakingFromBelt if depot_below => {
                    self.pull_from_depot(pos, facing);
                    Some(Turn::Blocked)
                }
                _ => None,
            };
            let at = match next {
                Some(Turn::Moved) => now + u64::from(EXTRACTION_COOLDOWN) + 1,
                Some(_) => now + 1,
                None if belt_funnel => now + 1,
                None => u64::MAX,
            };
            self.funnel_schedule(pos, at);
        }
    }
    fn funnel_schedule(&mut self, pos: Pos, at: u64) {
        let d = &mut self.w.dims[0];
        if let Some(due) = d.funnels.get_mut(&pos) {
            *due = at;
            if at != u64::MAX {
                d.funnel_timers.at(at, pos);
            }
        }
    }
    /// `push_onto_belt` (`activateExtractingBeltFunnel`): one item of the inventory behind onto
    /// the belt below from the funnel's side while the belt takes items there.
    fn push_onto_belt(&mut self, pos: Pos, facing: Dir) -> Option<Turn> {
        let below = [pos[0], pos[1] - 1, pos[2]];
        let (c, index, _) = self.belt_location(below)?;
        let rpm = self.w.speed(0, c).unwrap_or(0.);
        let b = self.w.dims[0].belts.get(&c)?;
        let movement = belt::movement_facing(rpm, b.segment.facing);
        let free = rpm != 0.
            && movement != facing.opposite()
            && b.items.can_insert_at_from_side(index, facing, movement);
        if !free {
            return Some(Turn::Blocked);
        }
        let behind = self.funnel_inventory(pos, facing)?;
        let Some(stack) = self.take_from_depot(behind, create_engine::funnel::amount(false) as u8)
        else {
            return Some(Turn::Empty);
        };
        let item = belt::Item::new(&stack.item, stack.count, 180, false);
        let left = self
            .handle_insertion(below, facing, &item, false, false)
            .unwrap_or(stack.count);
        if left > 0 {
            self.note_once(format!(
                "adapter gap: funnel at {pos:?} gives {left} back to its inventory"
            ));
        }
        Some(Turn::Moved)
    }
    /// `push_onto_depot`: the inventory behind onto the depot below while it holds nothing,
    /// sliding in from the funnel's side.
    fn push_onto_depot(&mut self, pos: Pos, facing: Dir, amount: u8) -> Option<Turn> {
        let below = [pos[0], pos[1] - 1, pos[2]];
        if self.is_ejector(below) {
            self.note_once(format!("adapter gap: funnel onto the ejector at {below:?}"));
            return None;
        }
        if self.depot_stacks.contains_key(&below)
            || self
                .depot_outputs
                .get(&below)
                .is_some_and(|o| !o.is_empty())
        {
            return Some(Turn::Blocked);
        }
        let behind = self.funnel_inventory(pos, facing)?;
        let Some(stack) = self.take_from_depot(behind, amount) else {
            return Some(Turn::Empty);
        };
        self.depot_stacks.insert(below, stack);
        self.w.depot_put(
            0,
            below,
            Some(depot::Held {
                position: 0.,
                from: depot::Side::from_dir(facing),
                ..depot::Held::new(180, false)
            }),
        );
        Some(Turn::Moved)
    }
    /// `pull_from_depot` (`handleBeltFunnelOutput`): the depot's outputs, then its held stack
    /// once at rest, into the inventory behind the funnel as far as they fit.
    fn pull_from_depot(&mut self, pos: Pos, facing: Dir) -> Option<Turn> {
        let below = [pos[0], pos[1] - 1, pos[2]];
        if self.is_ejector(below) {
            self.note_once(format!("adapter gap: funnel over the ejector at {below:?}"));
            return None;
        }
        let resting = self.w.dims[0]
            .depots
            .get(&below)
            .is_some_and(|h| h.is_none_or(|h| !h.sliding()));
        if self
            .depot_outputs
            .get(&below)
            .is_some_and(|o| !o.is_empty())
        {
            self.note_once(format!(
                "adapter gap: depot outputs under the funnel at {pos:?}"
            ));
            return None;
        }
        if !resting {
            return Some(Turn::Empty);
        }
        let held = self.depot_stacks.get(&below).cloned()?;
        let behind = self.funnel_inventory(pos, facing)?;
        let left = self.depot_insert(behind, &held, false);
        if left.as_ref().is_some_and(|l| l.count == held.count) {
            return Some(Turn::Empty);
        }
        match left {
            None => {
                self.depot_stacks.remove(&below);
                self.w.depot_put(0, below, None);
            }
            Some(rest) => {
                self.depot_stacks.insert(below, rest);
                self.w.depot_changed(0, below);
            }
        }
        Some(Turn::Moved)
    }
    /// `FunnelBlockEntity.extractionCooldown`: the ticks before the funnel's next turn, as its
    /// due tick says (0 while it acts each tick or is dormant).
    pub(super) fn funnel_cooldown(&self, p: Pos) -> i32 {
        let Some(&due) = self.w.dims[0].funnels.get(&p) else {
            return 0;
        };
        let now = u64::from(self.now);
        if due == u64::MAX || due <= now + 1 {
            return 0;
        }
        (due - now - 1) as i32
    }
}
