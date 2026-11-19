//! Semantic observations of the logistics blocks that meet belts: a tunnel's flaps and sides
//! (`BeltTunnelBlockEntity.updateTunnelConnections`), a funnel's transfer cooldown
//! (`FunnelBlockEntity.extractionCooldown`) and a chute's item and its position along the chute.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tunnel {
    /// Sides with a flap, by name, sorted.
    pub flaps: Vec<String>,
    /// Open sides (`sides`), by name, sorted.
    pub sides: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Funnel {
    pub cooldown: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chute {
    pub item: Option<crate::transport::Stack>,
    pub position: f32,
}

/// Direction names as Java serializes them, sorted.
pub fn names(dirs: impl IntoIterator<Item = create_engine::kinetics::Dir>) -> Vec<String> {
    let mut v: Vec<String> = dirs
        .into_iter()
        .map(|d| format!("{d:?}").to_lowercase())
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Chute positions compare within the transport tolerance.
pub fn chute_equal(a: Option<&Chute>, b: Option<&Chute>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.item == b.item && (a.position - b.position).abs() <= 1e-4,
        (None, None) => true,
        _ => false,
    }
}
