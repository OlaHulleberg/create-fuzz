//! Seeded scenario dispatch and shared deterministic random primitive.

use crate::scenario::Scenario;

/// SplitMix64: small, seedable, the same everywhere.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: u32) -> u32 {
        (self.next() % u64::from(n.max(1))) as u32
    }
    pub fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len() as u32) as usize]
    }
}

/// Runs past Create's 60-tick kinetic validation interval.
pub const SETTLE: u32 = 70;

pub fn generate(seed: u64) -> Scenario {
    if seed % 4 == 3 {
        return actions(seed);
    }
    crate::geometry::generate(seed)
}

fn actions(seed: u64) -> Scenario {
    use crate::actions::{Action, Face};
    use crate::scenario::{Change, Placed};
    let mut sc = Scenario {
        version: 2,
        seed,
        ticks: 180,
        checkpoints: vec![10, 30, 60],
        blocks: vec![
            Placed {
                pos: [0, 8, 0],
                state: "create:creative_motor[facing=east]".into(),
                motor: Some(32),
            },
            Placed {
                pos: [2, 7, 0],
                state: "minecraft:stone".into(),
                motor: None,
            },
        ],
        events: vec![],
    };
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
    let block = ["create:shaft", "create:clutch", "create:gearshift"][(seed as usize / 4) % 3];
    event(
        3,
        [1, 8, 0],
        Action::Place {
            block: block.into(),
            yaw: 90.,
            pitch: 0.,
            face: Face::Up,
            sneaking: seed % 8 == 3,
        },
    );
    event(
        4,
        [2, 8, 0],
        Action::Place {
            block: "create:shaft".into(),
            yaw: 90.,
            pitch: 0.,
            face: Face::Up,
            sneaking: false,
        },
    );
    event(
        8,
        [1, 9, 0],
        Action::Signal {
            strength: ((seed / 4) % 16) as u8,
        },
    );
    event(
        40,
        [1, 9, 0],
        Action::Signal {
            strength: ((seed / 64) % 16) as u8,
        },
    );
    event(70, [2, 8, 0], Action::Wrench { face: Face::Up });
    event(90, [1, 8, 0], Action::Remove);
    sc
}
