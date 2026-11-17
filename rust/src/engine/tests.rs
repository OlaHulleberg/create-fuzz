#[test]
fn busy_valve_box_passes_a_centred_dye_to_the_block() {
    // interaction/values.rs: `ValveHandleBlockEntity.showValue` hides the box while in use.
    let sc: crate::scenario::Scenario =
        serde_json::from_value(serde_json::json!({"ticks":80,"blocks":[
            {"pos":[0,7,0],"state":"minecraft:stone"},
            {"pos":[0,8,0],"state":"create:copper_valve_handle[facing=up,waterlogged=false]"}]}))
        .unwrap();
    let mut r = super::Run::new(&sc);
    for b in &sc.blocks {
        r.place(b);
    }
    let p = [0, 8, 0];
    let face = create_engine::kinetics::Dir::Up;
    r.use_item(p, "minecraft:air", face, [0.1, 0.5, 0.1], 0., 0., false);
    assert!(r.handles[&p].remaining > 0);
    r.use_item(p, "minecraft:red_dye", face, [0.5, 0.5, 0.5], 0., 0., false);
    assert_eq!(
        r.w.block_at(0, p),
        Some(create_engine::Block::RedValveHandle)
    );
}

#[test]
fn late_program_packet_cannot_reconfigure_a_replacement_motor() {
    let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({"ticks":80,
            "blocks":[{"pos":[0,8,0],"state":"create:sequenced_gearshift[axis=x,vertical=false,state=0]"}],
            "events":[{"tick":5,"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32}]})).unwrap();
    let mut r = super::Run::new(&sc);
    let p = [0, 8, 0];
    r.place(&sc.blocks[0]);
    r.place(&sc.events[0].placed().unwrap());
    let previous = r.w.sequencer_instructions(0, p);
    r.action(
        p,
        &crate::actions::Action::Program {
            instructions: vec![crate::program::Instruction {
                instruction: crate::program::Kind::Delay,
                modifier: crate::program::Modifier::Forward,
                value: 5,
            }],
        },
    );
    assert_eq!(r.w.sequencer_instructions(0, p), previous);
    assert_eq!(
        r.w.block_at(0, p),
        Some(create_engine::Block::CreativeMotor)
    );
}

#[test]
fn replaced_sequencer_has_only_the_current_blocks_observations() {
    let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
            "ticks":80,"blocks":[{"pos":[0,8,0],"state":"create:sequenced_gearshift[axis=x,vertical=false,state=0]"}],
            "events":[{"tick":5,"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32}]})).unwrap();
    let snap = super::run(&sc);
    let k = snap.blocks["0,8,0"].kinetic.as_ref().unwrap();
    assert_eq!(k.speed_setting, Some(32.));
    assert!(k.sequencer.is_none());
}

#[test]
fn client_only_power_commit_does_not_enqueue_neighbour_inputs() {
    let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
            "ticks":80,"blocks":[
            {"pos":[0,8,0],"state":"create:adjustable_chain_gearshift[axis=x,axis_along_first=false,part=none,powered=false]"},
            {"pos":[0,8,1],"state":"create:adjustable_chain_gearshift[axis=x,axis_along_first=false,part=none,powered=false]"}]})).unwrap();
    let mut r = super::Run::new(&sc);
    for b in &sc.blocks {
        r.place(b);
    }
    // Complete the placement-time initial signal read before changing power.
    for block in &sc.blocks {
        r.w.set_chain_signal(0, block.pos, 0);
    }
    r.w.take_redstone(0);
    let p = [0, 8, 0];
    let powered = r.id(&crate::scenario::with_prop(
        &sc.blocks[0].state,
        "powered",
        "true",
    ));
    r.commit(p, powered, false);
    assert!(r.w.take_redstone(0).is_empty());
    r.set(p, r.id(&sc.blocks[0].state));
    assert_eq!(r.w.take_redstone(0), vec![[0, 8, 1]]);
}

#[test]
fn network_observation_uses_driven_positive_split_half() {
    let sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
        "ticks": 80,
        "blocks": [
            {"pos":[1,8,0],"state":"create:creative_motor[facing=west]","motor":32},
            {"pos":[0,9,0],"state":"minecraft:redstone_block"},
            {"pos":[0,8,0],"state":"create:clutch[axis=x,powered=true]"},
            {"pos":[-1,8,0],"state":"create:shaft[axis=x]"}
        ]
    }))
    .unwrap();
    let snapshot = super::run(&sc);
    let networks = snapshot.networks.unwrap();
    assert_eq!(networks.partitions, vec![vec!["0,8,0", "1,8,0"]]);
    assert_eq!(networks.sizes["0,8,0"], 2);
    assert_eq!(
        snapshot.blocks["0,8,0"].kinetic.as_ref().unwrap().speed,
        -32.0
    );
}
#[test]
fn network_observations_group_identity_and_follow_removal() {
    let mut sc: crate::scenario::Scenario = serde_json::from_value(serde_json::json!({
        "ticks": 80,
        "blocks": [
            {"pos":[0,8,0],"state":"create:creative_motor[facing=east]","motor":32},
            {"pos":[1,8,0],"state":"create:shaft[axis=x]"},
            {"pos":[0,8,4],"state":"create:creative_motor[facing=east]","motor":32},
            {"pos":[1,8,4],"state":"create:shaft[axis=x]"},
            {"pos":[4,8,0],"state":"create:shaft[axis=x]"}
        ]
    }))
    .unwrap();
    let snapshot = super::run(&sc);
    assert!(snapshot.unavailable.is_empty());
    let networks = snapshot.networks.unwrap();
    assert_eq!(
        networks.partitions,
        vec![vec!["0,8,0", "1,8,0"], vec!["0,8,4", "1,8,4"]]
    );
    assert_eq!(networks.sizes.len(), 4);
    assert!(networks.sizes.values().all(|size| *size == 2));
    sc.events
        .push(crate::scenario::Change::set(10, [0, 8, 0], "minecraft:air"));
    let networks = super::run(&sc).networks.unwrap();
    assert_eq!(networks.partitions, vec![vec!["0,8,4", "1,8,4"]]);
    assert_eq!(networks.sizes.len(), 2);
}

#[test]
fn connector_builds_driven_chain_and_keeps_casing_kind() {
    let sc = crate::belts::generate(15);
    let snap = super::run(&sc);
    let at = &snap.checkpoints[&75].blocks["0,8,0"];
    assert_eq!(at.belt.as_ref().unwrap().length, 3);
    assert_eq!(at.belt.as_ref().unwrap().casing, "brass");
    assert_ne!(at.kinetic.as_ref().unwrap().speed, 0.);
    assert!(snap.blocks.values().all(|b| b.belt.is_none()));
}
#[test]
fn remote_conveyor_links_form_loop_and_disconnect_symmetrically() {
    let sc = crate::belts::generate(127);
    let snap = super::run(&sc);
    assert_eq!(
        snap.checkpoints[&40].blocks["0,8,0"]
            .conveyor_links
            .as_ref()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        snap.checkpoints[&80].blocks["0,8,0"]
            .conveyor_links
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        snap.blocks["0,8,0"].conveyor_links.as_ref().unwrap().len(),
        2
    );
}
