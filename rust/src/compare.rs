//! Compares two snapshots block by block.

use crate::scenario::{Kinetic, Snapshot, parse_state};

/// Float tolerance: Create's figures are f32 sums of small products.
const EPS: f32 = 1e-3;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= EPS * a.abs().max(b.abs()).max(1.0)
}

pub fn diff(java: &Snapshot, rust: &Snapshot) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(j), Some(r)) = (&java.networks, &rust.networks) {
        let normalize = |groups: &[Vec<String>]| {
            let mut groups = groups.to_vec();
            for g in &mut groups {
                g.sort();
            }
            groups.sort();
            groups
        };
        if normalize(&j.partitions) != normalize(&r.partitions) {
            out.push("network partitions differ".into());
        }
        if j.sizes != r.sizes {
            out.push("network sizes differ".into());
        }
    }
    let keys: std::collections::BTreeSet<&String> =
        java.blocks.keys().chain(rust.blocks.keys()).collect();
    for k in keys {
        match (java.blocks.get(k), rust.blocks.get(k)) {
            (Some(j), None) => out.push(format!("{k}: java has {}, rust has air", j.state)),
            (None, Some(r)) => out.push(format!("{k}: java has air, rust has {}", r.state)),
            (Some(j), Some(r)) => {
                if j.bracket != r.bracket {
                    out.push(format!(
                        "{k}: bracket: java {:?} rust {:?}",
                        j.bracket, r.bracket
                    ));
                }
                if let Some(d) = state_diff(&j.state, &r.state)
                    && !crate::allow::trig_state(&j.state, &r.state, r.measured_trig.as_ref())
                {
                    out.push(format!("{k}: state {d}"));
                }
                match (&j.kinetic, &r.kinetic) {
                    (Some(a), Some(b)) => {
                        let mut b = b.clone();
                        if crate::allow::gearbox_frame(&j.state, a, &b)
                            || crate::allow::split_shaft_frame(a, &b)
                        {
                            (b.speed, b.theoretical) = (a.speed, a.theoretical);
                        }
                        out.extend(kinetic_diff(a, &b).into_iter().map(|d| format!("{k}: {d}")))
                    }
                    (Some(_), None) => {
                        out.push(format!("{k}: java kinetic, rust not ({})", j.state))
                    }
                    (None, Some(_)) => {
                        out.push(format!("{k}: rust kinetic, java not ({})", j.state))
                    }
                    (None, None) => {}
                }
            }
            (None, None) => {}
        }
    }
    let ticks: std::collections::BTreeSet<_> = java
        .checkpoints
        .keys()
        .chain(rust.checkpoints.keys())
        .collect();
    for t in ticks {
        match (java.checkpoints.get(t), rust.checkpoints.get(t)) {
            (Some(j), Some(r)) => {
                out.extend(diff(j, r).into_iter().map(|d| format!("tick {t}: {d}")))
            }
            _ => out.push(format!("tick {t}: missing checkpoint")),
        }
    }
    for n in &rust.notes {
        out.push(format!("rust note: {n}"));
    }
    out
}

/// The block names differ, or a property both sides name has another value.
fn state_diff(j: &str, r: &str) -> Option<String> {
    let (jn, jp) = parse_state(j);
    let (rn, rp) = parse_state(r);
    if jn != rn {
        return Some(format!("java {j} rust {r}"));
    }
    let differing: Vec<String> = rp
        .iter()
        .filter(|(k, _)| {
            !crate::catalogue::entries()
                .iter()
                .find(|e| e.id == jn)
                .is_some_and(|e| e.observation_scope.contains_key(k))
        })
        .filter_map(|(k, v)| {
            let jv = &jp.iter().find(|(jk, _)| jk == k)?.1;
            (jv != v).then(|| format!("{k}: java {jv} rust {v}"))
        })
        .collect();
    (!differing.is_empty()).then(|| differing.join(", "))
}

fn kinetic_diff(j: &Kinetic, r: &Kinetic) -> Vec<String> {
    let mut out = Vec::new();
    let mut f = |name: &str, a: f32, b: f32| {
        if !close(a, b) {
            out.push(format!("{name}: java {a} rust {b}"));
        }
    };
    f("speed", j.speed, r.speed);
    f("theoretical", j.theoretical, r.theoretical);
    f("stress", j.stress, r.stress);
    f("capacity", j.capacity, r.capacity);
    f("impact", j.impact, r.impact);
    f("added_capacity", j.added_capacity, r.added_capacity);
    f("generated", j.generated, r.generated);
    if let (Some(jf), Some(rf)) = (&j.faces, &r.faces) {
        for (face, a) in jf {
            let b = rf.get(face).copied().unwrap_or(f32::NAN);
            if !close(*a, b) {
                out.push(format!("{face} face: java {a} rust {b}"));
            }
        }
    }
    if j.has_source != r.has_source {
        out.push(format!(
            "has_source: java {} rust {}",
            j.has_source, r.has_source
        ));
    }
    if j.comparator != r.comparator {
        out.push(format!(
            "comparator: java {:?} rust {:?}",
            j.comparator, r.comparator
        ));
    }
    for (name, a, b) in [
        ("speed_setting", j.speed_setting, r.speed_setting),
        ("sequence_angle", j.sequence_angle, r.sequence_angle),
        (
            "sequence_distance",
            j.sequence_distance,
            r.sequence_distance,
        ),
    ] {
        match (a, b) {
            (Some(a), Some(b)) if close(a, b) => {}
            (None, None) => {}
            _ => out.push(format!("{name}: java {a:?} rust {b:?}")),
        }
    }
    if j.overstressed != r.overstressed {
        out.push(format!(
            "overstressed: java {} rust {}",
            j.overstressed, r.overstressed
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::Networks;
    #[test]
    fn scope_excludes_only_declared_outputs() {
        assert!(
            state_diff(
                "create:fluid_valve[enabled=true,axis=x]",
                "create:fluid_valve[enabled=false,axis=x]"
            )
            .is_none()
        );
        assert!(
            state_diff(
                "create:fluid_valve[enabled=true,axis=x]",
                "create:fluid_valve[enabled=false,axis=y]"
            )
            .is_some()
        );
        assert!(
            state_diff(
                "create:display_board[up=true,facing=north]",
                "create:display_board[up=false,facing=south]"
            )
            .is_some()
        );
        assert!(crate::catalogue::validate("create:shaft[waterlogged=true]").is_err());
        for e in crate::catalogue::entries() {
            for (property, reason) in &e.observation_scope {
                assert!(e.properties.contains_key(property));
                assert!(!reason.is_empty());
            }
        }
    }
    #[test]
    fn partitions_compare_membership_independently_of_order() {
        let mut j = Snapshot {
            networks: Some(Networks {
                partitions: vec![vec!["a".into(), "b".into()], vec!["c".into()]],
                ..Networks::default()
            }),
            ..Snapshot::default()
        };
        let r = Snapshot {
            networks: Some(Networks {
                partitions: vec![vec!["c".into()], vec!["b".into(), "a".into()]],
                ..Networks::default()
            }),
            ..Snapshot::default()
        };
        assert!(diff(&j, &r).is_empty());
        j.networks.as_mut().unwrap().partitions =
            vec![vec!["a".into(), "c".into()], vec!["b".into()]];
        assert_eq!(diff(&j, &r), ["network partitions differ"]);
    }
    #[test]
    fn checkpoints_and_air_positions_are_compared() {
        let mut j = Snapshot::default();
        let mut r = Snapshot::default();
        j.checkpoints.insert(1, Snapshot::default());
        r.checkpoints.insert(
            1,
            Snapshot {
                blocks: [(
                    "2,8,0".into(),
                    crate::scenario::BlockSnap {
                        state: "create:shaft[axis=x]".into(),
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        );
        assert!(diff(&j, &r)[0].starts_with("tick 1: 2,8,0:"));
    }
}
