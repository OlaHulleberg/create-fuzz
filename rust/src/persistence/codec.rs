//! MIT glue codecs, expressed over a local NBT value tree (no host SDK dependency).
use create_engine::kinetics::{Context, PartLocation, SavedKineticState, SavedNetwork};
use serde::{Deserialize, Serialize};
pub type Compound = std::collections::BTreeMap<String, Tag>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Tag {
    Byte(i8),
    Int(i32),
    Long(i64),
    Float(u32),
    IntArray(Vec<i32>),
    Compound(Compound),
    List(Vec<Tag>),
    String(String),
}
pub fn saved_to_nbt(saved: SavedKineticState) -> Compound {
    let mut out = Compound::from([
        ("Speed".into(), Tag::Float(saved.speed.to_bits())),
        (
            "NeedsSpeedUpdate".into(),
            Tag::Byte(i8::from(saved.needs_speed_update)),
        ),
    ]);
    if let Some(source) = saved.source {
        out.insert("Source".into(), Tag::IntArray(source.pos.to_vec()));
        out.insert("SourcePort".into(), Tag::Byte(source.sub as i8));
    }
    if let Some(n) = saved.network {
        out.insert(
            "Network".into(),
            Tag::Compound(Compound::from([
                ("Id".into(), Tag::Long(n.id)),
                ("Stress".into(), Tag::Float(n.stress.to_bits())),
                ("Capacity".into(), Tag::Float(n.capacity.to_bits())),
                ("Size".into(), Tag::Int(n.size as i32)),
                ("AddedStress".into(), Tag::Float(n.added_stress.to_bits())),
                (
                    "AddedCapacity".into(),
                    Tag::Float(n.added_capacity.to_bits()),
                ),
            ])),
        );
    }
    if let Some((context, relative)) = saved.sequence {
        out.insert(
            "Sequence".into(),
            Tag::Compound(Compound::from([
                ("Distance".into(), Tag::Byte(i8::from(context.distance))),
                ("Value".into(), Tag::Float(context.value.to_bits())),
                ("Run".into(), Tag::Long(context.run as i64)),
                ("RelativeValue".into(), Tag::Float(relative.to_bits())),
            ])),
        );
    }
    out
}
/// KineticBlockEntity.read; no saved data means a freshly placed entity or an older save.
pub fn saved_from_nbt(data: &Compound) -> Option<SavedKineticState> {
    fn float(c: &Compound, k: &str) -> f32 {
        match c.get(k) {
            Some(Tag::Float(v)) => f32::from_bits(*v),
            _ => 0.0,
        }
    }
    fn int(c: &Compound, k: &str) -> i32 {
        match c.get(k) {
            Some(Tag::Int(v)) => *v,
            _ => 0,
        }
    }
    let Some(Tag::Float(speed)) = data.get("Speed") else {
        return None;
    };
    let source = match data.get("Source") {
        Some(Tag::IntArray(p)) if p.len() == 3 => Some(PartLocation {
            pos: [p[0], p[1], p[2]],
            sub: match data.get("SourcePort") {
                Some(Tag::Byte(1)) => 1,
                _ => 0,
            },
        }),
        _ => None,
    };
    let network = match data.get("Network") {
        Some(Tag::Compound(n)) => match n.get("Id") {
            Some(Tag::Long(id)) => Some(SavedNetwork {
                id: *id,
                stress: float(n, "Stress"),
                capacity: float(n, "Capacity"),
                size: int(n, "Size").max(0) as u32,
                added_stress: float(n, "AddedStress"),
                added_capacity: float(n, "AddedCapacity"),
            }),
            _ => None,
        },
        _ => None,
    };
    let sequence = match data.get("Sequence") {
        Some(Tag::Compound(c)) => Some((
            Context {
                distance: matches!(c.get("Distance"), Some(Tag::Byte(1))),
                value: float(c, "Value"),
                run: match c.get("Run") {
                    Some(Tag::Long(v)) => *v as u32,
                    _ => 0,
                },
            },
            float(c, "RelativeValue"),
        )),
        _ => None,
    };
    Some(SavedKineticState {
        speed: f32::from_bits(*speed),
        source,
        network,
        sequence,
        needs_speed_update: matches!(data.get("NeedsSpeedUpdate"), Some(Tag::Byte(1))),
    })
}

pub fn belts_to_nbt(belts: &[([i32; 3], create_engine::belt::Inventory)]) -> Tag {
    Tag::List(
        belts
            .iter()
            .map(|(c, inv)| {
                let mut b = Compound::new();
                b.insert("pos".into(), Tag::IntArray(c.to_vec()));
                let numbers = inv
                    .items
                    .iter()
                    .flat_map(|i| {
                        [
                            i.position.to_bits() as i32,
                            i.side_offset.to_bits() as i32,
                            i.angle,
                            i32::from(i.count),
                            i32::from(i.locked),
                        ]
                    })
                    .collect();
                b.insert("items".into(), Tag::IntArray(numbers));
                b.insert(
                    "ids".into(),
                    Tag::List(
                        inv.items
                            .iter()
                            .map(|i| Tag::String(i.item.as_str().into()))
                            .collect(),
                    ),
                );
                b.insert(
                    "positive".into(),
                    Tag::IntArray(vec![i32::from(inv.positive)]),
                );
                // Items in a sequenced assembly: their index and step, and the recipe.
                let assembly: Vec<Tag> = inv
                    .items
                    .iter()
                    .enumerate()
                    .filter_map(|(n, i)| {
                        let p = i.progress.as_ref()?;
                        let mut a = Compound::new();
                        a.insert("at".into(), Tag::IntArray(vec![n as i32, p.step as i32]));
                        a.insert("id".into(), Tag::String(p.id.as_str().into()));
                        Some(Tag::Compound(a))
                    })
                    .collect();
                if !assembly.is_empty() {
                    b.insert("assembly".into(), Tag::List(assembly));
                }
                Tag::Compound(b)
            })
            .collect(),
    )
}

/// What `belts_to_nbt` wrote.
pub fn belts_from_nbt(data: &Compound) -> Vec<([i32; 3], create_engine::belt::Inventory)> {
    let Some(Tag::List(list)) = data.get("belts") else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|b| {
            let Tag::Compound(b) = b else { return None };
            let Some(Tag::IntArray(pos)) = b.get("pos") else {
                return None;
            };
            let Some(Tag::IntArray(numbers)) = b.get("items") else {
                return None;
            };
            let Some(Tag::List(ids)) = b.get("ids") else {
                return None;
            };
            let positive =
                matches!(b.get("positive"), Some(Tag::IntArray(v)) if v.first() == Some(&1));
            let mut items: Vec<create_engine::belt::Item> = numbers
                .as_chunks::<5>()
                .0
                .iter()
                .zip(ids)
                .filter_map(|(n, id)| {
                    let Tag::String(id) = id else { return None };
                    let mut item = create_engine::belt::Item::new(
                        id.as_str(),
                        n[3].clamp(1, 255) as u8,
                        n[2],
                        false,
                    );
                    item.position = f32::from_bits(n[0] as u32);
                    item.side_offset = f32::from_bits(n[1] as u32);
                    item.locked = n[4] != 0;
                    Some(item)
                })
                .collect();
            if let Some(Tag::List(assembly)) = b.get("assembly") {
                for a in assembly {
                    let Tag::Compound(a) = a else { continue };
                    let (Some(Tag::IntArray(at)), Some(Tag::String(id))) =
                        (a.get("at"), a.get("id"))
                    else {
                        continue;
                    };
                    let (Some(n), Some(step)) = (at.first(), at.get(1)) else {
                        continue;
                    };
                    if let Some(item) = items.get_mut(*n as usize) {
                        item.progress = Some(create_engine::sequenced::Progress {
                            id: id.to_string(),
                            step: (*step).max(0) as u32,
                        });
                    }
                }
            }
            let c = [*pos.first()?, *pos.get(1)?, *pos.get(2)?];
            Some((
                c,
                create_engine::belt::Inventory::from_saved(items, positive),
            ))
        })
        .collect()
}

use create_engine::sequencer::{self as seq, Instruction, Kind, Speed};
const INSTRUCTIONS: &str = "Instructions";
pub fn to_tag(list: &[Instruction]) -> Tag {
    Tag::List(
        list.iter()
            .map(|i| {
                Tag::Compound(Compound::from([
                    ("Type".into(), Tag::String(i.kind.name().into())),
                    ("Modifier".into(), Tag::String(i.speed.name().into())),
                    ("Value".into(), Tag::Int(i.value)),
                ]))
            })
            .collect(),
    )
}

/// `Instruction.deserializeAll`: none read gives none (the default list then).
pub fn of_tag(data: &Compound) -> Vec<Instruction> {
    let Some(Tag::List(l)) = data.get(INSTRUCTIONS) else {
        return Vec::new();
    };
    l.iter()
        .filter_map(|t| {
            let Tag::Compound(c) = t else {
                return None;
            };
            let text = |k: &str| match c.get(k) {
                Some(Tag::String(s)) => s.to_string(),
                _ => String::new(),
            };
            let value = match c.get("Value") {
                Some(Tag::Int(v)) => *v,
                _ => 0,
            };
            Some(Instruction {
                kind: Kind::of_name(&text("Type")),
                speed: Speed::of_name(&text("Modifier")),
                value,
            })
        })
        .take(seq::ROWS)
        .collect()
}

const RUN_KEYS: [&str; 5] = [
    "InstructionIndex",
    "InstructionDuration",
    "InstructionProgress",
    "Timer",
    "PrevPowered",
];

pub fn run_of_tag(data: &Compound) -> Option<seq::Run> {
    let int = |k: &str| match data.get(k) {
        Some(Tag::Int(v)) => Some(*v),
        _ => None,
    };
    let progress = match data.get(RUN_KEYS[2]) {
        Some(Tag::Float(bits)) => f32::from_bits(*bits),
        _ => 0.0,
    };
    Some(seq::Run {
        current: int(RUN_KEYS[0])?,
        duration: int(RUN_KEYS[1]).unwrap_or(-1),
        progress,
        timer: int(RUN_KEYS[3]).unwrap_or(0),
        powered_before: matches!(data.get(RUN_KEYS[4]), Some(Tag::Byte(b)) if *b != 0),
    })
}

pub fn sequencer_to_nbt(s: &seq::Sequencer) -> Compound {
    Compound::from([
        ("Instructions".into(), to_tag(&s.instructions)),
        ("InstructionIndex".into(), Tag::Int(s.current)),
        ("InstructionDuration".into(), Tag::Int(s.duration)),
        (
            "InstructionProgress".into(),
            Tag::Float(s.progress.to_bits()),
        ),
        ("Timer".into(), Tag::Int(s.timer)),
        ("PrevPowered".into(), Tag::Byte(i8::from(s.powered_before))),
    ])
}
