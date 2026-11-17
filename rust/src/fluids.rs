//! Fluid tank contract and player histories. Native world-fluid simulation is a separate input.
use crate::scenario::Pos;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tank {
    pub controller: Pos,
    pub width: u8,
    pub height: u8,
    pub capacity: u32,
    pub fluid: Option<String>,
    pub amount: u32,
}
#[derive(Deserialize)]
pub struct Contract {
    pub blocks: Vec<Entry>,
    pub unimplemented: std::collections::BTreeMap<String, String>,
}
#[derive(Deserialize)]
pub struct Entry {
    pub id: String,
    pub properties: std::collections::BTreeMap<String, Vec<String>>,
}
pub fn contract() -> &'static Contract {
    static CONTRACT: std::sync::OnceLock<Contract> = std::sync::OnceLock::new();
    CONTRACT
        .get_or_init(|| serde_json::from_str(include_str!("fluids.json")).expect("fluid contract"))
}
pub fn validate_state(id: &str, props: &[(String, String)]) -> Option<Result<(), String>> {
    let entry = contract().blocks.iter().find(|e| e.id == id)?;
    let mut seen = std::collections::BTreeSet::new();
    Some(
        if props.iter().all(|(k, v)| {
            seen.insert(k)
                && entry
                    .properties
                    .get(k)
                    .is_some_and(|values| values.contains(v))
        }) {
            Ok(())
        } else {
            Err("invalid fluid tank state".into())
        },
    )
}
pub fn report() {
    for entry in &contract().blocks {
        println!("fluid {}: {:?}", entry.id, entry.properties);
    }
    for (group, reason) in &contract().unimplemented {
        println!("fluid excluded {group}: {reason}");
    }
}
pub fn generate(seed: u64) -> crate::scenario::Scenario {
    use crate::{
        actions::{Action, Face},
        scenario::{Change, Placed, Scenario},
    };
    let width = 1 + (seed / 32 % 3) as i32;
    let height = 1 + (seed / 96 % 3) as i32;
    let mut sc = Scenario {
        version: 3,
        seed,
        day_time: 0,
        ticks: 110,
        checkpoints: vec![30, 40, 60, 80],
        blocks: vec![],
        events: vec![],
    };
    for x in 0..width {
        for z in 0..width {
            sc.blocks.push(Placed {
                pos: [x, 3, z],
                state: "minecraft:stone".into(),
                motor: None,
            });
        }
    }
    let mut tick = 12;
    for y in 0..height {
        for x in 0..width {
            for z in 0..width {
                sc.events.push(Change {
                    tick,
                    pos: [x, 3 + y, z],
                    action: Some(Action::Use {
                        item: "create:fluid_tank".into(),
                        face: Face::Up,
                        hit: [0.5, 1., 0.5],
                        yaw: 0.,
                        pitch: 0.,
                        sneaking: true,
                    }),
                    state: None,
                    motor: None,
                    turn: None,
                });
                tick += 1;
            }
        }
    }
    sc.events.push(Change {
        tick: 55,
        pos: [0, 4, 0],
        action: Some(Action::Use {
            item: "create:wrench".into(),
            face: Face::North,
            hit: [0.5, 0.5, 0.],
            yaw: 0.,
            pitch: 0.,
            sneaking: false,
        }),
        state: None,
        motor: None,
        turn: None,
    });
    sc.events.push(Change {
        tick: 70,
        pos: [0, 4, 0],
        action: Some(Action::Remove),
        state: None,
        motor: None,
        turn: None,
    });
    sc
}
