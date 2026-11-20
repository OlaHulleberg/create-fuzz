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
    let mut sc = generate_base(seed);
    if seed & 32 != 0 {
        crate::persistence::schedule(&mut sc);
    }
    sc
}

fn generate_base(seed: u64) -> Scenario {
    (FAMILIES[OWNERS[(seed % PERIOD) as usize] as usize].generate)(seed)
}

/// Seeds are dealt to scenario families by their residue modulo `PERIOD`: every residue has
/// exactly one owner (`build` refuses a collision, a gap or a family without a seed at compile
/// time), so no family can shadow another. Shares and their reasons are in README.md.
pub const PERIOD: u64 = 128;

/// A scenario family and the residues modulo `PERIOD` it owns, as `(modulus, residue)` classes;
/// each modulus divides `PERIOD`.
pub struct Family {
    pub name: &'static str,
    classes: &'static [(u64, u64)],
    generate: fn(u64) -> Scenario,
}

const fn family(
    name: &'static str,
    classes: &'static [(u64, u64)],
    generate: fn(u64) -> Scenario,
) -> Family {
    Family {
        name,
        classes,
        generate,
    }
}

/// The transmission traces keep the half of their residue modulo 16 below 64 modulo 128.
const fn low_half(r: u64) -> [(u64, u64); 4] {
    [(128, r), (128, r + 16), (128, r + 32), (128, r + 48)]
}

use crate::{geometry, network_inputs as net, transmission};

pub const FAMILIES: &[Family] = &[
    // Kinetic network histories: one residue modulo 64 each.
    family("network/stacked_chains", &[(64, 32)], net::stacked_chains),
    family("network/encased_wrench", &[(64, 24)], net::encased_wrench),
    family("network/opposed_motors", &[(64, 56)], net::opposed_motors),
    family("network/controller_axes", &[(64, 33)], net::controller_axes),
    family("network/wrench_pickup", &[(64, 57)], net::wrench_pickup),
    family("network/valve_rim", &[(64, 25)], net::valve_rim),
    family("network/girder_shaft", &[(64, 1)], net::girder_shaft),
    family("network/sequencer_pair", &[(64, 52)], net::sequencer_pair),
    family("network/held_crank", &[(64, 20)], net::held_crank),
    family(
        "network/contested_placement",
        &[(64, 4)],
        net::contested_placement,
    ),
    family("network/chain_wrench", &[(64, 36)], net::chain_wrench),
    family(
        "network/vertical_sequencer",
        &[(64, 7)],
        net::vertical_sequencer,
    ),
    family("network/shaft_extension", &[(64, 54)], net::shaft_extension),
    family("network/chain_junction", &[(64, 38)], net::chain_junction),
    family(
        "network/gauge_thresholds",
        &[(64, 22)],
        net::gauge_thresholds,
    ),
    family("network/controller_cog", &[(64, 6)], net::controller_cog),
    family("network/switched_shaft", &[(64, 53)], net::switched_shaft),
    family(
        "network/competing_sources",
        &[(64, 37)],
        net::competing_sources,
    ),
    family("network/instruments", &[(64, 21)], net::instruments),
    family("network/relays", &[(64, 5)], net::relays),
    // Environmental generators, on residues the transmission traces gave up.
    family(
        "waterwheel",
        &[(128, 74), (128, 90), (128, 106), (128, 122)],
        waterwheel,
    ),
    family(
        "steam_engine",
        &[(128, 75), (128, 91), (128, 107), (128, 123)],
        steam_engine,
    ),
    // Fluids and item transport.
    family("fluid_inputs", &[(32, 19)], crate::fluid_inputs::generate),
    family("fluid_pumps", &[(32, 18)], crate::fluid_pumps::generate),
    family("fluid_world", &[(32, 17)], crate::fluid_world::generate),
    family("fluid_pipes", &[(32, 16)], crate::fluid_pipes::generate),
    family("fluids", &[(32, 8)], crate::fluids::generate),
    family("transport", &[(32, 9)], crate::transport::generate),
    family("belts", &[(16, 15)], crate::belts::generate),
    // Transmission traces.
    family("transmission/valve", &low_half(10), transmission::valve),
    family(
        "transmission/sequence",
        &low_half(11),
        transmission::sequence,
    ),
    family("transmission/chain", &low_half(12), transmission::chain),
    family("transmission/crank", &low_half(13), transmission::crank),
    family(
        "transmission/generate",
        &low_half(14),
        transmission::generate,
    ),
    family("transmission/flicker", &[(64, 2)], transmission::flicker),
    // Player actions on a single shaft, clutch or gearshift.
    family(
        "actions",
        &[(64, 3), (128, 23), (128, 35), (128, 39), (128, 55)],
        actions,
    ),
    // Static kinetics.
    family("geometry/cog_boundary", &[(64, 34)], geometry::cog_boundary),
    family(
        "geometry/source_faces",
        &[(128, 76), (128, 108)],
        source_faces,
    ),
    family(
        "geometry/stress_boundary",
        &[(64, 0)],
        geometry::stress_boundary,
    ),
    family(
        "geometry/network",
        &[(128, 92), (128, 124)],
        geometry::network,
    ),
    family(
        "geometry/states",
        &[
            (128, 77),
            (128, 93),
            (128, 109),
            (128, 125),
            (128, 78),
            (128, 94),
            (128, 110),
            (128, 126),
            (128, 87),
            (128, 99),
            (128, 103),
            (128, 119),
        ],
        geometry_states,
    ),
];

fn waterwheel(seed: u64) -> Scenario {
    crate::waterwheel::generate(seed, index(seed))
}
fn steam_engine(seed: u64) -> Scenario {
    crate::steam_engine::generate(seed, index(seed))
}
fn source_faces(seed: u64) -> Scenario {
    geometry::source_faces(seed, index(seed))
}
fn geometry_states(seed: u64) -> Scenario {
    geometry::states(seed, index(seed))
}

/// The owning family of each residue.
const OWNERS: [u8; PERIOD as usize] = build();

const fn build() -> [u8; PERIOD as usize] {
    const NONE: u8 = u8::MAX;
    assert!(FAMILIES.len() < NONE as usize);
    let mut owners = [NONE; PERIOD as usize];
    let mut f = 0;
    while f < FAMILIES.len() {
        let classes = FAMILIES[f].classes;
        assert!(!classes.is_empty(), "a family without a seed");
        let mut c = 0;
        while c < classes.len() {
            let (modulus, residue) = classes[c];
            assert!(PERIOD.is_multiple_of(modulus) && residue < modulus);
            let mut r = residue;
            while r < PERIOD {
                assert!(owners[r as usize] == NONE, "two families claim a residue");
                owners[r as usize] = f as u8;
                r += modulus;
            }
            c += 1;
        }
        f += 1;
    }
    let mut r = 0;
    while r < PERIOD as usize {
        assert!(owners[r] != NONE, "a residue without a family");
        r += 1;
    }
    owners
}

/// The name of the family that generates `seed`.
pub fn family_name(seed: u64) -> &'static str {
    FAMILIES[OWNERS[(seed % PERIOD) as usize] as usize].name
}

/// The seed's place among its family's seeds: 0, 1, 2, ... in seed order. Families whose
/// variants were written for consecutive seeds pick them by this index.
pub fn index(seed: u64) -> u64 {
    let r = (seed % PERIOD) as usize;
    let owner = OWNERS[r];
    let share = OWNERS.iter().filter(|o| **o == owner).count() as u64;
    let rank = OWNERS[..r].iter().filter(|o| **o == owner).count() as u64;
    seed / PERIOD * share + rank
}

fn actions(seed: u64) -> Scenario {
    use crate::actions::{Action, Face};
    use crate::scenario::{Change, Placed};
    let mut sc = Scenario {
        version: 3,
        day_time: 0,
        seed,
        ticks: 180,
        checkpoints: vec![10, 30, 60, 73, 75, 77, 79, 101],
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
            Placed {
                pos: [1, 7, 0],
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
    let u = if seed % 8 == 3 { 0.5 } else { 0.1 };
    let usage = |item: &str, face: Face, sneaking| Action::Use {
        item: item.into(),
        face,
        hit: face.point(u, 0.5),
        yaw: 90.,
        pitch: 0.,
        sneaking,
    };
    let block = ["create:shaft", "create:clutch", "create:gearshift"][(seed as usize / 4) % 3];
    event(3, [1, 7, 0], usage(block, Face::Up, seed % 8 == 3));
    event(4, [2, 7, 0], usage("create:shaft", Face::Up, false));
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
    event(70, [2, 8, 0], usage("create:wrench", Face::Up, false));
    event(
        72,
        [2, 8, 0],
        usage(
            if seed % 8 == 3 {
                "create:wooden_bracket"
            } else {
                "create:metal_bracket"
            },
            Face::Up,
            false,
        ),
    );
    event(74, [2, 8, 0], usage("create:wrench", Face::Up, false));
    event(
        76,
        [2, 8, 0],
        usage(
            if seed % 8 == 3 {
                "create:brass_casing"
            } else {
                "create:andesite_casing"
            },
            Face::Up,
            false,
        ),
    );
    event(78, [2, 8, 0], usage("create:wrench", Face::Up, true));
    event(90, [1, 8, 0], Action::Remove);
    if seed % 32 == 3 {
        let hit = Face::South.point(
            if (seed / 32).is_multiple_of(2) {
                0.5
            } else {
                0.1
            },
            0.5,
        );
        event(
            100,
            [0, 8, 0],
            Action::Use {
                item: "create:wrench".into(),
                face: Face::South,
                hit,
                yaw: 0.,
                pitch: 0.,
                sneaking: false,
            },
        );
    }
    sc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The family each residue had in the if-chain this table replaced (8c63ea7).
    fn before(r: u64) -> String {
        const NETWORK: [(u64, &str); 20] = [
            (32, "stacked_chains"),
            (24, "encased_wrench"),
            (56, "opposed_motors"),
            (33, "controller_axes"),
            (57, "wrench_pickup"),
            (25, "valve_rim"),
            (1, "girder_shaft"),
            (52, "sequencer_pair"),
            (20, "held_crank"),
            (4, "contested_placement"),
            (36, "chain_wrench"),
            (7, "vertical_sequencer"),
            (54, "shaft_extension"),
            (38, "chain_junction"),
            (22, "gauge_thresholds"),
            (6, "controller_cog"),
            (53, "switched_shaft"),
            (37, "competing_sources"),
            (21, "instruments"),
            (5, "relays"),
        ];
        if let Some((_, name)) = NETWORK.iter().find(|(m, _)| r % 64 == *m) {
            return format!("network/{name}");
        }
        match (r % 32, r % 16) {
            (19, _) => "fluid_inputs",
            (18, _) => "fluid_pumps",
            (17, _) => "fluid_world",
            (16, _) => "fluid_pipes",
            (8, _) => "fluids",
            (9, _) => "transport",
            (_, 15) => "belts",
            (_, 10) => "transmission/valve",
            (_, 11) => "transmission/sequence",
            (_, 12) => "transmission/chain",
            (_, 13) => "transmission/crank",
            (_, 14) => "transmission/generate",
            _ if r % 64 == 2 => "transmission/flicker",
            _ if r % 64 == 34 => "geometry/cog_boundary",
            _ if r % 4 == 3 => "actions",
            _ => "geometry/stress_boundary",
        }
        .into()
    }

    #[test]
    fn every_family_has_seeds() {
        for (f, family) in FAMILIES.iter().enumerate() {
            assert!(
                OWNERS.contains(&(f as u8)),
                "{} is never generated",
                family.name
            );
        }
        let mut names: Vec<_> = FAMILIES.iter().map(|f| f.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), FAMILIES.len());
    }

    /// Only these residues changed owner; every other seed generates what it did before.
    #[test]
    fn moved_residues() {
        let moved: Vec<_> = (0..PERIOD)
            .filter(|r| before(*r) != family_name(*r))
            .collect();
        assert_eq!(
            moved,
            [
                74, 75, 76, 77, 78, 87, 90, 91, 92, 93, 94, 99, 103, 106, 107, 108, 109, 110, 119,
                122, 123, 124, 125, 126
            ]
        );
    }

    #[test]
    fn index_counts_a_family_seeds() {
        let mut next = vec![0; FAMILIES.len()];
        for seed in 0..4 * PERIOD {
            let f = OWNERS[(seed % PERIOD) as usize] as usize;
            assert_eq!(index(seed), next[f], "seed {seed}");
            next[f] += 1;
        }
    }
}
