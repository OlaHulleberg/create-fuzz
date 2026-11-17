//! Plain player stacks and semantic transport observations. Component-bearing items need
//! their own contract; random render angle and lateral jitter are not gameplay positions.
use crate::{actions::Face, scenario::Pos};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stack {
    pub item: String,
    pub count: u8,
}
impl Stack {
    pub fn limit(&self) -> u8 {
        contract()
            .items
            .iter()
            .find(|i| i.id == self.item)
            .unwrap_or_else(|| panic!("unsupported transport item {}", self.item))
            .max_stack
    }
    pub fn validate(&self) {
        assert!(self.count > 0 && self.count <= self.limit());
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    /// A player's dropped stack at its controlled landing boundary, not native-world flight.
    DropOn { stack: Stack },
    /// Real game-mode use on the supplied face; None is an empty hand.
    Use {
        stack: Option<Stack>,
        face: Face,
        yaw: f32,
        sneaking: bool,
    },
}
impl Input {
    pub fn validate(&self) {
        match self {
            Self::DropOn { stack } => stack.validate(),
            Self::Use { stack, yaw, .. } => {
                if let Some(s) = stack {
                    s.validate();
                }
                assert!(yaw.is_finite());
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub stack: Stack,
    pub position: f32,
    pub locked: bool,
    pub pending: bool,
    pub stalled: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contents {
    pub kind: String,
    pub items: Vec<Item>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub pos: Pos,
    pub offered: Option<Stack>,
    pub remainder: Option<Stack>,
    pub taken: Vec<Stack>,
}

/// Controlled item histories on player-connected belts. No slicing or pulley edits while loaded.
pub fn generate(seed: u64) -> crate::scenario::Scenario {
    use crate::{
        actions::{Action, Face},
        scenario::{Change, Placed, Scenario},
    };
    let mode = (seed / 32) % 5;
    let mut sc = Scenario {
        version: 3,
        seed,
        ticks: 270,
        day_time: 0,
        checkpoints: (40..=120).collect(),
        blocks: vec![],
        events: vec![],
    };
    let mut block = |pos, state: &str, motor| {
        sc.blocks.push(Placed {
            pos,
            state: state.into(),
            motor,
        })
    };
    block([0, 8, 0], "create:shaft[axis=x,waterlogged=false]", None);
    block([0, 8, 4], "create:shaft[axis=x,waterlogged=false]", None);
    block(
        [-1, 8, 0],
        "create:creative_motor[facing=east]",
        Some(if mode == 3 { -32 } else { 32 }),
    );
    if mode == 1 || mode == 4 {
        block([0, 8, 5], "create:depot[waterlogged=false]", None);
    } else if mode == 2 {
        block([0, 8, 5], "create:shaft[axis=x,waterlogged=false]", None);
        block([0, 8, 8], "create:shaft[axis=x,waterlogged=false]", None);
        block([-1, 8, 5], "create:creative_motor[facing=east]", Some(32));
        block([0, 8, 9], "minecraft:stone", None);
    } else {
        block([0, 8, 5], "minecraft:stone", None);
        block([0, 8, -1], "minecraft:stone", None);
    }
    let mut event = |tick, pos, action| {
        sc.events.push(Change {
            tick,
            pos,
            action: Some(action),
            state: None,
            motor: None,
            turn: None,
        })
    };
    let connector = || Action::Use {
        item: "create:belt_connector".into(),
        face: Face::Up,
        hit: [0.5, 1., 0.5],
        yaw: 0.,
        pitch: 0.,
        sneaking: false,
    };
    event(10, [0, 8, 0], connector());
    event(20, [0, 8, 4], connector());
    if mode == 2 {
        event(26, [0, 8, 5], connector());
        event(32, [0, 8, 8], connector());
    }
    let at = if mode == 3 { [0, 8, 3] } else { [0, 8, 1] };
    for (tick, item, count) in [
        (40, "minecraft:iron_ingot", 16),
        (41, "minecraft:gold_ingot", 3),
        (65, "minecraft:gold_ingot", 7),
        (90, "minecraft:ender_pearl", 16),
    ] {
        event(
            tick,
            at,
            Action::Item {
                input: Input::DropOn {
                    stack: Stack {
                        item: item.into(),
                        count,
                    },
                },
            },
        );
    }
    if mode == 4 {
        event(
            36,
            [0, 8, 5],
            Action::Item {
                input: Input::Use {
                    stack: Some(Stack {
                        item: "minecraft:diamond".into(),
                        count: 2,
                    }),
                    face: Face::Up,
                    yaw: 90.,
                    sneaking: false,
                },
            },
        );
    }
    let pickup = if mode == 1 || mode == 4 {
        [0, 8, 5]
    } else if mode == 3 {
        [0, 8, 0]
    } else {
        [0, 8, 4]
    };
    event(
        160,
        pickup,
        Action::Item {
            input: Input::Use {
                stack: None,
                face: Face::Up,
                yaw: 0.,
                sneaking: false,
            },
        },
    );
    if mode == 3 {
        event(100, [-1, 8, 0], Action::SpeedSetting { row: 1, value: 32 });
    }
    if mode == 4 {
        event(
            180,
            [0, 8, 5],
            Action::Item {
                input: Input::Use {
                    stack: Some(Stack {
                        item: "minecraft:iron_ingot".into(),
                        count: 1,
                    }),
                    face: Face::Up,
                    yaw: 180.,
                    sneaking: false,
                },
            },
        );
        event(
            195,
            [0, 8, 5],
            Action::Item {
                input: Input::Use {
                    stack: None,
                    face: Face::Up,
                    yaw: 0.,
                    sneaking: false,
                },
            },
        );
    }
    if mode == 0 {
        sc.events.push(Change {
            tick: 75,
            pos: [-1, 8, 0],
            action: Some(Action::Remove),
            state: None,
            motor: None,
            turn: None,
        });
        sc.events.push(Change {
            tick: 100,
            pos: [-1, 8, 0],
            action: None,
            state: Some("create:creative_motor[facing=east]".into()),
            motor: Some(32),
            turn: None,
        });
        sc.events.sort_by_key(|e| e.tick);
    }
    sc.checkpoints.extend(130..=150);
    sc.checkpoints.extend([159, 161, 180, 181, 195, 196, 200]);
    sc
}

#[derive(Deserialize)]
pub struct Contract {
    pub blocks: Vec<BlockContract>,
    pub items: Vec<ItemContract>,
}
#[derive(Deserialize)]
pub struct BlockContract {
    pub id: String,
    pub properties: std::collections::BTreeMap<String, Vec<String>>,
    pub source: String,
}
#[derive(Deserialize)]
pub struct ItemContract {
    pub id: String,
    pub max_stack: u8,
}
pub fn contract() -> &'static Contract {
    static C: std::sync::OnceLock<Contract> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        serde_json::from_str(include_str!("transport.json")).expect("transport contract")
    })
}
pub fn validate_state(id: &str, props: &[(String, String)]) -> Option<Result<(), String>> {
    let entry = contract().blocks.iter().find(|b| b.id == id)?;
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
            Err(format!("unsupported transport state {id}: {props:?}"))
        },
    )
}

/// A bounded transport world: unsupported machine/collision inputs fail before either runner.
pub fn validate_scenario(sc: &crate::scenario::Scenario) {
    if !sc
        .events
        .iter()
        .any(|e| matches!(e.action, Some(crate::actions::Action::Item { .. })))
    {
        return;
    }
    for b in sc
        .blocks
        .iter()
        .cloned()
        .chain(sc.events.iter().filter_map(|e| e.placed()))
    {
        let id = crate::scenario::parse_state(&b.state).0;
        assert!(
            [
                "minecraft:air",
                "minecraft:stone",
                "minecraft:redstone_block",
                "create:shaft",
                "create:creative_motor",
                "create:belt",
                "create:depot"
            ]
            .contains(&id.as_str()),
            "transport adapter does not model collision/processing for {id}"
        );
    }
    for e in &sc.events {
        if let Some(crate::actions::Action::Use { item, .. }) = &e.action {
            assert!(
                ["create:belt_connector", "minecraft:air"].contains(&item.as_str()),
                "loaded-belt structural edits and item overrides require a separate transport adapter"
            );
        }
    }
}
