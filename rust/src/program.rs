//! Screen packet payload, independent of engine storage.
use create_engine::sequencer;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Kind {
    TurnAngle,
    TurnDistance,
    Delay,
    Await,
    End,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Modifier {
    ForwardFast,
    Forward,
    Back,
    BackFast,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    pub instruction: Kind,
    pub modifier: Modifier,
    pub value: i32,
}
impl Instruction {
    pub fn validate(&self) {
        let max = match self.instruction {
            Kind::TurnAngle => 360,
            Kind::TurnDistance => 128,
            Kind::Delay => 600,
            Kind::Await | Kind::End => return,
        };
        assert!(
            (1..=max).contains(&self.value),
            "instruction parameter outside screen range"
        );
    }
    pub fn engine(&self) -> sequencer::Instruction {
        sequencer::Instruction {
            kind: match self.instruction {
                Kind::TurnAngle => sequencer::Kind::TurnAngle,
                Kind::TurnDistance => sequencer::Kind::TurnDistance,
                Kind::Delay => sequencer::Kind::Delay,
                Kind::Await => sequencer::Kind::Await,
                Kind::End => sequencer::Kind::End,
            },
            speed: match self.modifier {
                Modifier::ForwardFast => sequencer::Speed::ForwardFast,
                Modifier::Forward => sequencer::Speed::Forward,
                Modifier::Back => sequencer::Speed::Back,
                Modifier::BackFast => sequencer::Speed::BackFast,
            },
            value: self.value,
        }
    }
}
