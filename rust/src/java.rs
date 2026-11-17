//! The Java side: batches go through the test mod's inbox and come back from its outbox.

use crate::scenario::{Scenario, Snapshot};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static SEQ: AtomicU64 = AtomicU64::new(0);

pub fn run(dir: &Path, cases: &[Scenario]) -> Vec<Snapshot> {
    exchange(dir, cases, false).0
}

pub fn covered(dir: &Path, case: &Scenario) -> (Snapshot, std::path::PathBuf) {
    let (mut snapshots, path) = exchange(dir, std::slice::from_ref(case), true);
    (snapshots.remove(0), path)
}

fn exchange(dir: &Path, cases: &[Scenario], coverage: bool) -> (Vec<Snapshot>, std::path::PathBuf) {
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
    std::fs::write(&tmp, serde_json::to_string(cases).unwrap()).expect("write inbox");
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
