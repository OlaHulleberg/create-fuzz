//! The scenario and snapshot formats, shared with the Java side (`java/`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Pos = [i32; 3];

/// One case: blocks placed in order at tick 0, changes at later ticks, then `ticks` ticks run.
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

/// At `tick` (before that tick runs): place a block (`set`) or remove one (`state` = air).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Change {
    pub tick: u32,
    #[serde(flatten)]
    pub block: Placed,
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
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
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
