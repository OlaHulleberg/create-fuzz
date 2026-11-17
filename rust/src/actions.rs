//! Version 2 player inputs. Unsupported glue interactions fail explicitly.
use create_engine::kinetics::Dir;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Place {
        block: String,
        yaw: f32,
        pitch: f32,
        face: Face,
        sneaking: bool,
    },
    Wrench {
        face: Face,
    },
    Remove,
    Signal {
        strength: u8,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Face {
    Down,
    Up,
    North,
    South,
    West,
    East,
}
impl Face {
    pub fn dir(self) -> Dir {
        match self {
            Self::Down => Dir::Down,
            Self::Up => Dir::Up,
            Self::North => Dir::North,
            Self::South => Dir::South,
            Self::West => Dir::West,
            Self::East => Dir::East,
        }
    }
}
/// Only blocks whose complete placement and default wrench rules are driven here.
pub const BLOCKS: &[&str] = &[
    "create:shaft",
    "create:creative_motor",
    "create:clutch",
    "create:gearshift",
    "create:gearbox",
    "create:flywheel",
];
impl Action {
    pub fn validate(&self) {
        match self {
            Self::Signal { strength } => assert!(*strength <= 15, "signal outside 0..15"),
            Self::Place {
                block, yaw, pitch, ..
            } => {
                assert!(
                    BLOCKS.contains(&block.as_str()),
                    "placement requires an unsupported glue adapter: {block}"
                );
                assert!(yaw.is_finite() && pitch.is_finite() && pitch.abs() <= 90.0);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn versioned_actions_validate_and_reject_unknown_inputs() {
        let action: super::Action =
            serde_json::from_str(r#"{"kind":"signal","strength":7}"#).unwrap();
        action.validate();
        assert!(
            serde_json::from_str::<super::Action>(r#"{"kind":"signal","strength":7,"extra":true}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<super::Action>(r#"{"kind":"casing"}"#).is_err());
        for seed in (3..256).step_by(4) {
            let case = crate::generate::generate(seed);
            let encoded = serde_json::to_string(&case).unwrap();
            let decoded = serde_json::from_str(&encoded).unwrap();
            assert_eq!(case, decoded);
            crate::catalogue::scenario(&decoded);
            crate::engine::run(&decoded);
        }
    }
}
