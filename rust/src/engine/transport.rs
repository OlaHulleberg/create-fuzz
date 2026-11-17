//! Logistics phase and player item boundaries, mirrored from glue/logistics/{belt,depot}.rs.
use super::*;
use crate::transport::{Contents, Input, Item as ObservedItem, Outcome, Stack};
use create_engine::{
    Block, belt, depot,
    kinetics::{Axis, Dir},
};
impl Run {
    fn belt_location(&self, p: Pos) -> Option<(Pos, i32, belt::Segment)> {
        let seg = self.w.table.get(self.at(p))?.1.belt?;
        let c = *self.w.dims[0].belt_of.get(&p)?;
        let index = self.w.dims[0]
            .belts
            .get(&c)?
            .chain
            .iter()
            .position(|q| *q == p)? as i32;
        Some((c, index, seg))
    }
    pub(super) fn transport_empty_use(&mut self, p: Pos, face: Dir, sneaking: bool) -> bool {
        if self.belt_location(p).is_some() || self.w.block_at(0, p) == Some(Block::Depot) {
            self.item_input(
                p,
                &Input::Use {
                    stack: None,
                    face: match face {
                        Dir::Up => crate::actions::Face::Up,
                        Dir::Down => crate::actions::Face::Down,
                        Dir::East => crate::actions::Face::East,
                        Dir::West => crate::actions::Face::West,
                        Dir::North => crate::actions::Face::North,
                        Dir::South => crate::actions::Face::South,
                    },
                    yaw: 0.,
                    sneaking,
                },
            );
            true
        } else {
            false
        }
    }
    pub(super) fn item_input(&mut self, p: Pos, input: &Input) {
        let mut taken = Vec::new();
        let offered = match input {
            Input::DropOn { stack } => Some(stack.clone()),
            Input::Use { stack, .. } => stack.clone(),
        };
        let mut remainder = offered.clone();
        match input {
            Input::DropOn { stack } => {
                if let Some((c, index, seg)) = self.belt_location(p) {
                    if seg.transports() {
                        let rpm = self.w.speed(0, c).unwrap_or(0.);
                        let b = self.w.dims[0].belts.get_mut(&c).unwrap();
                        if b.items.can_insert_at_from_side(
                            index,
                            Dir::Up,
                            belt::movement_facing(rpm, b.segment.facing),
                        ) {
                            let mut i = belt::Item::new(&stack.item, stack.count, 180, false);
                            i.inserted_at = index;
                            i.inserted_from = Some(Dir::Up);
                            i.position =
                                index as f32 + 0.5 + if b.items.positive { -1. } else { 1. } / 16.;
                            b.items.add(i);
                            self.w.belt_inventory_changed(0, c);
                            remainder = None;
                        }
                    }
                } else if self.w.dims[0].depots.contains_key(&p)
                    && !self.depot_stacks.contains_key(&p)
                {
                    self.depot_stacks.insert(p, stack.clone());
                    self.w.depot_put(0, p, Some(depot::Held::new(180, false)));
                    remainder = None;
                }
            }
            Input::Use {
                stack,
                face,
                yaw,
                sneaking,
            } => {
                if let Some((c, index, _)) = self.belt_location(p) {
                    if !sneaking && stack.is_none() {
                        taken = self.w.dims[0]
                            .belts
                            .get_mut(&c)
                            .unwrap()
                            .items
                            .take_within(index as f32 + 0.5, 0.55)
                            .into_iter()
                            .map(|i| Stack {
                                item: i.item,
                                count: i.count,
                            })
                            .collect();
                        if !taken.is_empty() {
                            self.w.belt_inventory_changed(0, c);
                        }
                    }
                } else if self.w.dims[0].depots.contains_key(&p) {
                    if face.dir() != Dir::Up {
                        self.item_outcomes.push(Outcome {
                            pos: p,
                            offered: offered.clone(),
                            remainder: offered,
                            taken: vec![],
                        });
                        return;
                    }
                    let mut inventory = DepotUse {
                        run: self,
                        pos: p,
                        hand: stack.clone(),
                        taken: Vec::new(),
                        facing: depot::Side::of_yaw(*yaw),
                    };
                    depot::use_top(
                        &mut inventory,
                        stack.is_none(),
                        stack
                            .as_ref()
                            .is_some_and(|s| s.item == "create:mechanical_arm"),
                    );
                    remainder = inventory.hand;
                    taken = inventory.taken;
                }
                // The remaining blocks in the bounded contract have no stack-transfer
                // handler for these plain items. A refused use leaves the offered stack.
            }
        }
        let mut merged = BTreeMap::<String, u8>::new();
        for s in taken {
            *merged.entry(s.item).or_default() += s.count;
        }
        self.item_outcomes.push(Outcome {
            pos: p,
            offered,
            remainder,
            taken: merged
                .into_iter()
                .map(|(item, count)| Stack { item, count })
                .collect(),
        });
    }
    pub(super) fn tick_transport(&mut self) {
        let changed = self.w.take_depot_changes(0);
        for p in changed {
            if self.w.dims[0].depots.get(&p) == Some(&None) && self.depot_stacks.contains_key(&p) {
                if let Some(h) = self.w.dims[0].depots.get_mut(&p) {
                    *h = Some(depot::Held::new(180, false));
                }
                self.w.wake_depot(0, p);
            }
        }
        self.belt_breaks();
        // The glue's shared level registration walk (BeltBlockEntity.tick and
        // DepotBehaviour.tick): apply transfers before the next entity's turn.
        let mut phase = self.w.transport_ticks(0);
        while let Some(turn) = phase.next(&mut self.w) {
            let c = match turn {
                create_engine::world::TransportTick::Depot(p) => {
                    self.w.tick_depot(0, p, false);
                    continue;
                }
                create_engine::world::TransportTick::Belt(c) => c,
            };
            let b = self.w.dims[0].belts.get_mut(&c).unwrap();
            let (seg, length, mut items) = (
                b.segment,
                b.chain.len() as i32,
                std::mem::take(&mut b.items),
            );
            let rpm = self.w.speed(0, c).unwrap_or(0.);
            let speed = belt::direction_aware_speed(rpm, seg.facing);
            let past =
                belt::position_for_offset(c, &seg, length, if speed > 0. { length } else { -1 });
            let ticked = items.tick(
                length,
                speed,
                seg.slope == belt::Slope::Horizontal,
                &mut Side {
                    run: self,
                    past,
                    movement: belt::movement_facing(rpm, seg.facing),
                },
            );
            if let Some(b) = self.w.dims[0].belts.get_mut(&c) {
                let added = std::mem::replace(&mut b.items, items);
                for i in added.items {
                    b.items.add(i);
                }
            }
            if ticked.saved_changed {
                self.w.belt_inventory_changed(0, c);
            }
            if ticked.changed {
                self.w.dims[0].belt_changes.push(c);
            }
            // Native dropped-entity flight after an open end is outside this adapter.
            let _ = ticked;
        }
        let mut changed = std::mem::take(&mut self.w.dims[0].belt_changes);
        changed.sort_unstable();
        changed.dedup();
        self.save_belt_records(&changed);
        for p in changed {
            self.w.mark_unsaved(0, p);
        }
    }
    fn insert_transport(&mut self, p: Pos, side: Dir, item: &belt::Item) -> Option<u8> {
        if self.w.dims[0].depots.contains_key(&p) {
            if self.depot_stacks.contains_key(&p) {
                return Some(item.count);
            }
            self.depot_stacks.insert(
                p,
                Stack {
                    item: item.item.clone(),
                    count: item.count,
                },
            );
            self.w.depot_put(
                0,
                p,
                Some(depot::Held {
                    position: if side.axis() == Axis::Y { 0.5 } else { 0. },
                    from: depot::Side::from_dir(side),
                    angle: item.angle,
                    side_offset: item.side_offset,
                    locked: false,
                    fan: item.fan,
                }),
            );
            return Some(0);
        }
        let (c, index, seg) = self.belt_location(p)?;
        if !seg.transports() {
            return Some(item.count);
        }
        let rpm = self.w.speed(0, c).unwrap_or(0.);
        let speed = belt::direction_aware_speed(rpm, seg.facing);
        let movement = belt::movement_facing(rpm, seg.facing);
        let b = self.w.dims[0].belts.get_mut(&c)?;
        if rpm == 0.
            || movement == side.opposite()
            || !b.items.can_insert_at_from_side(index, side, movement)
        {
            return Some(item.count);
        }
        let mut moved = item.clone();
        moved.locked = false;
        moved.position = index as f32 + 0.5 - speed.signum() / 16.;
        if side.axis() != Axis::Y {
            if movement != side {
                moved.side_offset = if side.positive() { 0.675 } else { -0.675 };
                if side.axis() == Axis::X {
                    moved.side_offset = -moved.side_offset;
                }
            } else {
                moved.position = if speed > 0. {
                    index as f32 - 0.26
                } else {
                    index as f32 + 1.26
                };
            }
        }
        moved.inserted_at = index;
        moved.inserted_from = Some(side);
        b.items.add(moved);
        self.w.belt_inventory_changed(0, c);
        Some(0)
    }
    pub(super) fn transport_observation(&self, p: Pos) -> Option<Contents> {
        if self.w.block_at(0, p) == Some(Block::Depot) {
            let items = self
                .depot_stacks
                .get(&p)
                .map(|s| {
                    let h = self.w.dims[0]
                        .depots
                        .get(&p)
                        .copied()
                        .flatten()
                        .expect("held depot motion");
                    ObservedItem {
                        stack: s.clone(),
                        position: h.position,
                        locked: h.locked,
                        pending: false,
                        stalled: None,
                    }
                })
                .into_iter()
                .collect();
            return Some(Contents {
                kind: "depot".into(),
                items,
            });
        }
        let b = self.w.dims[0].belts.get(&p)?;
        let mut live = b.items.items.clone();
        let mut items = Vec::new();
        for i in b.items.to_save().items {
            let pending = if let Some(index) = live.iter().position(|x| *x == i) {
                live.remove(index);
                false
            } else {
                true
            };
            items.push(ObservedItem {
                stack: Stack {
                    item: i.item,
                    count: i.count,
                },
                position: i.position,
                locked: i.locked,
                pending,
                stalled: if pending {
                    None
                } else {
                    Some(self.w.speed(0, p).unwrap_or(0.) == 0. || i.stalled)
                },
            });
        }
        items.sort_by(|a, b| {
            a.position
                .total_cmp(&b.position)
                .then(a.stack.item.cmp(&b.stack.item))
                .then(a.stack.count.cmp(&b.stack.count))
        });
        Some(Contents {
            kind: "belt".into(),
            items,
        })
    }
}
/// `depot::use_depot`'s inventory boundary. Transport inputs are explicitly
/// component-free (`Stack` rejects component fields), so equal item ids here
/// also have equal components. Stack limits come from the audited contract.
struct DepotUse<'a> {
    run: &'a mut Run,
    pos: Pos,
    hand: Option<Stack>,
    taken: Vec<Stack>,
    facing: depot::Side,
}
impl depot::PlayerUse for DepotUse<'_> {
    type Stack = Stack;

    fn take_contents(&mut self) -> Option<Vec<Stack>> {
        let contents = self
            .run
            .depot_stacks
            .remove(&self.pos)
            .into_iter()
            .collect();
        self.run.w.depot_put(0, self.pos, None);
        Some(contents)
    }

    fn give_back(&mut self, mut stack: Stack) {
        if let Some(hand) = &mut self.hand
            && hand.item == stack.item
        {
            let count = stack.count.min(hand.limit() - hand.count);
            hand.count += count;
            stack.count -= count;
        }
        // An initially empty hand only collects. Its refunds are reported as
        // taken rather than as a remainder, as ItemInputs measures on Java.
        if stack.count > 0 {
            self.taken.push(stack);
        }
    }

    fn place_hand(&mut self) {
        if let Some(stack) = self.hand.take() {
            self.run.depot_stacks.insert(self.pos, stack);
            self.run.w.depot_put(
                0,
                self.pos,
                Some(depot::Held::put_by_player(180, false, self.facing)),
            );
        }
    }
}

struct Side<'a> {
    run: &'a mut Run,
    past: Pos,
    movement: Dir,
}
impl belt::Surroundings for Side<'_> {
    fn ending(&mut self) -> belt::Ending {
        if self.run.w.dims[0].depots.contains_key(&self.past)
            || self.run.belt_location(self.past).is_some()
        {
            belt::Ending::Insert
        } else if matches!(self.run.at(self.past), OTHER | REDSTONE_BLOCK) {
            belt::Ending::Blocked
        } else {
            belt::Ending::Eject
        }
    }
    fn insert(&mut self, i: &belt::Item) -> Option<u8> {
        self.run.insert_transport(self.past, self.movement, i)
    }
    fn process(&mut self, _: i32, _: &mut belt::Item, _: bool) -> Option<belt::Process> {
        None
    }
}
