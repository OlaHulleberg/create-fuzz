//! Guided exploration orchestrates the independent collector, corpus and mutation primitives.
use crate::coverage::Collector;
use crate::{
    corpus::Corpus,
    generate::{self, Rng},
    java, mutate,
    scenario::Scenario,
};
use std::path::PathBuf;
pub fn run(rounds: u64) {
    let dir = crate::fuzz_dir();
    let report = std::env::var_os("FUZZ_COVERAGE_REPORT")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("coverage"));
    std::fs::create_dir_all(report.join("cases")).unwrap();
    let mut collector = Collector::new(&report);
    let mut corpus = Corpus::open(&PathBuf::from(
        std::env::var("FUZZ_CORPUS").unwrap_or_else(|_| "corpus".into()),
    ));
    let replay = corpus.seeds.clone();
    let mut rng = Rng::new(0);
    let mut covered = 0;
    let mut counts = crate::Counts::default();
    let mut history = String::from("round\tseed\tnew_branches\n");
    for round in 0..replay.len() as u64 + rounds {
        let replaying = round < replay.len() as u64;
        let seed = round - round.min(replay.len() as u64);
        let sc: Scenario = if replaying {
            replay[round as usize].clone()
        } else if !corpus.seeds.is_empty() && rng.chance(0.8) {
            mutate::mutate(rng.pick(&corpus.seeds), seed)
        } else {
            generate::generate(seed)
        };
        std::fs::write(
            report.join("cases").join(format!("{round}.json")),
            serde_json::to_string_pretty(&sc).unwrap(),
        )
        .unwrap();
        let (snapshot, exec) = java::covered(&dir, &sc);
        let now = collector.measure(&exec);
        history.push_str(&format!("{round}\t{}\t{}\n", sc.seed, now - covered));
        if now > covered && !replaying {
            corpus.keep(sc.clone());
        }
        covered = now;
        // Reports may be on another filesystem from the server inbox/outbox.
        std::fs::copy(&exec, report.join("cases").join(format!("{round}.exec"))).unwrap();
        std::fs::remove_file(exec).unwrap();
        crate::record(&sc, &snapshot, &mut counts);
    }
    std::fs::write(report.join("seeds.tsv"), history).unwrap();
    counts.print();
    println!(
        "{}corpus: {} scenarios; report: {}",
        std::fs::read_to_string(report.join("summary.txt")).unwrap(),
        corpus.seeds.len(),
        report.display()
    );
}
