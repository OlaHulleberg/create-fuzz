//! Random kinetic scenarios, grown from Create's connection rules (`RotationPropagator`,
//! each block's `hasShaftTowards`) so most blocks connect, with odd but legal cases mixed in:
//! second sources, wrong cog spacing, loops, overstressed cranks, redstone and blocks changed
//! mid-run.

use crate::scenario::{Change, Placed, Pos, Scenario};
use std::collections::HashMap;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    X,
    Y,
    Z,
}

const AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

impl Axis {
    fn name(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
            Axis::Z => "z",
        }
    }
    fn unit(self) -> Pos {
        match self {
            Axis::X => [1, 0, 0],
            Axis::Y => [0, 1, 0],
            Axis::Z => [0, 0, 1],
        }
    }
    fn others(self) -> [Axis; 2] {
        match self {
            Axis::X => [Axis::Y, Axis::Z],
            Axis::Y => [Axis::X, Axis::Z],
            Axis::Z => [Axis::X, Axis::Y],
        }
    }
}

/// A face: an axis and a sign.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Dir(Axis, bool);

const DIRS: [Dir; 6] = [
    Dir(Axis::X, true),
    Dir(Axis::X, false),
    Dir(Axis::Y, true),
    Dir(Axis::Y, false),
    Dir(Axis::Z, true),
    Dir(Axis::Z, false),
];

impl Dir {
    fn offset(self) -> Pos {
        let u = self.0.unit();
        let s = if self.1 { 1 } else { -1 };
        [u[0] * s, u[1] * s, u[2] * s]
    }
    fn opposite(self) -> Dir {
        Dir(self.0, !self.1)
    }
    fn name(self) -> &'static str {
        match self {
            Dir(Axis::X, true) => "east",
            Dir(Axis::X, false) => "west",
            Dir(Axis::Y, true) => "up",
            Dir(Axis::Y, false) => "down",
            Dir(Axis::Z, true) => "south",
            Dir(Axis::Z, false) => "north",
        }
    }
}

fn add(a: Pos, b: Pos) -> Pos {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// What the generator knows of a placed block: its shaft faces and cog.
#[derive(Clone, Debug)]
struct Info {
    shafts: Vec<Dir>,
    /// Small (false) or large (true) cog, on this axis.
    cog: Option<(Axis, bool)>,
    /// Splits on redstone (clutch, gearshift).
    redstone: bool,
    crank: bool,
}

fn shafts_along(a: Axis) -> Vec<Dir> {
    vec![Dir(a, true), Dir(a, false)]
}

/// One block, as a state string and what connects to it.
fn block(kind: &str, a: Axis, facing: Dir, powered: bool) -> (String, Info) {
    let info = |shafts, cog| Info {
        shafts,
        cog,
        redstone: false,
        crank: false,
    };
    let ax = a.name();
    match kind {
        "shaft" => (format!("create:shaft[axis={ax}]"), info(shafts_along(a), None)),
        "cogwheel" => (format!("create:cogwheel[axis={ax}]"), info(shafts_along(a), Some((a, false)))),
        "large_cogwheel" => (
            format!("create:large_cogwheel[axis={ax}]"),
            info(shafts_along(a), Some((a, true))),
        ),
        // Shafts on the four faces off its axis.
        "gearbox" => (
            format!("create:gearbox[axis={ax}]"),
            info(DIRS.iter().copied().filter(|d| d.0 != a).collect(), None),
        ),
        "clutch" | "gearshift" => {
            let mut i = info(shafts_along(a), None);
            i.redstone = true;
            (format!("create:{kind}[axis={ax},powered={powered}]"), i)
        }
        "creative_motor" => (
            format!("create:creative_motor[facing={}]", facing.name()),
            info(vec![facing], None),
        ),
        "hand_crank" => {
            let mut i = info(vec![facing.opposite()], None);
            i.crank = true;
            (format!("create:hand_crank[facing={}]", facing.name()), i)
        }
        "encased_fan" => (
            format!("create:encased_fan[facing={}]", facing.name()),
            info(vec![facing.opposite()], None),
        ),
        "mechanical_press" => (
            format!("create:mechanical_press[facing={}]", facing.name()),
            info(shafts_along(facing.0), None),
        ),
        "millstone" => ("create:millstone".into(), info(vec![Dir(Axis::Y, false)], Some((Axis::Y, false)))),
        "mechanical_mixer" => ("create:mechanical_mixer".into(), info(vec![], Some((Axis::Y, false)))),
        "turntable" => ("create:turntable".into(), info(vec![Dir(Axis::Y, false)], None)),
        "speedometer" | "stressometer" => {
            // DirectionalAxisKineticBlock: its axis is one of the two off its facing.
            let along_first = powered;
            (
                format!(
                    "create:{kind}[axis_along_first={along_first},facing={}]",
                    facing.name()
                ),
                info(vec![], None),
            )
        }
        _ => unreachable!("{kind}"),
    }
}

const RELAYS: [&str; 6] = ["shaft", "shaft", "cogwheel", "large_cogwheel", "gearbox", "clutch"];
const CONSUMERS: [&str; 6] = [
    "mechanical_press",
    "encased_fan",
    "millstone",
    "turntable",
    "speedometer",
    "stressometer",
];

/// Speeds a motor is set to: the edges more often than the middle.
const SPEEDS: [i32; 16] = [256, -256, 255, -255, 1, -1, 16, -16, 32, 64, -64, 128, 200, 2, 3, -100];

/// Positions stay within this box around the origin, above the floor.
const RX: i32 = 10;
const RY: (i32, i32) = (1, 16);

fn inside(p: Pos) -> bool {
    p[0].abs() <= RX && p[2].abs() <= RX && p[1] >= RY.0 && p[1] <= RY.1
}

struct Gen {
    rng: Rng,
    blocks: Vec<Placed>,
    at: HashMap<Pos, Info>,
}

impl Gen {
    fn put(&mut self, p: Pos, (state, info): (String, Info), motor: Option<i32>) -> bool {
        if !inside(p) || self.at.contains_key(&p) {
            return false;
        }
        self.blocks.push(Placed { pos: p, state, motor });
        self.at.insert(p, info);
        true
    }

    fn motor_speed(&mut self) -> i32 {
        if self.rng.chance(0.6) {
            *self.rng.pick(&SPEEDS)
        } else {
            let s = self.rng.below(256) as i32 + 1;
            if self.rng.chance(0.5) { s } else { -s }
        }
    }

    fn random_dir(&mut self) -> Dir {
        *self.rng.pick(&DIRS)
    }

    /// A source at `p`, its shaft towards `toward`.
    fn source(&mut self, p: Pos, toward: Dir) -> bool {
        if self.rng.chance(0.25) {
            self.put(p, block("hand_crank", toward.0, toward.opposite(), false), None)
        } else {
            let speed = self.motor_speed();
            self.put(p, block("creative_motor", toward.0, toward, false), Some(speed))
        }
    }

    /// Something that takes a shaft from `from` (the face of the new block that touches it).
    fn attach_through_shaft(&mut self, p: Pos, from: Dir) -> bool {
        let a = from.0;
        let r = self.rng.below(100);
        let (kind, facing) = if r < 50 {
            let k = *self.rng.pick(&RELAYS);
            let k = if k == "clutch" && self.rng.chance(0.5) { "gearshift" } else { k };
            (k, from)
        } else if r < 85 {
            let k = *self.rng.pick(&CONSUMERS);
            (k, from.opposite())
        } else {
            // A second source on the same line: it may agree or conflict.
            return self.source(p, from);
        };
        let state = match kind {
            "gearbox" => {
                let other = *self.rng.pick(&a.others());
                block(kind, other, from, false)
            }
            "mechanical_press" => {
                if a == Axis::Y {
                    return false;
                }
                let f = if self.rng.chance(0.5) { from } else { from.opposite() };
                block(kind, a, f, false)
            }
            // Its shaft faces its facing's opposite: facing away from what drives it.
            "encased_fan" => block(kind, a, facing, false),
            // Their shaft is on their bottom: only from below.
            "millstone" | "turntable" => {
                if from != Dir(Axis::Y, false) {
                    return false;
                }
                block(kind, a, from, false)
            }
            "speedometer" | "stressometer" => {
                let f = Dir(*self.rng.pick(&a.others()), self.rng.chance(0.5));
                let along = self.rng.chance(0.5);
                block(kind, a, f, along)
            }
            _ => block(kind, a, facing, false),
        };
        self.put(p, state, None)
    }

    /// A cog meshing with the cog at `p`: small beside small, large diagonal to small, large to
    /// large at right angles; sometimes one step off.
    fn mesh(&mut self, p: Pos, axis: Axis, large: bool) -> bool {
        let [u, v] = axis.others();
        let su = if self.rng.chance(0.5) { 1 } else { -1 };
        let sv = if self.rng.chance(0.5) { 1 } else { -1 };
        let scale = |a: Axis, s: i32| {
            let o = a.unit();
            [o[0] * s, o[1] * s, o[2] * s]
        };
        let r = self.rng.below(100);
        let (offset, kind, new_axis) = if !large && r < 45 {
            let side = if self.rng.chance(0.5) { u } else { v };
            let k = if axis == Axis::Y && self.rng.chance(0.25) {
                *self.rng.pick(&["mechanical_mixer", "millstone"])
            } else {
                "cogwheel"
            };
            (scale(side, su), k, axis)
        } else if r < 80 {
            // Small and large mesh across a diagonal of the plane off their axis.
            let kind = if large { "cogwheel" } else { "large_cogwheel" };
            (add(scale(u, su), scale(v, sv)), kind, axis)
        } else if large && r < 95 {
            // Large to large at right angles: offset on both axes, none on the third.
            let other = *self.rng.pick(&[u, v]);
            let third = AXES.iter().copied().find(|a| *a != axis && *a != other).unwrap();
            let _ = third;
            (add(scale(axis, su), scale(other, sv)), "large_cogwheel", other)
        } else {
            // One step off: touches, does not mesh (or meshes wrongly).
            let d = self.random_dir();
            (d.offset(), *self.rng.pick(&["cogwheel", "large_cogwheel"]), *self.rng.pick(&AXES))
        };
        let q = add(p, offset);
        self.put(q, block(kind, new_axis, Dir(new_axis, true), false), None)
    }

    fn grow(&mut self) {
        let keys: Vec<Pos> = self.blocks.iter().map(|b| b.pos).collect();
        let p = *self.rng.pick(&keys);
        let info = self.at[&p].clone();
        let r = self.rng.below(100);
        if r < 60 && !info.shafts.is_empty() {
            let d = *self.rng.pick(&info.shafts);
            self.attach_through_shaft(add(p, d.offset()), d.opposite());
        } else if r < 88 && info.cog.is_some() {
            let (a, large) = info.cog.unwrap();
            self.mesh(p, a, large);
        } else if r < 94 {
            // Anything beside anything, any way round.
            let d = self.random_dir();
            let q = add(p, d.offset());
            let kind = *self.rng.pick(&["shaft", "cogwheel", "large_cogwheel", "gearbox", "clutch", "gearshift"]);
            let a = *self.rng.pick(&AXES);
            let state = block(kind, a, d, false);
            self.put(q, state, None);
        } else {
            let d = self.random_dir();
            self.source(add(p, d.offset()), d.opposite());
        }
    }
}

pub fn generate(seed: u64) -> Scenario {
    let mut g = Gen {
        rng: Rng::new(seed),
        blocks: Vec::new(),
        at: HashMap::new(),
    };
    let first = g.random_dir();
    g.source([0, 8, 0], first);
    let big = g.rng.chance(0.2);
    let size = 2 + g.rng.below(if big { 60 } else { 20 });
    for _ in 0..size * 3 {
        if g.blocks.len() as u32 >= size {
            break;
        }
        g.grow();
    }
    // Events come after the blocks are placed, one per tick.
    let placed = g.blocks.len() as u32;
    let ticks = placed + 40 + g.rng.below(80);
    let mut events = Vec::new();
    // Cranks are turned: held for a while, maybe let go, maybe backwards.
    for b in g.blocks.clone() {
        if g.at[&b.pos].crank {
            let until = if g.rng.chance(0.7) { ticks } else { placed + g.rng.below(ticks - placed) };
            let back = g.rng.chance(0.3);
            for t in (placed..until).step_by(4) {
                events.push(Change::turn(t, b.pos, back));
            }
        }
    }
    // Redstone beside a clutch or gearshift, placed and maybe taken away.
    let splits: Vec<Pos> = g.blocks.iter().map(|b| b.pos).filter(|p| g.at[p].redstone).collect();
    for p in splits {
        if !g.rng.chance(0.5) {
            continue;
        }
        let d = g.random_dir();
        let q = add(p, d.offset());
        if !inside(q) || g.at.contains_key(&q) {
            continue;
        }
        let on = placed + g.rng.below((ticks - placed) / 2);
        events.push(Change::set(on, q, "minecraft:redstone_block"));
        if g.rng.chance(0.5) {
            events.push(Change::set(on + 1 + g.rng.below((ticks - placed) / 2), q, "minecraft:air"));
        }
        g.at.insert(q, Info { shafts: vec![], cog: None, redstone: false, crank: false });
    }
    // A block broken mid-run.
    if g.blocks.len() > 2 && g.rng.chance(0.25) {
        let b = g.rng.pick(&g.blocks).clone();
        events.push(Change::set(placed + g.rng.below(ticks - placed - 10), b.pos, "minecraft:air"));
    }
    // A block added mid-run (grown, then moved out of the start).
    if g.rng.chance(0.25) {
        let before = g.blocks.len();
        g.grow();
        if g.blocks.len() > before {
            let b = g.blocks.pop().unwrap();
            let t = placed + g.rng.below(ticks - placed - 10);
            events.push(Change { tick: t, pos: b.pos, state: Some(b.state), motor: b.motor, turn: None });
        }
    }
    events.sort_by_key(|e| e.tick);
    Scenario {
        seed,
        ticks,
        blocks: g.blocks,
        events,
    }
}
