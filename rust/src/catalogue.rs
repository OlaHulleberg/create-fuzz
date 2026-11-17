//! Explicit Create 6.0.11 registry and Cartesian property domains.
use crate::scenario::{Scenario, parse_state};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
pub struct Entry {
    pub id: String,
    pub properties: BTreeMap<String, Vec<String>>,
    pub source: String,
    pub connection: crate::geometry::Connection,
    pub excluded: Option<String>,
}
pub fn entries() -> &'static [Entry] {
    static ENTRIES: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| {
        serde_json::from_str(include_str!("kinetics.json")).expect("kinetic manifest")
    })
}
pub fn validate(state: &str) -> Result<(), String> {
    let (id, props) = parse_state(state);
    if !id.starts_with("create:") {
        return if [
            "minecraft:air",
            "minecraft:stone",
            "minecraft:redstone_block",
        ]
        .contains(&state)
        {
            Ok(())
        } else {
            Err(format!("unrepresented external state {state}"))
        };
    }
    let entry = entries()
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown kinetic id {id}"))?;
    if state.contains('[') && !state.ends_with(']') {
        return Err(format!("malformed state {state}"));
    }
    let mut seen = BTreeSet::new();
    for (k, v) in props {
        if !seen.insert(k.clone()) || !entry.properties.get(&k).is_some_and(|vs| vs.contains(&v)) {
            return Err(format!("unrepresented state {state}: {k}={v}"));
        }
    }
    if state
        .split_once('[')
        .is_some_and(|(_, p)| p.trim_end_matches(']').split(',').any(|p| !p.contains('=')))
    {
        return Err(format!("malformed state {state}"));
    }
    Ok(())
}
pub fn scenario(sc: &Scenario) {
    sc.region();
    assert!(
        sc.checkpoints.iter().all(|t| *t <= sc.ticks),
        "checkpoint after final tick"
    );
    assert!(
        sc.events.iter().all(|e| e.tick < sc.ticks
            && usize::from(e.state.is_some()) + usize::from(e.turn.is_some()) == 1),
        "event must contain exactly one action before final tick"
    );
    for b in sc
        .blocks
        .iter()
        .cloned()
        .chain(sc.events.iter().filter_map(|e| e.placed()))
    {
        assert!(
            b.motor.is_none() || parse_state(&b.state).0 == "create:creative_motor",
            "motor action only supported for creative motors"
        );
    }
    let mut world = BTreeMap::new();
    for t in 0..sc.ticks {
        if let Some(b) = sc.blocks.get(t as usize) {
            world.insert(b.pos, parse_state(&b.state).0);
        }
        for event in sc.events.iter().filter(|e| e.tick == t) {
            if let Some(s) = &event.state {
                world.insert(event.pos, parse_state(s).0);
            }
            if event.turn.is_some() {
                assert!(
                    world
                        .get(&event.pos)
                        .is_none_or(|id| id == "create:hand_crank" || id == "minecraft:air"),
                    "turn adapter supports hand cranks only; valve activation is excluded"
                );
            }
        }
    }
    for state in sc
        .blocks
        .iter()
        .map(|b| &b.state)
        .chain(sc.events.iter().filter_map(|e| e.state.as_ref()))
    {
        validate(state).unwrap_or_else(|e| panic!("{e}"));
    }
}
/// Counts submitted states separately from crank interaction targets; never claims full domains.
pub fn report(manual: &[Scenario], generated: &[Scenario]) {
    let collect = |cases: &[Scenario]| -> BTreeMap<String, BTreeSet<String>> {
        let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for sc in cases {
            scenario(sc);
            for s in sc
                .blocks
                .iter()
                .map(|b| &b.state)
                .chain(sc.events.iter().filter_map(|e| e.state.as_ref()))
            {
                out.entry(parse_state(s).0).or_default().insert(s.clone());
            }
        }
        out
    };
    let interactions = |cases: &[Scenario]| -> (usize, usize) {
        (
            cases
                .iter()
                .map(|sc| sc.events.iter().filter(|e| e.turn.is_some()).count())
                .sum(),
            cases
                .iter()
                .map(|sc| {
                    sc.blocks.iter().filter(|b| b.motor.is_some()).count()
                        + sc.events.iter().filter(|e| e.motor.is_some()).count()
                })
                .sum(),
        )
    };
    println!(
        "interaction submissions (turn, motor): generated {:?}; manual {:?}",
        interactions(generated),
        interactions(manual)
    );
    let m = collect(manual);
    let g = collect(generated);
    println!(
        "id\tdomain states\tgenerated specifications\tmanual specifications\tinteraction support\tstatic scope"
    );
    for e in entries() {
        let domain: usize = e.properties.values().map(Vec::len).product();
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            e.id,
            domain,
            g.get(&e.id).map_or(0, BTreeSet::len),
            m.get(&e.id).map_or(0, BTreeSet::len),
            if e.id == "create:hand_crank" {
                "turn"
            } else if e.id == "create:creative_motor" {
                "motor setting"
            } else {
                "excluded: no interaction adapter"
            },
            e.excluded
                .as_deref()
                .unwrap_or("static node; machine work and player interactions excluded")
        );
        assert!(!e.source.is_empty());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract() {
        let e = entries();
        assert_eq!(e.len(), 73);
        assert_eq!(e.iter().map(|e| &e.id).collect::<BTreeSet<_>>().len(), 73);
        assert!(validate("create:shaft[axis=q]").is_err());
        assert!(validate("create:shaft[axis=x,axis=y]").is_err());
        assert!(validate("create:missing").is_err());
        assert!(validate("create:shaft[wat=true]").is_err());
        assert!(validate("create:shaft[axis=x]").is_ok());
    }
}
