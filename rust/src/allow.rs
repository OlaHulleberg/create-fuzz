//! Differences confirmed not to be port bugs, each with its evidence. The comparison skips only
//! the exact pattern described.

use crate::scenario::Kinetic;

/// A gearbox's own speed, sign only.
///
/// Java keeps a gearbox's speed in its source face's frame: `RotationPropagator.getAxisModifier`
/// returns 1 for a gearbox with no source yet, so the gearbox takes the incoming shaft's speed,
/// and its other faces turn relative to that face (`GearboxRenderer` does the same). The port
/// keeps a fixed frame with per-face signs (`Part::gearbox`'s `face_sign`). Evidence:
/// `cases/gearbox-*.json`, a motor on each of the 12 shafted faces with shafts on the other three:
/// every shaft matches; only the gearbox's own figure flips, on half the faces. Nothing reads a
/// gearbox's own speed but its own drawing.
pub fn gearbox_frame(state: &str, j: &Kinetic, r: &Kinetic) -> bool {
    state.starts_with("create:gearbox")
        && (j.speed + r.speed).abs() < 1e-3
        && (j.theoretical + r.theoretical).abs() < 1e-3
}

/// A clutch's or gearshift's own speed, when its faces are compared instead.
///
/// Create keeps one block entity for both halves, holding the speed of the half its source
/// drives (`SplitShaftBlockEntity.getRotationSpeedModifier` relative to `getSourceFacing`); the
/// port solves the halves as two parts. What a player sees is each half's shaft, so both sides
/// report each face's speed (`faces`) and those are compared; the block's own figure is not.
pub fn split_shaft_frame(j: &Kinetic, r: &Kinetic) -> bool {
    j.faces.is_some() && r.faces.is_some()
}
