mod actions;
mod allow;
mod catalogue;
mod compare;
mod corpus;
mod coverage;
mod engine;
mod generate;
mod geometry;
mod guided;
mod handles;
mod java;
mod mutate;
mod observation;
mod program;
mod scenario;
mod shrink;
mod transmission;

use scenario::{Scenario, Snapshot};
use std::path::{Path, PathBuf};

const HELP: &str = "\
create-fuzz <command>
  rust <case.json>               run one case through the port's engine, print its snapshot
  java <case.json>               run one case on the Java server, print its snapshot
  check <case.json>...           run cases on both sides and compare them
  gen <seed>                     print the generated case for a seed
  fuzz <first-seed> <count> [batch]  generate, run and compare; mismatches go to $FUZZ_FOUND (found/)
  manifest <case.json>...        report 73-id domains and generated/manual/interaction coverage
  cover <rounds>                coverage-guided cases, corpus/ and work/coverage reports
  shrink <case.json> [witness]   shrink preserving a difference substring; writes <case>.min.json
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprint!("{HELP}");
        std::process::exit(2);
    };
    match cmd.as_str() {
        "manifest" => {
            let manual: Vec<_> = args[1..].iter().map(|p| load(p)).collect();
            let generated: Vec<_> = (0..2000).map(generate::generate).collect();
            catalogue::report(&manual, &generated);
        }
        "rust" => print(&engine::run(&load(&args[1]))),
        "java" => print(&java::run(&fuzz_dir(), &[load(&args[1])]).remove(0)),
        "gen" => println!(
            "{}",
            serde_json::to_string_pretty(&generate::generate(args[1].parse().unwrap())).unwrap()
        ),
        "cover" => guided::run(args[1].parse().unwrap()),
        "fuzz" => {
            let first: u64 = args[1].parse().unwrap();
            let count: u64 = args[2].parse().unwrap();
            let batch: usize = args.get(3).map_or(128, |b| b.parse().unwrap());
            fuzz(first, count, batch);
        }
        "shrink" => {
            let (sc, d) =
                shrink::shrink(&fuzz_dir(), load(&args[1]), args.get(2).map(String::as_str));
            let out = args[1].trim_end_matches(".json").to_string() + ".min.json";
            let record = serde_json::json!({"scenario": sc, "diffs": d});
            std::fs::write(&out, serde_json::to_string_pretty(&record).unwrap()).unwrap();
            println!("{}", serde_json::to_string_pretty(&record).unwrap());
        }
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
            for (k, b) in &j.blocks {
                if diffs.iter().any(|d| d.starts_with(&format!("{k}:"))) {
                    println!("  java {k} debug: {}", b.debug.as_deref().unwrap_or("-"));
                }
            }
        }
    }
    failed
}

/// Seeds `first..first + count`, in batches the Java side runs side by side. Each mismatch is
/// saved as `found/<seed>.json`: the case, both snapshots and the differences.
fn fuzz(first: u64, count: u64, batch: usize) {
    let seeds: Vec<u64> = (first..first + count).collect();
    let mut counts = Counts::default();
    for chunk in seeds.chunks(batch) {
        let cases: Vec<Scenario> = chunk.iter().map(|&s| generate::generate(s)).collect();
        let java = java::run(&fuzz_dir(), &cases);
        for (sc, j) in cases.iter().zip(&java) {
            record(sc, j, &mut counts);
        }
    }
    counts.print();
}

pub fn fuzz_dir() -> PathBuf {
    std::env::var_os("FUZZ_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../work"))
}

fn load(path: &str) -> Scenario {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    // A saved mismatch (`found/<seed>.json`) holds its case under `scenario`.
    if let Some(sc) = value.get_mut("scenario") {
        value = sc.take();
    }
    serde_json::from_value(value).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn print(s: &Snapshot) {
    println!("{}", serde_json::to_string_pretty(s).unwrap());
}

#[derive(Default)]
struct Counts {
    matched: usize,
    differed: usize,
    unstable: usize,
}
impl Counts {
    fn print(&self) {
        println!(
            "{} matched, {} differed, {} java-unstable",
            self.matched, self.differed, self.unstable
        );
    }
}
fn record(sc: &Scenario, j: &Snapshot, counts: &mut Counts) {
    let found = std::env::var_os("FUZZ_FOUND")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../found"));
    std::fs::create_dir_all(&found).unwrap();
    let r = engine::run(sc);
    let diffs = compare::diff(j, &r);
    if diffs.is_empty() {
        counts.matched += 1;
        return;
    }
    // Java can settle differently run to run (its networks iterate identity-hashed maps):
    // three more runs; a case Java disagrees with itself on is unstable, not a mismatch.
    let again = java::run(&fuzz_dir(), &[sc.clone(), sc.clone(), sc.clone()]);
    if again.iter().any(|a| !compare::diff(j, a).is_empty()) {
        counts.unstable += 1;
        let record = serde_json::json!({"scenario": sc, "java": j, "java_again": again});
        std::fs::write(
            found.join(format!("unstable-{}.json", sc.seed)),
            serde_json::to_string_pretty(&record).unwrap(),
        )
        .unwrap();
        println!("seed {}: java unstable", sc.seed);
        return;
    }
    counts.differed += 1;
    let record = serde_json::json!({"scenario": sc, "java": j, "rust": r, "diffs": diffs});
    std::fs::write(
        found.join(format!("{}.json", sc.seed)),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();
    println!(
        "seed {}: {} differences, first: {}",
        sc.seed,
        diffs.len(),
        diffs[0]
    );
}
