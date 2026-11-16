//! The scenario and snapshot formats, shared with the Java side (`java/`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Pos = [i32; 3];

/// One case: blocks placed one per tick in order (block i before tick i), changes at given
/// ticks, `ticks` ticks in all.
/// Positions are relative to the case's origin; the Java side places them on a stone floor.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Scenario {
    #[serde(default)]
    pub seed: u64,
    pub ticks: u32,
    pub blocks: Vec<Placed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Change>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Placed {
    pub pos: Pos,
    /// A block state as `/setblock` takes it: `create:shaft[axis=x]`.
    pub state: String,
    /// A creative motor's speed setting (rpm, -256..=256, never 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motor: Option<i32>,
}

/// At `tick` (before that tick runs): place a block (`state`; air removes it), or a player
/// turns the hand crank at `pos` (`turn`: true backwards, as when sneaking).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub tick: u32,
    pub pos: Pos,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motor: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<bool>,
}

impl Change {
    pub fn set(tick: u32, pos: Pos, state: &str) -> Change {
        Change { tick, pos, state: Some(state.into()), motor: None, turn: None }
    }
    pub fn turn(tick: u32, pos: Pos, back: bool) -> Change {
        Change { tick, pos, state: None, motor: None, turn: Some(back) }
    }
    pub fn placed(&self) -> Option<Placed> {
        Some(Placed { pos: self.pos, state: self.state.clone()?, motor: self.motor })
    }
}

/// What both sides write after the last tick.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Snapshot {
    pub side: String,
    /// Every non-air block at a scenario position, keyed `x,y,z`.
    pub blocks: BTreeMap<String, BlockSnap>,
    /// Notes from the side that ran it (errors, skipped parts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BlockSnap {
    pub state: String,
    /// Present for kinetic blocks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinetic: Option<Kinetic>,
    /// The Java block entity's saved data, to read mismatches by; not compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Kinetic {
    /// `getSpeed()`: 0 when overstressed.
    pub speed: f32,
    /// `getTheoreticalSpeed()`.
    pub theoretical: f32,
    pub overstressed: bool,
    /// The network's stress and capacity as the block holds them (`stress`, `capacity`).
    pub stress: f32,
    pub capacity: f32,
    /// Its own stress impact and capacity per rpm (`calculateStressApplied`,
    /// `calculateAddedStressCapacity`).
    pub impact: f32,
    pub added_capacity: f32,
    /// `getGeneratedSpeed()`.
    pub generated: f32,
    /// A clutch's or gearshift's halves: the speed each shaft face turns at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faces: Option<BTreeMap<String, f32>>,
}

pub fn key(p: Pos) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}

/// `create:shaft[axis=x,waterlogged=false]` -> (`create:shaft`, [(axis, x), ...]).
pub fn parse_state(s: &str) -> (String, Vec<(String, String)>) {
    match s.split_once('[') {
        None => (s.to_string(), Vec::new()),
        Some((name, rest)) => {
            let props = rest
                .trim_end_matches(']')
                .split(',')
                .filter(|p| !p.is_empty())
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            (name.to_string(), props)
        }
    }
}

pub fn format_state(name: &str, props: &[(String, String)]) -> String {
    if props.is_empty() {
        return name.to_string();
    }
    let mut sorted = props.to_vec();
    sorted.sort();
    let inner: Vec<String> = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{name}[{}]", inner.join(","))
}

/// The same state with one property changed (added when missing).
pub fn with_prop(state: &str, key: &str, value: &str) -> String {
    let (name, mut props) = parse_state(state);
    match props.iter_mut().find(|(k, _)| k == key) {
        Some(p) => p.1 = value.to_string(),
        None => props.push((key.to_string(), value.to_string())),
    }
    format_state(&name, &props)
}
