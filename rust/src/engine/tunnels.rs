//! Andesite and brass tunnels over belts, mirroring glue/logistics/tunnel.rs: the casing their
//! placement puts on the belt (`BeltTunnelItem`), their shape from what lies beside their
//! segment (`BeltTunnelBlock.getTunnelState`/`updateShape`), the wrench's window toggle
//! (`onWrenched`), their flaps and sides (`updateTunnelConnections`), and an andesite junction
//! handing single items to the outputs beside it (`BeltTunnelInteractionHandler`).
use super::*;
use create_engine::{
    Block, belt,
    kinetics::{Axis, Dir},
    tunnel,
};

impl Run {
    /// The tunnel's axis and shape at `p`; None for any other block.
    pub(super) fn tunnel_state(&self, p: Pos) -> Option<(Axis, tunnel::Shape)> {
        let (block, info) = self.w.table.get(self.at(p))?;
        matches!(block, Block::AndesiteTunnel | Block::BrassTunnel)
            .then_some((info.axis?, info.tunnel?))
    }
    /// The belt funnel at `p`: its facing and shape.
    pub(super) fn belt_funnel_at(&self, p: Pos) -> Option<(Dir, create_engine::funnel::BeltShape)> {
        let (block, info) = self.w.table.get(self.at(p))?;
        matches!(block, Block::AndesiteBeltFunnel | Block::BrassBeltFunnel)
            .then(|| Some((info.facing?, info.belt_funnel.unwrap_or_default())))
            .flatten()
    }
    /// An extended belt funnel at `p` facing `facing`.
    fn extended_funnel(&self, p: Pos, facing: Dir) -> bool {
        self.belt_funnel_at(p)
            .is_some_and(|(f, s)| s == create_engine::funnel::BeltShape::Extended && f == facing)
    }
    /// `flaps`: the open sides that no tunnel continues and no extended belt funnel facing back
    /// into it closes.
    pub(super) fn tunnel_flaps(&self, p: Pos, axis: Axis, shape: tunnel::Shape) -> Vec<Dir> {
        tunnel::sides(axis, shape)
            .into_iter()
            .filter(|d| {
                let next = add(p, d.offset());
                self.tunnel_state(next).is_none() && !self.extended_funnel(next, d.opposite())
            })
            .collect()
    }
    pub(super) fn tunnel_observation(&self, p: Pos) -> Option<crate::logistics::Tunnel> {
        let (axis, shape) = self.tunnel_state(p)?;
        Some(crate::logistics::Tunnel {
            flaps: crate::logistics::names(self.tunnel_flaps(p, axis, shape)),
            sides: crate::logistics::names(tunnel::sides(axis, shape)),
        })
    }
    fn belt_below(&self, p: Pos) -> Option<belt::Segment> {
        self.w
            .table
            .get(self.at([p[0], p[1] - 1, p[2]]))
            .and_then(|(_, i)| i.belt)
    }
    fn cased_below(&self, p: Pos) -> bool {
        self.belt_cased([p[0], p[1] - 1, p[2]])
    }
    /// `tick_tunnels`: tunnels just placed case their horizontal belt in their own kind; those
    /// placed or whose neighbours changed take the shape their outputs give, and break when no
    /// cased horizontal belt holds them.
    pub(super) fn tick_tunnels(&mut self) {
        let d = &mut self.w.dims[0];
        let new = std::mem::take(&mut d.tunnels_new);
        let kept = std::mem::take(&mut d.tunnel_checks);
        let fresh = std::mem::take(&mut d.tunnel_fresh);
        for pos in new {
            if self.tunnel_state(pos).is_none() {
                continue;
            }
            if self
                .belt_below(pos)
                .is_some_and(|s| s.slope == belt::Slope::Horizontal)
                && !self.cased_below(pos)
            {
                let brass = self
                    .w
                    .table
                    .get(self.at(pos))
                    .is_some_and(|(b, _)| b == Block::BrassTunnel);
                self.set_belt_casing(
                    [pos[0], pos[1] - 1, pos[2]],
                    Some(if brass { "brass" } else { "andesite" }),
                );
            }
        }
        let mut checks: Vec<(Pos, bool)> = fresh
            .into_iter()
            .map(|p| (p, true))
            .chain(kept.into_iter().map(|p| (p, false)))
            .collect();
        checks.sort_by_key(|c| (c.0, !c.1));
        checks.dedup_by_key(|c| c.0);
        for (pos, at_once) in checks {
            let Some(now) = self.tunnel_state(pos) else {
                continue;
            };
            let seg = self
                .belt_below(pos)
                .filter(|s| s.slope == belt::Slope::Horizontal);
            let Some(seg) = seg.filter(|_| self.cased_below(pos)) else {
                // `destroyBlock(pos, true)`: its drop is native item flight.
                self.set(pos, AIR);
                continue;
            };
            let axis = seg.facing.axis();
            let output = |side: Dir| {
                let q = add([pos[0], pos[1] - 1, pos[2]], side.offset());
                if let Some((Block::Belt, i)) = self.w.table.get(self.at(q)) {
                    return i.belt.is_some_and(|b| b.facing.axis() == side.axis());
                }
                self.direct_input_at(q) && self.direct_input_accepts(q, side)
            };
            let left = tunnel::left(axis);
            let (on_left, on_right) = (output(left), output(left.opposite()));
            let fw = if axis == Axis::X {
                Dir::East
            } else {
                Dir::South
            };
            let ends = [fw, fw.opposite()].map(|d| {
                let next = add(pos, d.offset());
                let funnel = self.extended_funnel(next, d.opposite());
                (self.tunnel_state(next).is_some() || funnel, funnel)
            });
            let window = ends[0].0 && ends[1].0 && !(ends[0].1 && ends[1].1);
            let fresh = (axis, tunnel::shape_for(on_left, on_right, window));
            let (axis2, shape2) = if at_once {
                fresh
            } else {
                tunnel::updated(now, fresh)
            };
            if (axis2, shape2) == now {
                continue;
            }
            let name = self.name(self.at(pos));
            let next = with_prop(
                &with_prop(&name, "axis", if axis2 == Axis::X { "x" } else { "z" }),
                "shape",
                shape2.name(),
            );
            self.set(pos, self.id(&next));
        }
    }
    /// A tunnel item or a wrench on a tunnel (`BeltTunnelBlock.onWrenched`, the wrench's
    /// `onSneakWrenched`); true when the use was the tunnel's.
    pub(super) fn tunnel_use(&mut self, p: Pos, item: &str, face: Dir, sneaking: bool) -> bool {
        if item == "create:wrench"
            && let Some((_, shape)) = self.tunnel_state(p)
        {
            if sneaking {
                self.set(p, AIR);
                return true;
            }
            let next = match shape {
                tunnel::Shape::Window => "closed",
                tunnel::Shape::Closed => "window",
                _ => return true,
            };
            let s = with_prop(&self.name(self.at(p)), "shape", next);
            // `setBlock(..., UPDATE_CLIENTS)`: the neighbours are not told.
            self.commit(p, self.id(&s), false);
            return true;
        }
        if item != "create:andesite_tunnel" && item != "create:brass_tunnel" {
            return false;
        }
        // The game places the block item with the port's default state; `tick_tunnels`
        // then cases the belt and gives the tunnel its shape, or breaks it.
        let at = if self.replaceable(p) {
            p
        } else {
            add(p, face.offset())
        };
        if self.replaceable(at) {
            let state = format!("{item}[axis=x,shape=straight]");
            self.set(at, self.id(&state));
        }
        true
    }
    /// `flapTunnelsAndCheckIfStuck` for the belt `controller` (segment `seg`, length `length`,
    /// moving along `movement`): an andesite junction over segment `segment` hands single items
    /// to the outputs beside it while more than one is left. True when the item must wait.
    pub(super) fn tunnel_takes(
        &mut self,
        controller: Pos,
        seg: belt::Segment,
        length: i32,
        movement: Dir,
        segment: i32,
        item: &mut belt::Item,
    ) -> bool {
        if seg.slope != belt::Slope::Horizontal {
            return false;
        }
        let at = belt::position_for_offset(controller, &seg, length, segment);
        let t = [at[0], at[1] + 1, at[2]];
        let Some((axis, shape)) = self.tunnel_state(t) else {
            return false;
        };
        if self
            .w
            .table
            .get(self.at(t))
            .is_some_and(|(b, _)| b == Block::BrassTunnel)
        {
            // Brass distribution needs its group, filters and modes: a brass tunnel without
            // `hasDistributionBehaviour` lets items through.
            if self.brass_distributes(t, axis, shape) {
                self.notes.push(format!(
                    "adapter gap: brass tunnel distribution at {t:?} is outside the supported subset"
                ));
            }
            return false;
        }
        if !tunnel::splits(axis, shape, movement, item.count) {
            return false;
        }
        for d in tunnel::sides(axis, shape) {
            if d.axis() == axis {
                continue;
            }
            if self.tunnel_state(add(t, d.offset())).is_some() {
                continue;
            }
            let out = add(at, d.offset());
            if !self.direct_input_accepts(out, d) {
                continue;
            }
            let one = belt::Item {
                count: 1,
                ..item.clone()
            };
            match self.handle_insertion(out, d, &one, true) {
                None => continue,
                Some(0) => {}
                Some(_) => return true,
            }
            item.count -= 1;
            if item.count <= 1 {
                break;
            }
        }
        false
    }
    /// `has_distribution`: flaps, and connected with a brass tunnel beside it
    /// (`brass_connected`) or a flap across its axis.
    pub(super) fn brass_distributes(&self, p: Pos, axis: Axis, shape: tunnel::Shape) -> bool {
        let f = self.tunnel_flaps(p, axis, shape);
        !f.is_empty() && (self.brass_connected(p, axis) || f.iter().any(|d| d.axis() != axis))
    }
    /// `brass_connected`: it has flaps and so does a brass tunnel of its axis beside it.
    fn brass_connected(&self, p: Pos, axis: Axis) -> bool {
        let has_flaps = |q: Pos| {
            self.tunnel_state(q)
                .is_some_and(|(a, s)| !self.tunnel_flaps(q, a, s).is_empty())
        };
        let brass = |q: Pos| {
            self.w
                .table
                .get(self.at(q))
                .is_some_and(|(b, i)| b == Block::BrassTunnel && i.axis == Some(axis))
        };
        let sides = if axis == Axis::X {
            [Dir::North, Dir::South]
        } else {
            [Dir::East, Dir::West]
        };
        has_flaps(p)
            && sides.into_iter().any(|d| {
                let q = add(p, d.offset());
                brass(q) && has_flaps(q)
            })
    }
}

impl Run {
    /// `FunnelBlockEntity.extractionCooldown`: only extracting and pushing funnels count it
    /// down (`tick`, `startCooldown`); the driver takes items from belts, so a funnel in either
    /// mode is an adapter gap and the others hold 0.
    pub(super) fn funnel_observation(&self, p: Pos) -> Option<crate::logistics::Funnel> {
        let (block, info) = self.w.table.get(self.at(p))?;
        match block {
            Block::AndesiteBeltFunnel | Block::BrassBeltFunnel => {
                Some(crate::logistics::Funnel { cooldown: 0 })
            }
            Block::AndesiteFunnel | Block::BrassFunnel if !info.extracting => {
                Some(crate::logistics::Funnel { cooldown: 0 })
            }
            _ => None,
        }
    }
    /// The funnels in a mode the driver does not run (`determineCurrentMode`): a belt funnel
    /// pushing onto its belt, an extracting funnel.
    pub(super) fn note_funnel_modes(&mut self) {
        let mut gaps = Vec::new();
        for (&p, &id) in &self.world {
            let Some((block, info)) = self.w.table.get(id) else {
                continue;
            };
            let pushing = match block {
                Block::AndesiteBeltFunnel | Block::BrassBeltFunnel if !info.powered => {
                    match info.belt_funnel.unwrap_or_default() {
                        create_engine::funnel::BeltShape::Pushing => true,
                        create_engine::funnel::BeltShape::Pulling => false,
                        _ => {
                            let below = [p[0], p[1] - 1, p[2]];
                            self.belt_location(below).is_some_and(|(c, _, seg)| {
                                let rpm = self.w.speed(0, c).unwrap_or(0.);
                                belt::movement_facing(rpm, seg.facing)
                                    == info.facing.unwrap_or(Dir::Up)
                            })
                        }
                    }
                }
                Block::AndesiteFunnel | Block::BrassFunnel => info.extracting && !info.powered,
                _ => false,
            };
            if pushing {
                gaps.push(p);
            }
        }
        gaps.sort();
        for p in gaps {
            let note = format!("adapter gap: funnel extraction at {p:?}");
            if !self.notes.contains(&note) {
                self.notes.push(note);
            }
        }
    }
}
