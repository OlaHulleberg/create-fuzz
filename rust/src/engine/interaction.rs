use super::*;

impl Run {
    pub(super) fn action(&mut self, p: Pos, action: &crate::actions::Action) {
        use crate::actions::Action;
        match action {
            Action::GameMode { creative } => self.creative = *creative,
            Action::Reload => self.reload(),
            Action::Item { input } => self.item_input(p, input),
            Action::SpeedSetting { row, value } => {
                self.w.board_motor(
                    0,
                    p,
                    create_engine::board::Settings {
                        row: *row,
                        value: *value,
                    },
                );
                self.publish_interaction();
            }
            Action::ValveSetting { row, value } => {
                if let Some(h) = self.handles.get_mut(&p) {
                    h.selected = create_engine::board::kinetic_value(
                        create_engine::board::Settings {
                            row: *row,
                            value: *value,
                        },
                        180,
                    );
                }
                self.publish_interaction();
            }
            Action::Program { instructions } => {
                // sequencer::client_message rejects late screen packets after replacement.
                if self.w.block_at(0, p) != Some(create_engine::Block::SequencedGearshift) {
                    return;
                }
                if let Some(step) = self.w.set_sequencer_instructions(
                    0,
                    p,
                    instructions.iter().map(|i| i.engine()).collect(),
                    true,
                ) {
                    self.sequencer_step(p, step);
                    let list = &self.w.dims[0].sequencers[&p].instructions;
                    self.persisted.sequences.entry(p).or_default().insert(
                        "Instructions".into(),
                        crate::persistence::codec::to_tag(list),
                    );
                    self.mark_column(p);
                }
            }
            Action::ChainLink { target, connect } => self.chain_link(p, *target, *connect),
            Action::Remove => self.set(p, AIR),
            Action::Signal { strength } => self.set(p, 3 + u32::from(*strength)),
            Action::Use {
                item,
                face,
                hit,
                yaw,
                pitch,
                sneaking,
            } => {
                // The native host constructs an absolute hit, then the glue subtracts
                // the clicked block. Preserve that rounding in every batch slot.
                let point = [0, 1, 2].map(|i| {
                    let base = f64::from(self.native_origin[i] + p[i]);
                    (base + hit[i]) - base
                });
                self.use_item(p, item, face.dir(), point, *yaw, *pitch, *sneaking);
            }
        }
    }

    /// Glue order: RightClickBlock, block use_item_on, then the item's use_on.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn use_item(
        &mut self,
        p: Pos,
        item: &str,
        face: create_engine::kinetics::Dir,
        hit: [f64; 3],
        yaw: f32,
        pitch: f32,
        sneaking: bool,
    ) {
        use create_engine::{Block, board, place};
        if item == "minecraft:air" && self.transport_empty_use(p, face, sneaking) {
            return;
        }
        if self.belt_use(p, item, face, hit, sneaking) {
            return;
        }
        if self.fluid_use(p, item, face, hit, sneaking) {
            return;
        }
        if self.world_bucket(p, item, face, hit, sneaking) {
            return;
        }
        if self.pipe_use(p, item, face, hit, sneaking) {
            return;
        }
        let state = self.at(p);
        let own = self.w.table.get(state).map(|(b, i)| (b, *i));
        if let Some((block, info)) = own.filter(|(b, _)| create_engine::dye::is_valve_handle(*b)) {
            // interaction/values.rs: a box takes the click only while its behaviour is active.
            if !sneaking
                && self.w.value_box_active(0, p)
                && let Some(slot) = info.facing.and_then(|f| board::valve_slot(f, face))
                && board::hits_valve_box(slot, hit)
            {
                return;
            }
            if !(sneaking && item == "create:wrench") {
                if let Some(to) = create_engine::dye::DyeColor::of_item(item)
                    .and_then(|c| create_engine::dye::recoloured(block, c))
                {
                    let (_, props) = parse_state(&self.name(state));
                    let next =
                        crate::scenario::format_state(&format!("create:{}", to.name()), &props);
                    self.set(p, self.id(&next));
                    self.publish_interaction();
                } else {
                    let selected = self.handles.get(&p).map_or(45, |h| h.selected);
                    if self.w.use_valve(0, p, sneaking, selected) {
                        self.handles.entry(p).or_default().started(sneaking);
                        self.publish_interaction();
                    }
                }
                return;
            }
        }
        if item == "minecraft:air" && own.is_some_and(|(b, _)| b == Block::HandCrank) {
            self.w.turn_crank(0, p, sneaking);
            self.publish_interaction();
            return;
        }
        // WrenchItem.useOn dispatches a sneaking belt use to
        // IWrenchable.onSneakWrenched (destroyBlock), handled below like the glue.
        if let Some((block, _)) = own {
            let name = format!("create:{}", block.name());
            if !crate::fluids::contract()
                .blocks
                .iter()
                .any(|entry| entry.id == name)
                && !crate::actions::BLOCKS.contains(&name.as_str())
                && name != "create:andesite_encased_shaft"
                && name != "create:brass_encased_shaft"
                && name != "create:brass_casing"
                && name != "create:andesite_casing"
                && !(name == "create:belt"
                    && sneaking
                    && [
                        "create:andesite_casing",
                        "create:brass_casing",
                        "create:wrench",
                    ]
                    .contains(&item))
            {
                self.notes.push(format!("adapter gap: RightClickBlock/value-box and block-use handlers for {name} at {p:?} are outside the supported subset; input retained"));
                return;
            }
        }
        // interaction/values.rs: the motor's active value box consumes the click first.
        if !sneaking
            && let Some((Block::CreativeMotor, info)) = own
            && self.w.value_box_active(0, p)
            && let Some(slot) = info.facing.and_then(|f| board::motor_slot(f, face))
            && board::hits_box(slot, hit)
        {
            return;
        }
        if !sneaking {
            // decor/brackets.rs runs before the wrench item's use_on.
            if item == "create:wrench" && {
                let removed = self.brackets.remove(&p).is_some();
                if removed {
                    self.mark_column(p);
                }
                removed
            } {
                return;
            }
            // transmission.rs Clicked: casing, then the shaft placement helper.
            if let Some((Block::Shaft, info)) = own {
                let axis = info.axis.expect("shaft axis");
                if item == "create:brass_casing" || item == "create:andesite_casing" {
                    let next = format!(
                        "create:{}_encased_shaft[axis={}]",
                        if item == "create:brass_casing" {
                            "brass"
                        } else {
                            "andesite"
                        },
                        format!("{axis:?}").to_lowercase()
                    );
                    self.switch(p, self.id(&next));
                    return;
                }
                if item == "create:shaft" {
                    let at = place::pole_helper(
                        p,
                        axis,
                        hit.map(|v| v - 0.5),
                        |q| {
                            self.w
                                .table
                                .get(self.at(q))
                                .is_some_and(|(b, i)| b == Block::Shaft && i.axis == Some(axis))
                        },
                        |q| self.at(q) == AIR,
                    );
                    if let Some(q) = at {
                        let next = format!(
                            "create:shaft[axis={},waterlogged=false]",
                            format!("{axis:?}").to_lowercase()
                        );
                        self.set(q, self.id(&next));
                        return;
                    }
                }
            }
        }
        // Item use_on: bracket, wrench (including subsystem casing removal), BlockItem.
        if item == "create:metal_bracket" || item == "create:wooden_bracket" {
            if let Some((Block::Shaft, info)) = own {
                let toward = place::ordered_by_nearest(yaw, pitch)[0].opposite();
                if let Some(b) = create_engine::decor::bracket_for_click(
                    item == "create:wooden_bracket",
                    info.axis,
                    create_engine::decor::BracketKind::Shaft,
                    face,
                    toward,
                ) {
                    self.brackets.insert(p, b);
                    self.mark_column(p);
                }
            } else if own.is_some() {
                self.notes.push(format!(
                    "adapter gap: bracket target outside shaft subset at {p:?}"
                ));
            }
            return;
        }
        if item == "create:wrench" {
            let (name, props) = parse_state(&self.name(state));
            if name == "minecraft:air" || name == "minecraft:stone" {
                return;
            }
            if sneaking {
                if name == "create:andesite_encased_shaft" || name == "create:brass_encased_shaft" {
                    let axis = props.iter().find(|(k, _)| k == "axis").unwrap().1.clone();
                    self.switch(
                        p,
                        self.id(&format!("create:shaft[axis={axis},waterlogged=false]")),
                    );
                } else {
                    self.set(p, AIR);
                }
                return;
            }
            if !crate::actions::BLOCKS.contains(&name.as_str())
                && !name.ends_with("_encased_shaft")
                && !name.ends_with("_casing")
            {
                self.notes.push(format!(
                    "adapter gap: unsupported wrench override at {p:?}: {name}"
                ));
                return;
            }
            let rotated = create_engine::wrench::rotated(&props.into_iter().collect(), false, face);
            let next =
                crate::scenario::format_state(&name, &rotated.into_iter().collect::<Vec<_>>());
            self.switch(p, self.id(&next));
            return;
        }
        if item == "minecraft:air"
            || create_engine::dye::DyeColor::of_item(item).is_some()
            || (!crate::actions::BLOCKS.contains(&item)
                && !["create:andesite_casing", "create:brass_casing"].contains(&item))
        {
            return;
        }
        // No override of the clicked position: air replaces itself; solid clicks place adjacent.
        let p = if self.replaceable(p) {
            p
        } else {
            add(p, face.offset())
        };
        if !self.replaceable(p) {
            return;
        }
        self.place_player(p, item, yaw, pitch, face, sneaking);
    }

    pub(super) fn switch(&mut self, p: Pos, id: u32) {
        if self.at(p) == id {
            return;
        }
        let inherited = self.measured_trig.get(&p).cloned().filter(|property| {
            let old = parse_state(&self.name(self.at(p))).1;
            let new = parse_state(&self.name(id)).1;
            old.iter().find(|(k, _)| k == property) == new.iter().find(|(k, _)| k == property)
                && new.iter().any(|(k, _)| k == property)
        });
        self.w.switching_block_state(0, p, id);
        self.set(p, id);
        if let Some(property) = inherited {
            self.measured_trig.insert(p, property);
        }
    }
    pub(super) fn place_player(
        &mut self,
        p: Pos,
        block: &str,
        yaw: f32,
        pitch: f32,
        face: create_engine::kinetics::Dir,
        sneaking: bool,
    ) {
        if self.place_pipe(p, block, yaw, pitch, sneaking) {
            return;
        }
        if block == "create:fluid_tank" {
            self.set(
                p,
                self.id("create:fluid_tank[bottom=true,shape=window,top=true]"),
            );
            // TankItem.on_block_placed_by publishes the accepted placement.
            self.publish_interaction();
            return;
        }
        if [
            "create:brass_casing",
            "create:andesite_casing",
            "create:copper_casing",
        ]
        .contains(&block)
        {
            self.set(p, self.id(block));
            return;
        }
        let b = create_engine::Block::ALL
            .iter()
            .copied()
            .find(|b| format!("create:{}", b.name()) == block)
            .expect("block");
        let ctx = create_engine::place::Context {
            yaw,
            pitch,
            sneaking,
            clicked_face: face,
        };
        let shaft = |d: create_engine::kinetics::Dir| {
            self.w
                .has_shaft_towards(0, add(p, d.offset()), d.opposite())
        };
        let mut next = if let Some(f) = create_engine::place::facing(b, ctx, shaft) {
            format!("{block}[facing={}]", format!("{f:?}").to_lowercase())
        } else {
            let (a, _) = create_engine::place::axis(
                b,
                ctx,
                shaft,
                create_engine::place::Nearby {
                    large_cog_above: self
                        .w
                        .cog_at(0, add(p, [0, 1, 0]))
                        .and_then(|(large, axis)| large.then_some(axis)),
                    ..Default::default()
                },
            )
            .expect("placement axis");
            format!("{block}[axis={}]", format!("{a:?}").to_lowercase())
        };
        // Start from the port's declared default for properties placement does not select.
        let set: Vec<String> = parse_state(&next).1.into_iter().map(|(k, _)| k).collect();
        for (k, v) in crate::defaults::declared(block) {
            if !set.contains(&k) {
                next = with_prop(&next, &k, &v);
            }
        }
        if block == "create:shaft" {
            next = with_prop(&next, "waterlogged", "false");
        }
        if block == "create:clutch" || block == "create:gearshift" {
            next = with_prop(
                &next,
                "powered",
                if self.signal(p) > 0 { "true" } else { "false" },
            );
        }
        if block == "create:adjustable_chain_gearshift" {
            next = with_prop(&next, "powered", "false");
            if self.signal(p) > 0 {
                self.notes.push(
                    "API_GAPS: redstone unavailable during state_for_placement; corrected on tick"
                        .into(),
                );
            }
        }
        let near = create_engine::place::Nearby {
            large_cog_above: self
                .w
                .cog_at(0, add(p, [0, 1, 0]))
                .and_then(|(large, axis)| large.then_some(axis)),
            ..Default::default()
        };
        let measured = crate::trig::exact_tie(yaw, pitch)
            .then(|| crate::trig::nearest_property(b, ctx, shaft, near))
            .flatten();
        if self.java_ties
            && let Some(property) = measured
        {
            next = crate::trig::java_choice(&next, property);
        }
        self.set(p, self.id(&next));
        if let Some(property) = measured {
            self.measured_trig.insert(p, property.into());
        }
    }
}
