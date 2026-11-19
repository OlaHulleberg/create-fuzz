//! Vertical chutes at belts, mirroring glue/logistics/chute.rs without fans: a belt's end
//! hands a stack into an empty chute (`ChuteBlockEntity`'s `DirectBeltInputBehaviour`); it
//! falls at gravity's `chute::MOTION` (`tick`) and leaves at the bottom into the belt segment
//! below (`ItemHandlerBeltSegment`), a depot, or the chute below, or out of the bottom
//! (`handleDownwardOutput`), whose drop is native item flight.
use super::*;
use crate::transport::Stack;
use create_engine::{Block, belt, chute, kinetics::Dir};

impl Run {
    /// `chute_at`: a plain chute at `p` (smart chutes, diagonal chutes and fans are outside this
    /// adapter and noted).
    pub(super) fn chute_at(&self, p: Pos) -> Option<Dir> {
        let (block, info) = self.w.table.get(self.at(p))?;
        match block {
            Block::Chute => Some(info.facing.unwrap_or(Dir::Down)),
            _ => None,
        }
    }
    /// `canDirectlyInsert`: no chute above leads down into it (with no fans its stack falls).
    pub(super) fn chute_takes_from_side(&self, p: Pos) -> bool {
        self.chute_at([p[0], p[1] + 1, p[2]]) != Some(Dir::Down)
    }
    /// `chute_direct_input` and `chute_take`: the whole stack, at the top, while it is empty.
    pub(super) fn chute_direct_input(&mut self, p: Pos, item: &belt::Item) -> u8 {
        if !self.chute_takes_from_side(p) || self.chute_stacks.contains_key(&p) {
            return item.count;
        }
        self.chute_stacks.insert(
            p,
            Stack {
                item: item.item.clone(),
                count: item.count,
            },
        );
        self.w.dims[0].chutes.insert(p, 1.);
        0
    }
    /// `tick_chutes` for chutes falling at `chute::MOTION`.
    pub(super) fn tick_chutes(&mut self) {
        let mut moving: Vec<(Pos, f32)> = self.w.dims[0]
            .chutes
            .iter()
            .map(|(p, v)| (*p, *v))
            .collect();
        moving.sort_by_key(|m| m.0);
        for (pos, position) in moving {
            let Some(stack) = self.chute_stacks.get(&pos).cloned() else {
                self.w.dims[0].chutes.remove(&pos);
                continue;
            };
            if self.chute_at(pos) != Some(Dir::Down) {
                self.notes
                    .push(format!("adapter gap: diagonal chute at {pos:?}"));
                continue;
            }
            let below = [pos[0], pos[1] - 1, pos[2]];
            let target_chute = self.chute_at(below).is_some();
            let belt_below = !target_chute
                && self
                    .belt_location(below)
                    .is_some_and(|(_, _, s)| s.transports());
            let depot_below = !target_chute
                && self
                    .w
                    .table
                    .get(self.at(below))
                    .is_some_and(|(b, _)| b == Block::Depot);
            let container = belt_below || depot_below;
            let can = if container {
                true
            } else if target_chute {
                !self.chute_stacks.contains_key(&below)
            } else {
                // `falls_through`: unless the block below can hold a rigid block up.
                !self.sturdy(below, Dir::Up)
            };
            let next = match chute::step(position, chute::MOTION, can) {
                chute::Fall::At(p) => Some(p),
                chute::Fall::Out if container => {
                    let left = if belt_below {
                        self.put_on_from_above(below, &stack)
                            .map_or(Some(stack.clone()), |l| {
                                (l > 0).then(|| Stack {
                                    item: stack.item.clone(),
                                    count: l,
                                })
                            })
                    } else {
                        self.depot_insert(below, &stack, false)
                    };
                    match left {
                        Some(l) if l.count == stack.count => Some(0.5),
                        Some(l) => {
                            self.chute_stacks.insert(pos, l);
                            Some(0.5)
                        }
                        None => {
                            self.chute_stacks.remove(&pos);
                            None
                        }
                    }
                }
                chute::Fall::Out if target_chute => {
                    self.chute_stacks.remove(&pos);
                    self.chute_stacks.insert(below, stack);
                    self.w.dims[0].chutes.insert(below, 1.);
                    None
                }
                chute::Fall::Out => {
                    self.chute_stacks.remove(&pos);
                    None
                }
            };
            match next {
                Some(p) => {
                    self.w.dims[0].chutes.insert(pos, p);
                }
                None => {
                    self.w.dims[0].chutes.remove(&pos);
                }
            }
        }
    }
    pub(super) fn chute_observation(&self, p: Pos) -> Option<crate::logistics::Chute> {
        self.chute_at(p)?;
        let item = self.chute_stacks.get(&p).cloned();
        // A chute's position is only observed while it holds a stack.
        let position = if item.is_some() {
            self.w.dims[0].chutes.get(&p).copied().unwrap_or(0.5)
        } else {
            0.
        };
        Some(crate::logistics::Chute { item, position })
    }
}
