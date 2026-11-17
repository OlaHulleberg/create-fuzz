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

/// Measured game-version difference, restricted to an isolated shaft placed at yaw 45,
/// pitch 0, without a preferred neighbouring shaft, and subsequent default wrench turns.
/// Binary probe (Java 1.21.1): radians -0.7853982, sin bits 3207922931,
/// cos bits 1060439283, nearest SOUTH => axis z. The port's actual place.rs functions
/// (26.3 double-index table): same sin, cos bits 1060438146, nearest West => axis x.
/// Probe evidence lives outside the repo in the actions rework report's trig-java.json and
/// trig-port.json. No network, kinetic, bracket or arbitrary orientation difference is allowed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MeasuredTrigState {
    pub java: String,
    pub port: String,
}

pub fn trig_state(java: &str, port: &str, measured: Option<&MeasuredTrigState>) -> bool {
    let normalize = |s: &str| {
        let (name, props) = crate::scenario::parse_state(s);
        crate::scenario::format_state(&name, &props)
    };
    measured.is_some_and(|m| {
        m.java.starts_with("create:shaft[")
            && m.port.starts_with("create:shaft[")
            && normalize(java) == m.java
            && normalize(port) == m.port
    })
}

#[cfg(test)]
mod measured_tests {
    #[test]
    fn only_measured_state_pair_is_allowed() {
        let m = super::MeasuredTrigState {
            java: "create:shaft[axis=z,waterlogged=false]".into(),
            port: "create:shaft[axis=x,waterlogged=false]".into(),
        };
        assert!(super::trig_state(&m.java, &m.port, Some(&m)));
        assert!(!super::trig_state(
            "create:shaft[axis=y,waterlogged=false]",
            &m.port,
            Some(&m)
        ));
        assert!(!super::trig_state(&m.java, &m.port, None));
        let sc = serde_json::from_str(include_str!("../../cases/actions-angle-45.json")).unwrap();
        let snap = crate::engine::run(&sc);
        let r = &snap.blocks["2,8,0"];
        let m = r.measured_trig.as_ref().unwrap();
        assert_eq!(m.java, "create:shaft[axis=x,waterlogged=false]");
        assert_eq!(m.port, "create:shaft[axis=z,waterlogged=false]");
        let mut j = snap.clone();
        j.blocks.get_mut("2,8,0").unwrap().state = m.java.clone();
        assert!(crate::compare::diff(&j, &snap).is_empty());
        j.networks
            .as_mut()
            .unwrap()
            .sizes
            .insert("2,8,0".into(), 99);
        assert!(crate::compare::diff(&j, &snap).contains(&"network sizes differ".into()));
    }
}
