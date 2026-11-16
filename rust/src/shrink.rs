//! Shrinks a mismatch one block or event group at a time to the smallest case that still
//! differs. Each round runs every one-step-smaller candidate side by side on the Java server.

use crate::scenario::Scenario;
use crate::{compare, engine, java};
use std::path::Path;

fn candidates(sc: &Scenario) -> Vec<Scenario> {
    let mut out = Vec::new();
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
    // Fewer ticks: just past the last event.
    let last = sc.events.iter().map(|e| e.tick).max().unwrap_or(0);
    if sc.ticks > last + 20 {
        let mut c = sc.clone();
        c.ticks = last + 20;
        out.push(c);
    }
    out.retain(|c| !c.blocks.is_empty());
    out
}

/// The differences a case shows now (empty when it matches).
pub fn diffs(dir: &Path, cases: &[Scenario]) -> Vec<Vec<String>> {
    let java = java::run(dir, cases);
    cases.iter().zip(&java).map(|(sc, j)| compare::diff(j, &engine::run(sc))).collect()
}

pub fn shrink(dir: &Path, mut sc: Scenario) -> (Scenario, Vec<String>) {
    let mut last = diffs(dir, std::slice::from_ref(&sc)).remove(0);
    if last.is_empty() {
        eprintln!("seed {}: matches now, nothing to shrink", sc.seed);
        return (sc, last);
    }
    loop {
        let cands = candidates(&sc);
        if cands.is_empty() {
            break;
        }
        // The Java side runs at most 128 cases at once: try them in chunks, first hit wins.
        let mut hit = None;
        for chunk in cands.chunks(128) {
            let results = diffs(dir, chunk);
            if let Some((c, d)) = chunk.iter().zip(results).find(|(_, d)| !d.is_empty()) {
                hit = Some((c.clone(), d));
                break;
            }
        }
        match hit {
            Some((c, d)) => {
                eprintln!("  {} blocks, {} events: {} differences", c.blocks.len(), c.events.len(), d.len());
                sc = c;
                last = d;
            }
            None => break,
        }
    }
    (sc, last)
}
