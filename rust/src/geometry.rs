//! Static connection primitives derived from Create 6.0.11 IRotate/ICogWheel methods.
//! Manifest metadata owns block classification; no generator-side block lists.
use crate::{
    catalogue,
    scenario::{Change, Placed, Pos, Scenario, format_state},
};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisRule {
    Property,
    Facing,
    Y,
    Clockwise,
    Directional,
    Saw,
    Sequencer,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaftRule {
    Axis,
    OffAxis,
    Front,
    Back,
    Down,
    Up,
    Clockwise,
    None,
    Flags,
    Saw,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cog {
    Small,
    Large,
}
#[derive(Deserialize)]
pub struct Connection {
    axis: AxisRule,
    shafts: ShaftRule,
    cog: Option<Cog>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    axis: usize,
    positive: bool,
}
impl Face {
    fn offset(self) -> Pos {
        let mut p = [0; 3];
        p[self.axis] = if self.positive { 1 } else { -1 };
        p
    }
    fn opposite(self) -> Self {
        Self {
            positive: !self.positive,
            ..self
        }
    }
    fn name(self) -> &'static str {
        [["west", "east"], ["down", "up"], ["north", "south"]][self.axis]
            [usize::from(self.positive)]
    }
}
const FACES: [Face; 6] = [
    Face {
        axis: 0,
        positive: false,
    },
    Face {
        axis: 0,
        positive: true,
    },
    Face {
        axis: 1,
        positive: false,
    },
    Face {
        axis: 1,
        positive: true,
    },
    Face {
        axis: 2,
        positive: false,
    },
    Face {
        axis: 2,
        positive: true,
    },
];
fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn axis(s: &str) -> usize {
    match s {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => panic!("unknown axis {s}"),
    }
}
fn facing(s: &str) -> Face {
    *FACES.iter().find(|f| f.name() == s).expect("facing")
}
fn clockwise(f: Face) -> Face {
    match f.name() {
        "north" => FACES[1],
        "east" => FACES[5],
        "south" => FACES[0],
        "west" => FACES[4],
        _ => panic!("horizontal facing required"),
    }
}

pub struct Specification {
    pub state: String,
    shafts: Vec<Face>,
    cog: Option<Cog>,
    axis: usize,
}
impl catalogue::Entry {
    /// Enumerate the complete manifest domain, rather than relying on Java defaults.
    pub fn specifications(&self) -> Vec<Specification> {
        let mut domains = vec![BTreeMap::<String, String>::new()];
        for (key, values) in &self.properties {
            domains = domains
                .into_iter()
                .flat_map(|props| {
                    values
                        .iter()
                        .filter(|v| key != "waterlogged" || v.as_str() == "false")
                        .map(move |v| {
                            let mut p = props.clone();
                            p.insert(key.clone(), v.clone());
                            p
                        })
                })
                .collect();
        }
        domains
            .into_iter()
            .map(|p| {
                let get = |k: &str| p.get(k).map(String::as_str).unwrap_or("");
                let f = if p.contains_key("facing") {
                    facing(get("facing"))
                } else {
                    FACES[3]
                };
                let directional = || {
                    let other: Vec<_> = (0..3).filter(|a| *a != f.axis).collect();
                    other[usize::from(get("axis_along_first") != "true")]
                };
                let a = match self.connection.axis {
                    AxisRule::Property => axis(get("axis")),
                    AxisRule::Facing => f.axis,
                    AxisRule::Y => 1,
                    AxisRule::Clockwise => clockwise(f).axis,
                    AxisRule::Directional => directional(),
                    AxisRule::Saw => {
                        if f.axis == 1 {
                            directional()
                        } else {
                            f.axis
                        }
                    }
                    AxisRule::Sequencer => {
                        if get("vertical") == "true" {
                            1
                        } else {
                            axis(get("axis"))
                        }
                    }
                };
                let shafts = FACES
                    .into_iter()
                    .filter(|d| match self.connection.shafts {
                        ShaftRule::Axis => d.axis == a,
                        ShaftRule::OffAxis => d.axis != a,
                        ShaftRule::Front => *d == f,
                        ShaftRule::Back => *d == f.opposite(),
                        ShaftRule::Down => *d == FACES[2],
                        ShaftRule::Up => *d == FACES[3],
                        ShaftRule::Clockwise => *d == clockwise(f),
                        ShaftRule::None => false,
                        ShaftRule::Flags => {
                            d.axis == a
                                && get(if d.positive {
                                    "top_shaft"
                                } else {
                                    "bottom_shaft"
                                }) == "true"
                        }
                        ShaftRule::Saw => {
                            if f.axis == 1 {
                                d.axis == a
                            } else {
                                *d == f.opposite()
                            }
                        }
                    })
                    .collect();
                Specification {
                    state: format_state(&self.id, &p.into_iter().collect::<Vec<_>>()),
                    shafts,
                    cog: self.connection.cog,
                    axis: a,
                }
            })
            .collect()
    }
}
fn specifications() -> &'static [Specification] {
    static SPECS: std::sync::OnceLock<Vec<Specification>> = std::sync::OnceLock::new();
    SPECS.get_or_init(|| {
        catalogue::entries()
            .iter()
            .filter(|e| e.excluded.is_none())
            .flat_map(|e| e.specifications())
            .collect()
    })
}
fn placed(pos: Pos, state: String, motor: Option<i32>) -> Placed {
    Placed { pos, state, motor }
}
/// Each seed visits a complete state; successive cycles rotate all faces, signs and orders.
/// Eight independent probes share a case: six shaft faces plus two cog geometries.
pub fn generate(seed: u64) -> Scenario {
    if seed.is_multiple_of(8) {
        return stress_boundary(seed);
    }
    if seed % 16 == 15 {
        return network(seed);
    }
    let seed_index = seed - seed / 8 - seed / 16 - 1;
    let specs = specifications();
    let s = &specs[seed_index as usize % specs.len()];
    let cycle = seed_index / specs.len() as u64;
    let mut blocks = Vec::new();
    for (i, face) in FACES.into_iter().enumerate() {
        let p = [(i as i32 % 3) * 5 - 5, 8, (i as i32 / 3) * 5 - 3];
        let q = add(p, face.offset());
        let speed = if (seed + i as u64).is_multiple_of(2) {
            32
        } else {
            -32
        };
        let subject = placed(
            p,
            s.state.clone(),
            s.state
                .starts_with("create:creative_motor")
                .then_some(speed),
        );
        let motor = placed(
            q,
            format!("create:creative_motor[facing={}]", face.opposite().name()),
            Some(speed),
        );
        // Both insertion orders, on alternate visits. The face probes include every nonmatch.
        if (cycle + i as u64).is_multiple_of(2) {
            blocks.extend([motor, subject]);
        } else {
            blocks.extend([subject, motor]);
        }
        if let Some(output) = s.shafts.iter().find(|d| **d != face) {
            blocks.push(placed(
                add(p, output.offset()),
                format!("create:shaft[axis={}]", ["x", "y", "z"][output.axis]),
                None,
            ));
        }
    }
    if let Some(cog) = s.cog {
        for j in 0..3 {
            let diagonal = j == 1;
            let p = [-6 + j * 6, 3, 5];
            let off: Vec<_> = (0..3).filter(|a| *a != s.axis).collect();
            let mut delta = [0; 3];
            delta[off[0]] = 1;
            if diagonal {
                delta[off[1]] = 1;
            }
            let q = add(p, delta);
            let kind = match (cog, diagonal) {
                (Cog::Small, false) | (Cog::Large, true) => "cogwheel",
                _ => "large_cogwheel",
            };
            let mut q = q;
            let target_axis = if j == 2 { off[0] } else { s.axis };
            if j == 2 {
                q = p;
                q[s.axis] += 1;
                q[target_axis] += 1;
            }
            let kind = if j == 2 { "large_cogwheel" } else { kind };
            let mut motor = q;
            motor[target_axis] -= 1;
            blocks.extend([
                placed(
                    motor,
                    format!(
                        "create:creative_motor[facing={}]",
                        Face {
                            axis: target_axis,
                            positive: true
                        }
                        .name()
                    ),
                    Some(if seed.is_multiple_of(2) { 128 } else { -128 }),
                ),
                placed(
                    q,
                    format!("create:{kind}[axis={}]", ["x", "y", "z"][target_axis]),
                    None,
                ),
                placed(p, s.state.clone(), None),
            ]);
        }
    }
    let end = blocks.len() as u32;
    // Source detach, reversal and reattachment reach network invalidation/rebuild paths.
    let first = blocks.iter().find(|b| b.motor.is_some()).unwrap().clone();
    let mut events = vec![Change::set(end + 70, first.pos, "minecraft:air")];
    for motor in blocks.iter().filter(|b| b.motor.is_some()) {
        let mut replacement = Change::set(end + 80, motor.pos, &motor.state);
        replacement.motor = Some(-motor.motor.unwrap());
        events.push(replacement);
    }
    Scenario {
        seed,
        ticks: end + 160,
        checkpoints: vec![end + 69],
        blocks,
        events,
    }
}

/// Crank capacity 8/rpm, ejector impact 2/rpm are
/// registered in AllBlocks. A fast small cog weights the ejector by two.
fn stress_boundary(seed: u64) -> Scenario {
    let sign = seed.is_multiple_of(16);
    let mut blocks = vec![
        placed([0, 8, 0], "create:large_cogwheel[axis=x]".into(), None),
        placed([-1, 8, 0], "create:hand_crank[facing=west]".into(), None),
        placed([0, 9, 1], "create:cogwheel[axis=x]".into(), None),
    ];
    // 1, 2, 3 ejectors at twice crank rpm: weighted loads 4, 8, 12.
    let n = match seed / 8 % 3 {
        0 => 1,
        1 => 2,
        _ => 3,
    };
    for i in 1..=n {
        blocks.push(placed(
            [i, 9, 1],
            "create:weighted_ejector[facing=north,waterlogged=false]".into(),
            None,
        ));
    }
    let end = blocks.len() as u32;
    let events = (end..end + 100)
        .step_by(4)
        .map(|t| Change::turn(t, [-1, 8, 0], sign))
        .collect();
    Scenario {
        seed,
        ticks: end + 100,
        blocks,
        events,
        checkpoints: Vec::new(),
    }
}

/// Grow networks through catalogue shaft faces, mixing competing sources and dead ends.
fn network(seed: u64) -> Scenario {
    let mut rng = crate::generate::Rng::new(seed);
    let specs = specifications();
    let mut blocks = vec![placed(
        [0, 8, 0],
        "create:creative_motor[facing=east]".into(),
        Some(32),
    )];
    let mut frontier = vec![([1, 8, 0], FACES[0])];
    for _ in 0..24 {
        if frontier.is_empty() {
            break;
        }
        let i = rng.below(frontier.len() as u32) as usize;
        let (p, from) = frontier.swap_remove(i);
        if blocks.iter().any(|b| b.pos == p)
            || p[0].abs() > 8
            || p[2].abs() > 8
            || !(3..=13).contains(&p[1])
        {
            continue;
        }
        let choices: Vec<_> = specs.iter().filter(|s| s.shafts.contains(&from)).collect();
        let s = *rng.pick(&choices);
        blocks.push(placed(
            p,
            s.state.clone(),
            s.state
                .starts_with("create:creative_motor")
                .then(|| *rng.pick(&[-64, -32, 16, 32, 64, 256])),
        ));
        for d in &s.shafts {
            frontier.push((add(p, d.offset()), d.opposite()));
        }
    }
    let end = blocks.len() as u32;
    Scenario {
        seed,
        ticks: end + 150,
        blocks,
        events: vec![Change::set(end + 70, [0, 8, 0], "minecraft:air")],
        checkpoints: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn weighted_stress_boundary_keeps_equality_running() {
        for (seed, stress, stopped) in [(0, 128.0, false), (8, 256.0, false), (16, 384.0, true)] {
            let snapshot = crate::engine::run(&super::stress_boundary(seed));
            let k = snapshot.blocks["0,8,0"].kinetic.as_ref().unwrap();
            assert_eq!(k.capacity, 256.0);
            assert_eq!(k.stress, stress);
            assert_eq!(k.overstressed, stopped);
        }
    }
    #[test]
    fn complete_domains_and_valid_scenarios() {
        let total: usize = crate::catalogue::entries()
            .iter()
            .filter(|e| e.excluded.is_none())
            .map(|e| e.specifications().len())
            .sum();
        assert!(total < 2000);
        for seed in 0..2000 {
            crate::catalogue::scenario(&super::generate(seed));
        }
        let enc = crate::catalogue::entries()
            .iter()
            .find(|e| e.id == "create:andesite_encased_cogwheel")
            .unwrap();
        assert_eq!(
            enc.specifications()
                .iter()
                .map(|s| s.shafts.len())
                .sum::<usize>(),
            12
        );
    }
}
