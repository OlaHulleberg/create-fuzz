//! Compares two snapshots block by block.

use crate::scenario::{Kinetic, Snapshot, parse_state};

/// Float tolerance: Create's figures are f32 sums of small products.
const EPS: f32 = 1e-3;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= EPS * a.abs().max(b.abs()).max(1.0)
}

pub fn diff(java: &Snapshot, rust: &Snapshot) -> Vec<String> {
    let mut out = Vec::new();
    let keys: std::collections::BTreeSet<&String> = java.blocks.keys().chain(rust.blocks.keys()).collect();
    for k in keys {
        match (java.blocks.get(k), rust.blocks.get(k)) {
            (Some(j), None) => out.push(format!("{k}: java has {}, rust has air", j.state)),
            (None, Some(r)) => out.push(format!("{k}: java has air, rust has {}", r.state)),
            (Some(j), Some(r)) => {
                if let Some(d) = state_diff(&j.state, &r.state) {
                    out.push(format!("{k}: state {d}"));
                }
                match (&j.kinetic, &r.kinetic) {
                    (Some(a), Some(b)) => {
                        let mut b = b.clone();
                        if crate::allow::gearbox_frame(&j.state, a, &b) || crate::allow::split_shaft_frame(a, &b) {
                            (b.speed, b.theoretical) = (a.speed, a.theoretical);
                        }
                        out.extend(kinetic_diff(a, &b).into_iter().map(|d| format!("{k}: {d}")))
                    }
                    (Some(_), None) => out.push(format!("{k}: java kinetic, rust not ({})", j.state)),
                    (None, Some(_)) => out.push(format!("{k}: rust kinetic, java not ({})", j.state)),
                    (None, None) => {}
                }
            }
            (None, None) => {}
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
    if j.overstressed != r.overstressed {
        out.push(format!("overstressed: java {} rust {}", j.overstressed, r.overstressed));
    }
    out
}
