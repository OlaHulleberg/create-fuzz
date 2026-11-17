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
                self.set(p, self.id(&s));
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
                self.set(p, self.id(&s));
                return true;
            }
            if item == "create:wrench" && face != Dir::Up && self.belt_casing.remove(&p).is_some() {
                let s = with_prop(&self.name(self.at(p)), "casing", "false");
                self.set(p, self.id(&s));
                return true;
            }
            if item == "create:wrench" && seg.part == belt::Part::Pulley && face.axis() != Axis::Y {
                let s = with_prop(&self.name(self.at(p)), "part", "middle");
                self.set(p, self.id(&s));
                return true;
            }
        }
        if item != bc::ITEM {
            return false;
        }
        match bc::click(self, sneaking, self.connector, p, true) {
            bc::Click::Forget | bc::Click::Fail { forget: true } => self.connector = None,
            bc::Click::Remember => self.connector = Some(p),
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
                    self.set(p, self.id(&s));
                }
                self.connector = None;
            }
            _ => {}
        }
        true
    }
    pub(super) fn belt_breaks(&mut self) {
        {
            let pending = std::mem::take(&mut self.w.dims[0].belt_breaks);

            for b in pending {
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
    fn set_segment(&mut self, p: Pos, seg: belt::Segment, cased: bool) {
        let s = format!(
            "create:belt[facing={},slope={},part={},casing={cased},waterlogged=false]",
            format!("{:?}", seg.facing).to_lowercase(),
            seg.slope.name(),
            seg.part.name()
        );
        self.set(p, self.id(&s));
    }
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
        let b = &self.w.dims[0].belts[&c];
        let length = b.chain.len();
        let v = seg.vector();
        let outward = seg.hovering_end(hit);
        if item == "create:wrench" {
            if self.belt_casing.contains_key(&p) && face != Dir::Up
                || seg.part == belt::Part::Pulley && face.axis() != Axis::Y
            {
                return false;
            }
            if length == 2 {
                return true;
            }
            if !outward {
                // Survival splitting requires a connector in the inventory. Uses equip only the
                // held wrench; no inventory provisioning is claimed by this grammar.
                return true;
            }
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
            let chain: Vec<_> = taken.chain.into_iter().filter(|q| *q != p).collect();
            self.w
                .reform_belt(0, chain[0], &chain, taken.items.to_save());
            return true;
        }
        if length == bc::MAX_LENGTH as usize {
            return true;
        }
        if !outward {
            return false;
        }
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
                return true;
            };
            if !seg.compatible(&other) {
                return true;
            }
            let Some(oc) = self.w.dims[0].belt_of.get(&next).copied() else {
                return true;
            };
            if self.w.dims[0].belts[&oc].chain.len() + length > bc::MAX_LENGTH as usize {
                return true;
            }
            let (_, mine) = self.w.take_belt(0, p).unwrap();
            let (_, theirs) = self.w.take_belt(0, next).unwrap();
            let flip = seg.facing != other.facing;
            let mut chain = theirs.chain;
            if flip {
                for q in &chain {
                    if let Some(s) = self.segment(*q) {
                        self.set_segment(*q, s.flipped(), self.belt_casing.contains_key(q));
                    }
                }
                chain.reverse();
            }
            self.set_segment(
                p,
                belt::Segment {
                    part: belt::Part::Middle,
                    ..seg
                },
                self.belt_casing.contains_key(&p),
            );
            self.set_segment(
                next,
                belt::Segment {
                    part: belt::Part::Middle,
                    ..seg
                },
                self.belt_casing.contains_key(&next),
            );
            if seg.part == belt::Part::End {
                let mut first = mine.chain;
                first.extend(chain);
                chain = first;
            } else {
                chain.extend(mine.chain);
            }
            self.w
                .reform_belt(0, chain[0], &chain, mine.items.to_save());
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
        self.set_segment(next, seg, false);
        let mut chain = taken.chain;
        if seg.part == belt::Part::Start {
            chain.insert(0, next);
        } else {
            chain.push(next);
        }
        self.w
            .reform_belt(0, chain[0], &chain, taken.items.to_save());
        true
    }
}
