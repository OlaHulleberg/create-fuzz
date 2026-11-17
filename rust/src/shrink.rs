//! Shrinks a mismatch one block or event group at a time to the smallest case that still
//! differs. Each round runs every one-step-smaller candidate side by side on the Java server.

use crate::scenario::Scenario;
use crate::{compare, engine, java};
use std::path::Path;

fn candidates(sc: &Scenario) -> Vec<Scenario> {
    let mut out = Vec::new();
    // Coarse reductions first: traces often contain many repeated uses. Keep the same
    // ordering and checkpoint schedule while removing contiguous chunks, then prove
    // one-step minimality with the single-action candidates below.
    for events in [false, true] {
        let len = if events {
            sc.events.len()
        } else {
            sc.blocks.len()
        };
        let mut width = len / 2;
        while width > 1 {
            for start in (0..len).step_by(width) {
                let mut c = sc.clone();
                let end = (start + width).min(len);
                if events {
                    c.events.drain(start..end);
                } else {
                    let removed: Vec<_> = c.blocks.drain(start..end).map(|b| b.pos).collect();
                    c.events.retain(|e| !removed.contains(&e.pos));
                }
                out.push(c);
            }
            width /= 2;
        }
    }
    // Each block, with every event at its position.
    for i in 0..sc.blocks.len() {
        let mut c = sc.clone();
        let p = c.blocks.remove(i).pos;
        c.events.retain(|e| e.pos != p);
        out.push(c);
    }
    // Every event at one position (a crank's turns, a redstone block placed and taken away).
    let mut positions: Vec<[i32; 3]> = sc.events.iter().map(|e| e.pos).collect();
    positions.dedup();
    for p in positions {
        let mut c = sc.clone();
        c.events.retain(|e| e.pos != p);
        out.push(c);
    }
    // Single events.
    for i in 0..sc.events.len() {
        let mut c = sc.clone();
        c.events.remove(i);
        out.push(c);
    }
    // Fewer ticks: settled past the last change (blocks are placed one per tick).
    let last = sc
        .events
        .iter()
        .map(|e| e.tick)
        .max()
        .unwrap_or(0)
        .max(sc.blocks.len() as u32);
    let settled = last + crate::generate::SETTLE;
    if sc.ticks > settled {
        let mut c = sc.clone();
        c.ticks = settled.max(c.checkpoints.iter().copied().max().unwrap_or(0));
        out.push(c);
    }
    out.retain(|c| !c.blocks.is_empty() && c != sc);
    out
}

/// The differences a case shows now (empty when it matches).
pub fn diffs(dir: &Path, cases: &[Scenario]) -> Vec<Vec<String>> {
    let java = java::run(dir, cases);
    cases
        .iter()
        .zip(&java)
        .map(|(sc, j)| compare::diff(j, &engine::run_at_phase(sc, j.save_phase)))
        .collect()
}

pub fn shrink(dir: &Path, mut sc: Scenario, required: Option<&str>) -> (Scenario, Vec<String>) {
    let mut last = diffs(dir, std::slice::from_ref(&sc)).remove(0);
    if last.is_empty() {
        eprintln!("seed {}: matches now, nothing to shrink", sc.seed);
        return (sc, last);
    }
    // Keep a witness from the original observation: reduction must not replace a
    // network finding with an unrelated state-injection discrepancy.
    let witness = last.clone();
    let required_witness: Vec<_> = witness
        .iter()
        .filter(|d| required.is_none_or(|text| d.contains(text)))
        .cloned()
        .collect();
    assert!(
        !required_witness.is_empty(),
        "requested shrink witness is absent"
    );
    loop {
        let cands = candidates(&sc);
        if cands.is_empty() {
            break;
        }
        // Small batches avoid constructing and clearing many areas after an early coarse hit.
        // The Java protocol permits up to 128, but shrinking usually accepts an early candidate.
        let mut hit = None;
        for chunk in cands.chunks(16) {
            let results = diffs(dir, chunk);
            if let Some((c, d)) = chunk.iter().zip(results).find(|(_, d)| {
                !d.is_empty()
                    && d.iter().all(|item| witness.contains(item))
                    && d.iter().any(|item| required_witness.contains(item))
            }) {
                hit = Some((c.clone(), d));
                break;
            }
        }
        match hit {
            Some((c, d)) => {
                eprintln!(
                    "  {} blocks, {} events: {} differences",
                    c.blocks.len(),
                    c.events.len(),
                    d.len()
                );
                sc = c;
                last = d;
            }
            None => break,
        }
    }
    (sc, last)
}
