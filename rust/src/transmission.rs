//! Player kinetic-board traces, including controller input from either shaft or cog.
use crate::scenario::Scenario;

pub fn crank(seed: u64) -> Scenario {
    let events: Vec<_> = [(20, false), (29, false), (39, true), (50, false)]
        .into_iter()
        .map(|(tick, sneaking)| {
            serde_json::json!({
            "tick":tick,"pos":[0,8,0],"action":{"kind":"use","item":"minecraft:air",
            "face":"up","hit":[0.5,1,0.5],"yaw":0,"pitch":0,"sneaking":sneaking}})
        })
        .collect();
    serde_json::from_value(serde_json::json!({"version":3,"seed":seed,"ticks":160,
        "checkpoints":(20..53).collect::<Vec<_>>(),"blocks":[
        {"pos":[0,7,0],"state":"minecraft:stone"},
        {"pos":[0,8,0],"state":"create:hand_crank[facing=up,waterlogged=false]"},
        {"pos":[0,9,0],"state":"create:shaft[axis=y,waterlogged=false]"}],"events":events}))
    .unwrap()
}

pub fn generate(seed: u64) -> Scenario {
    let reverse = seed & 16 != 0;
    let mut blocks = serde_json::json!([
        {"pos":[0,8,0],"state":"create:rotation_speed_controller[axis=x]"},
        {"pos":[0,9,0],"state":"create:large_cogwheel[axis=z,waterlogged=false]"},
        {"pos":[1,8,0],"state":"create:shaft[axis=x,waterlogged=false]"},
        {"pos":[2,8,0],"state":"create:mechanical_press[facing=east]"}
    ]);
    blocks.as_array_mut().unwrap().push(if reverse {
        serde_json::json!({"pos":[0,9,-1],"state":"create:creative_motor[facing=south]","motor":32})
    } else {
        serde_json::json!({"pos":[-1,8,0],"state":"create:creative_motor[facing=east]","motor":32})
    });
    let motor = if reverse { [0, 9, -1] } else { [-1, 8, 0] };
    let mut events = Vec::new();
    for (i, value) in [0, 1, 16, 128, 256].into_iter().enumerate() {
        for row in 0..2 {
            events.push(serde_json::json!({"tick":20+i*16+row*8,"pos":[0,8,0],
                "action":{"kind":"speed_setting","row":row,"value":value}}));
        }
    }
    events.push(serde_json::json!({"tick":110,"pos":motor,"action":{"kind":"speed_setting","row":0,"value":64}}));
    serde_json::from_value(serde_json::json!({"version":3,"seed":seed,"ticks":190,
        "checkpoints":[19,28,44,60,76,92,120],"blocks":blocks,"events":events}))
    .unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn controller_board_zero_column_selects_one_and_reverses() {
        let mut sc = super::generate(14);
        sc.checkpoints = vec![21, 29];
        let snap = crate::engine::run(&sc);
        assert_eq!(
            snap.checkpoints[&21].blocks["0,8,0"]
                .kinetic
                .as_ref()
                .unwrap()
                .speed_setting,
            Some(-1.)
        );
        assert_eq!(
            snap.checkpoints[&29].blocks["0,8,0"]
                .kinetic
                .as_ref()
                .unwrap()
                .speed_setting,
            Some(1.)
        );
    }
}

fn use_block(
    tick: u32,
    pos: [i32; 3],
    item: &str,
    yaw: f32,
    pitch: f32,
    sneaking: bool,
) -> serde_json::Value {
    serde_json::json!({"tick":tick,"pos":pos,"action":{"kind":"use","item":item,"face":"up","hit":[0.5,1,0.5],"yaw":yaw,"pitch":pitch,"sneaking":sneaking}})
}

pub fn chain(seed: u64) -> Scenario {
    let mut blocks = Vec::new();
    let mut events = Vec::new();
    for z in 0..5 {
        blocks.push(serde_json::json!({"pos":[0,7,z],"state":"minecraft:stone"}));
        events.push(use_block(
            10 + z as u32 * 2,
            [0, 7, z],
            if z % 2 == 0 {
                "create:encased_chain_drive"
            } else {
                "create:adjustable_chain_gearshift"
            },
            90.,
            0.,
            true,
        ));
    }
    blocks.push(serde_json::json!({"pos":[-1,8,0],"state":"create:creative_motor[facing=east]","motor":if seed&16==0 {32} else {-32}}));
    for (i, strength) in (0..16).chain([0, 1, 14, 15, 14, 1, 0]).enumerate() {
        events.push(serde_json::json!({"tick":30+i*4,"pos":[0,9,1],"action":{"kind":"signal","strength":strength}}));
        events.push(serde_json::json!({"tick":30+i*4,"pos":[0,9,3],"action":{"kind":"signal","strength":15-strength}}));
    }
    events.push(serde_json::json!({"tick":130,"pos":[0,8,2],"action":{"kind":"remove"}}));
    events.push(use_block(
        140,
        [0, 7, 2],
        "create:encased_chain_drive",
        90.,
        0.,
        true,
    ));
    serde_json::from_value(serde_json::json!({"version":3,"seed":seed,"ticks":220,"checkpoints":[25,125,135,150],"blocks":blocks,"events":events})).unwrap()
}
