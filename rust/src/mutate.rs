//! Bounded mutations reuse legal states from the generator and preserve the settling window.
use crate::{
    generate::{self, Rng},
    scenario::Scenario,
};
pub fn mutate(parent: &Scenario, seed: u64) -> Scenario {
    let mut rng = Rng::new(seed);
    let mut sc = parent.clone();
    sc.seed = seed;
    let donor = generate::generate(seed);
    let i = rng.below(sc.blocks.len() as u32) as usize;
    match rng.below(5) {
        0 if sc.blocks.len() > 1 => {
            let pos = sc.blocks.remove(i).pos;
            sc.events.retain(|e| e.pos != pos);
        }
        1 => {
            let mut b = rng.pick(&donor.blocks).clone();
            b.pos = sc.blocks[i].pos;
            sc.blocks[i] = b;
        }
        2 => {
            for b in donor.blocks {
                if !sc.blocks.iter().any(|old| old.pos == b.pos)
                    && !sc.events.iter().any(|e| e.pos == b.pos)
                {
                    sc.blocks.push(b);
                    break;
                }
            }
        }
        3 if !sc.events.is_empty() => {
            let j = rng.below(sc.events.len() as u32) as usize;
            sc.events[j].tick = sc.blocks.len() as u32 + rng.below(120);
        }
        _ => {
            if let Some(b) = sc.blocks.iter_mut().find(|b| b.motor.is_some()) {
                b.motor = Some(*rng.pick(&[-256, -128, -1, 1, 128, 256]));
            }
        }
    }
    sc.events.sort_by_key(|e| e.tick);
    let last = sc
        .events
        .iter()
        .map(|e| e.tick)
        .max()
        .unwrap_or(0)
        .max(sc.blocks.len() as u32);
    sc.ticks = last + generate::SETTLE;
    sc
}
#[cfg(test)]
mod tests {
    #[test]
    fn mutations_stay_bounded_and_settle() {
        for seed in 0..1000 {
            let sc = super::mutate(&crate::generate::generate(seed / 5), seed);
            assert!(!sc.blocks.is_empty());
            assert!(sc.blocks.iter().all(|b| b.pos[0].abs() <= 10
                && b.pos[2].abs() <= 10
                && (1..=16).contains(&b.pos[1])));
            assert!(
                sc.events
                    .iter()
                    .all(|e| e.tick + crate::generate::SETTLE <= sc.ticks)
            );
            assert!(sc.blocks.len() as u32 + crate::generate::SETTLE <= sc.ticks);
        }
    }
}
