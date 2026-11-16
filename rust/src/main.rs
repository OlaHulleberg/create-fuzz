mod compare;
mod engine;
mod java;
mod scenario;

use scenario::{Scenario, Snapshot};
use std::path::{Path, PathBuf};

const HELP: &str = "\
create-fuzz <command>
  rust <case.json>               run one case through the port's engine, print its snapshot
  java <case.json>               run one case on the Java server, print its snapshot
  check <case.json>...           run cases on both sides and compare them
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprint!("{HELP}");
        std::process::exit(2);
    };
    match cmd.as_str() {
        "rust" => print(&engine::run(&load(&args[1]))),
        "java" => print(&java::run(&fuzz_dir(), &[load(&args[1])]).remove(0)),
        "check" => {
            let cases: Vec<Scenario> = args[1..].iter().map(|p| load(p)).collect();
            let failed = check(&cases, &args[1..]);
            std::process::exit(i32::from(failed > 0));
        }
        _ => {
            eprint!("{HELP}");
            std::process::exit(2);
        }
    }
}

fn check(cases: &[Scenario], names: &[String]) -> usize {
    let java = java::run(&fuzz_dir(), cases);
    let mut failed = 0;
    for ((sc, j), name) in cases.iter().zip(&java).zip(names) {
        let r = engine::run(sc);
        let diffs = compare::diff(j, &r);
        if diffs.is_empty() {
            println!("{name}: match ({} blocks)", j.blocks.len());
        } else {
            failed += 1;
            println!("{name}: {} differences", diffs.len());
            for d in &diffs {
                println!("  {d}");
            }
        }
    }
    failed
}

pub fn fuzz_dir() -> PathBuf {
    std::env::var_os("FUZZ_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../work"))
}

fn load(path: &str) -> Scenario {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn print(s: &Snapshot) {
    println!("{}", serde_json::to_string_pretty(s).unwrap());
}
