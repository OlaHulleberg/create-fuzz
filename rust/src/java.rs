//! The Java side: batches go through the test mod's inbox and come back from its outbox.

use crate::scenario::{Scenario, Snapshot};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SEQ: AtomicU64 = AtomicU64::new(0);
static BOOT: OnceLock<String> = OnceLock::new();

pub fn run(dir: &Path, cases: &[Scenario]) -> Vec<Snapshot> {
    exchange(dir, cases, false, None).0
}

/// Repeat in a different slot and a separate batch, retaining the full comparison.
/// Full batches rotate; smaller ones leave empty slots ahead of their real cases.
pub fn varied(dir: &Path, cases: &[Scenario], offset: usize, phase: u8) -> Vec<Snapshot> {
    assert!(!cases.is_empty() && cases.len() <= 128 && offset > 0);
    if cases.len() + offset <= 128 {
        let empty = Scenario {
            version: 3,
            seed: 0,
            day_time: cases[0].day_time,
            ticks: 1,
            checkpoints: vec![],
            blocks: vec![],
            events: vec![],
        };
        let mut shifted = vec![empty; offset];
        shifted.extend_from_slice(cases);
        return exchange(dir, &shifted, false, Some(phase))
            .0
            .into_iter()
            .skip(offset)
            .collect();
    }
    let rotation = offset % cases.len();
    let mut shifted = cases.to_vec();
    shifted.rotate_right(rotation);
    let mut snapshots = exchange(dir, &shifted, false, Some(phase)).0;
    snapshots.rotate_left(rotation);
    snapshots
}

pub fn covered(dir: &Path, case: &Scenario) -> (Snapshot, std::path::PathBuf) {
    let (mut snapshots, path) = exchange(dir, std::slice::from_ref(case), true, None);
    (snapshots.remove(0), path)
}

fn exchange(
    dir: &Path,
    cases: &[Scenario],
    coverage: bool,
    phase: Option<u8>,
) -> (Vec<Snapshot>, std::path::PathBuf) {
    for sc in cases {
        crate::catalogue::scenario(sc);
    }
    let runtime = dir.join("harness-runtime.json");
    let stamp: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&runtime)
        .unwrap_or_else(|_| panic!("missing Java runtime stamp at {}; the server owner must load the matching reviewed harness build", runtime.display())))
        .expect("Java runtime stamp");
    assert_eq!(
        stamp["protocol"].as_u64(),
        Some(1),
        "unsupported Java harness protocol"
    );
    let expected = std::env::var("FUZZ_HARNESS_HASH").expect("use run.sh");
    assert_eq!(
        stamp["source_hash"].as_str(),
        Some(expected.as_str()),
        "Java harness source mismatch: refuse comparisons against a different server build"
    );
    let boot = stamp["boot"].as_str().expect("Java boot id").to_owned();
    assert_eq!(
        BOOT.get_or_init(|| boot.clone()),
        &boot,
        "Java server restarted during this run; discard the mixed-runtime campaign"
    );
    let name = format!(
        "{}{}-{}-{}.json",
        if coverage { "cover-" } else { "" },
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let inbox = dir.join("inbox");
    let tmp = inbox.join(format!("{name}.tmp"));
    let mut wire = serde_json::to_value(cases).unwrap();
    if let Some(phase) = phase {
        assert!(phase < 10);
        for case in wire.as_array_mut().unwrap() {
            case["_start_phase"] = phase.into();
        }
    }
    std::fs::write(&tmp, serde_json::to_string(&wire).unwrap()).expect("write inbox");
    std::fs::rename(&tmp, inbox.join(&name)).expect("move into inbox");
    let out = dir.join("outbox").join(&name);
    let start = Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(&out) {
            let _ = std::fs::remove_file(&out);
            let value: serde_json::Value = serde_json::from_str(&text).expect("outbox json");
            if let Some(e) = value.get("error") {
                panic!("java side: {e}");
            }
            let after: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(&runtime).expect("runtime stamp after response"),
            )
            .expect("runtime stamp");
            assert_eq!(
                after["boot"], stamp["boot"],
                "Java restarted while handling the request"
            );
            assert_eq!(
                after["source_hash"], stamp["source_hash"],
                "Java build changed during the request"
            );
            return (
                serde_json::from_value(value).expect("snapshots"),
                dir.join("outbox").join(format!("{name}.exec")),
            );
        }
        if let Ok(pid) = std::fs::read_to_string(dir.join("server.pid"))
            && std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()))
                .map_or(true, |st| st.split_whitespace().nth(2) == Some("Z"))
        {
            panic!(
                "the Java server exited: see {}",
                dir.join("server.log").display()
            );
        }
        if start.elapsed() > Duration::from_secs(600) {
            panic!("no answer from the Java side in 10 min: is `./run.sh server` running?");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
