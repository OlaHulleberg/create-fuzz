//! Belt item interactions and committed break effects, mirroring logistics/belt and items/belt_connector.
use super::*;
use create_engine::{
    Block, belt, belt_connector as bc,
    kinetics::{Axis, Dir},
};

impl bc::Ground for Run {
    fn loaded(&self, _: Pos) -> bool {
        true
    }
    fn shaft(&self, p: Pos) -> Option<Axis> {
        self.w
            .table
            .get(self.at(p))
            .and_then(|(b, i)| (b == Block::Shaft).then_some(i.axis).flatten())
    }
    fn speed(&self, p: Pos) -> f32 {
        self.w.speed(0, p).unwrap_or(0.)
    }
    fn replaceable(&self, p: Pos) -> bool {
        self.at(p) == AIR
    }
}
impl Run {
    pub(super) fn belt_use(
        &mut self,
        p: Pos,
        item: &str,
        face: Dir,
        hit: [f64; 3],
        sneaking: bool,
    ) -> bool {
        let seg = self.w.table.get(self.at(p)).and_then(|(_, i)| i.belt);
        if let Some(seg) = seg {
            if sneaking {
                return false;
            }
            if (item == bc::ITEM || item == "create:wrench")
                && self.slice_belt(p, item, face, hit.map(|v| v - 0.5), seg)
            {
                return true;
            }
            if item == "create:shaft" && seg.part == belt::Part::Middle {
                let s = with_prop(&self.name(self.at(p)), "part", "pulley");
                self.switch(p, self.id(&s));
                return true;
            }
            if item == "create:andesite_casing" || item == "create:brass_casing" {
                self.belt_casing.insert(
                    p,
                    if item.contains("brass") {
                        "brass"
                    } else {
                        "andesite"
                    }
                    .into(),
                );
                let s = with_prop(&self.name(self.at(p)), "casing", "true");
                self.switch(p, self.id(&s));
                return true;
            }
            if item == "create:wrench" && face != Dir::Up && self.belt_casing.remove(&p).is_some() {
                let s = with_prop(&self.name(self.at(p)), "casing", "false");
                self.switch(p, self.id(&s));
                return true;
            }
            if item == "create:wrench" && seg.part == belt::Part::Pulley && face.axis() != Axis::Y {
                let s = with_prop(&self.name(self.at(p)), "part", "middle");
                self.switch(p, self.id(&s));
                return true;
            }
        }
        if item != bc::ITEM {
            return false;
        }
        if self.now < self.connector_ready {
            return true;
        }
        match bc::click(self, sneaking, self.connector, p, true) {
            bc::Click::Forget | bc::Click::Fail { forget: true } => self.connector = None,
            bc::Click::Remember => {
                self.connector = Some(p);
                self.connector_ready = self.now + 5;
            }
            bc::Click::Connect(start) => {
                let axis = bc::Ground::shaft(self, start).unwrap();
                let layout = bc::layout(start, p, axis, |p| bc::Ground::shaft(self, p));
                for (p, seg) in layout {
                    if self.at(p) != AIR {
                        self.set(p, AIR);
                    }
                    let s = format!(
                        "create:belt[facing={},slope={},part={},casing=false,waterlogged=false]",
                        format!("{:?}", seg.facing).to_lowercase(),
                        bc::slope_name(seg.slope),
                        bc::part_name(seg.part)
                    );
                    self.switch(p, self.id(&s));
                }
                self.connector = None;
                self.connector_ready = self.now + 5;
            }
            _ => {}
        }
        true
    }
    pub(super) fn belt_breaks(&mut self) {
        belt::break_cascade(self);
    }
}
impl Run {
    pub(super) fn belt_observation(&self, p: Pos) -> Option<crate::belts::Observation> {
        let seg = self.w.table.get(self.at(p))?.1.belt?;
        let _ = seg;
        let c = self.w.dims[0].belt_of.get(&p).copied().unwrap_or(p);
        let b = self.w.dims[0].belts.get(&c);
        Some(crate::belts::Observation {
            controller: c,
            index: b
                .and_then(|b| b.chain.iter().position(|q| *q == p))
                .unwrap_or(0) as i32,
            length: b.map_or(0, |b| b.chain.len() as i32),
            casing: self.belt_casing.get(&p).cloned().unwrap_or("none".into()),
        })
    }
}
impl Run {
    fn segment(&self, p: Pos) -> Option<belt::Segment> {
        self.w.table.get(self.at(p))?.1.belt
    }
    fn segment_state(&self, seg: belt::Segment, cased: bool) -> u32 {
        let s = format!(
            "create:belt[facing={},slope={},part={},casing={cased},waterlogged=false]",
            format!("{:?}", seg.facing).to_lowercase(),
            seg.slope.name(),
            seg.part.name()
        );
        self.id(&s)
    }
    fn set_segment(&mut self, p: Pos, seg: belt::Segment, cased: bool) {
        let id = self.segment_state(seg, cased);
        self.switch(p, id);
    }
    fn set_segment_direct(&mut self, p: Pos, seg: belt::Segment, cased: bool) {
        let id = self.segment_state(seg, cased);
        self.set(p, id);
    }
    /// `BeltSlicer.useWrench`/`useConnector` as the glue's `logistics/slicer.rs` does them:
    /// true when the slicer answered the click.
    fn slice_belt(
        &mut self,
        p: Pos,
        item: &str,
        face: Dir,
        hit: [f64; 3],
        seg: belt::Segment,
    ) -> bool {
        let Some(c) = self.w.dims[0].belt_of.get(&p).copied() else {
            return false;
        };
        let length = self.w.dims[0].belts[&c].chain.len() as i32;
        if item == "create:wrench" {
            if self.belt_casing.contains_key(&p) && face != Dir::Up
                || seg.part == belt::Part::Pulley && face.axis() != Axis::Y
            {
                return false;
            }
            if length != 2 {
                self.slice_wrench(p, seg, length, hit);
            }
            return true;
        }
        if length == bc::MAX_LENGTH {
            return true;
        }
        if !seg.hovering_end(hit) {
            return false;
        }
        self.slice_connector(p, seg, length)
    }
    /// `use_wrench` past its pass-through checks: shorten from the hovered end, or split.
    fn slice_wrench(&mut self, p: Pos, seg: belt::Segment, length: i32, hit: [f64; 3]) {
        let v = seg.vector();
        let index = self.w.dims[0].belts[&self.w.dims[0].belt_of[&p]]
            .chain
            .iter()
            .position(|q| *q == p)
            .unwrap() as i32;
        if seg.hovering_end(hit) {
            let (_, taken) = self.w.take_belt(0, p).unwrap();
            let next = add(
                p,
                if seg.part == belt::Part::End {
                    v.map(|x| -x)
                } else {
                    v
                },
            );
            self.set_segment(next, seg, self.belt_casing.contains_key(&next));
            self.w.remove_belt_quietly(0, p);
            self.set(p, AIR);
            let mut items = taken.items.to_save();
            let chain: Vec<_> = taken.chain.iter().copied().filter(|q| *q != p).collect();
            // Items past the new end are thrown off (END) or dropped where the start was
            // (START); native item flight is outside the comparison.
            if seg.part == belt::Part::End {
                items.items.retain(|i| i.position <= (length - 1) as f32);
            } else {
                items.items = std::mem::take(&mut items.items)
                    .into_iter()
                    .map(|mut i| {
                        i.position -= 1.;
                        i
                    })
                    .filter(|i| i.position > 0.)
                    .collect();
            }
            self.w.reform_belt(0, chain[0], &chain, items);
            return;
        }
        let toward_positive = (0..3).map(|i| hit[i] * f64::from(v[i])).sum::<f64>() > 0.;
        let next = add(p, if toward_positive { v } else { v.map(|x| -x) });
        if index == 0 || index == 1 && !toward_positive {
            return;
        }
        if index == length - 1 || index == length - 2 && toward_positive {
            return;
        }
        let Some(next_seg) = self.segment(next) else {
            return;
        };
        // A survival split takes a belt connector and shafts from the inventory; the actor's
        // inventory holds neither (`use` equips only the held wrench), so it is refused.
        if !self.creative {
            return;
        }
        let (controller, taken) = self.w.take_belt(0, p).unwrap();
        let (here, there) = if toward_positive {
            (belt::Part::End, belt::Part::Start)
        } else {
            (belt::Part::Start, belt::Part::End)
        };
        self.set_segment(
            p,
            belt::Segment { part: here, ..seg },
            self.belt_casing.contains_key(&p),
        );
        self.set_segment(
            next,
            belt::Segment {
                part: there,
                ..next_seg
            },
            self.belt_casing.contains_key(&next),
        );
        let new_controller = if toward_positive { next } else { p };
        let shift = index as f32 + if toward_positive { 1. } else { 0. };
        let cut = ((index + i32::from(toward_positive)).max(0) as usize).min(taken.chain.len());
        let (first, second) = taken.chain.split_at(cut);
        let items = taken.items.to_save();
        let (moved, kept): (Vec<_>, Vec<_>) = items
            .items
            .into_iter()
            .partition(|i| i.position - shift > 0.);
        let moved = moved
            .into_iter()
            .map(|mut i| {
                i.position -= shift;
                i
            })
            .collect();
        self.w.reform_belt(
            0,
            controller,
            first,
            belt::Inventory::from_saved(kept, items.positive),
        );
        self.w.reform_belt(
            0,
            new_controller,
            second,
            belt::Inventory::from_saved(moved, items.positive),
        );
    }
    /// `use_connector` at a hovered end: join the belt there, or lengthen this one. False when
    /// the glue passes the click on to the connector item (a block there that is no belt).
    fn slice_connector(&mut self, p: Pos, seg: belt::Segment, length: i32) -> bool {
        let v = seg.vector();
        let next = add(
            p,
            if seg.part == belt::Part::Start {
                v.map(|x| -x)
            } else {
                v
            },
        );
        if self.at(next) != AIR {
            let Some(other) = self.segment(next) else {
                return false;
            };
            if !seg.compatible(&other) {
                return true;
            }
            let Some(oc) = self.w.dims[0].belt_of.get(&next).copied() else {
                return true;
            };
            let other_length = self.w.dims[0].belts[&oc].chain.len() as i32;
            if other_length + length > bc::MAX_LENGTH {
                return true;
            }
            let (_, mine) = self.w.take_belt(0, p).unwrap();
            let (_, theirs) = self.w.take_belt(0, next).unwrap();
            let flip = seg.facing != other.facing;
            let mut their_items = theirs.items.to_save();
            let mut their_chain = theirs.chain;
            if flip {
                for q in &their_chain {
                    if let Some(s) = self.segment(*q) {
                        self.set_segment_direct(*q, s.flipped(), self.belt_casing.contains_key(q));
                    }
                }
                for i in &mut their_items.items {
                    i.position = other_length as f32 - i.position;
                }
                their_chain.reverse();
            }
            for q in [p, next] {
                self.set_segment(
                    q,
                    belt::Segment {
                        part: belt::Part::Middle,
                        ..seg
                    },
                    self.belt_casing.contains_key(&q),
                );
            }
            let my_items = mine.items.to_save();
            let (chain, mut all) = if seg.part == belt::Part::End {
                let mut chain = mine.chain;
                chain.extend(their_chain);
                let mut all = my_items.items;
                all.extend(their_items.items.into_iter().map(|mut i| {
                    i.position += length as f32;
                    i
                }));
                (chain, all)
            } else {
                let mut chain = their_chain;
                chain.extend(mine.chain);
                let mut all = their_items.items;
                all.extend(my_items.items.into_iter().map(|mut i| {
                    i.position += other_length as f32;
                    i
                }));
                (chain, all)
            };
            all.sort_by(|a, b| b.position.total_cmp(&a.position));
            self.w
                .reform_belt(0, chain[0], &chain, belt::Inventory::from_saved(all, true));
            return true;
        }
        let (_, taken) = self.w.take_belt(0, p).unwrap();
        self.set_segment(
            p,
            belt::Segment {
                part: belt::Part::Middle,
                ..seg
            },
            self.belt_casing.contains_key(&p),
        );
        self.set_segment_direct(next, seg, false);
        let mut items = taken.items.to_save();
        let mut chain = taken.chain;
        if seg.part == belt::Part::Start {
            chain.insert(0, next);
            for i in &mut items.items {
                i.position += 1.;
            }
        } else {
            chain.push(next);
        }
        self.w.reform_belt(0, chain[0], &chain, items);
        true
    }
}

impl belt::BreakWorld for Run {
    fn take_breaks(&mut self) -> Vec<create_engine::world::BeltBreak> {
        std::mem::take(&mut self.w.dims[0].belt_breaks)
    }
    fn replace(&mut self, b: create_engine::world::BeltBreak) {
        let s = b.shaft.map(|axis| {
            format!(
                "create:shaft[axis={},waterlogged=false]",
                format!("{axis:?}").to_lowercase()
            )
        });
        self.set(b.pos, s.map_or(AIR, |s| self.id(&s)));
        self.belt_casing.remove(&b.pos);
    }
}
