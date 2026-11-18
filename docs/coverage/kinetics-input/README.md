# Kinetics campaign inputs

These archived inputs reproduce the generated report independently of a running server:

```
./run.sh fuzz gaps kinetics --input docs/coverage/kinetics-input/never-reached.tsv --provenance docs/coverage/kinetics-input/campaign.json
```

`initial-corpus.txt` selects the 115 starting scenarios from `../replay-corpus/`; their names and contents reproduce the campaign's recorded corpus SHA-256. The generated reports are retained as the pre-isolation coverage baseline, not represented as coverage of the later isolation changes.

The archived core index contains 1552 remaining harness-gap branches in 774 groups. Its final two campaigns had identical core missed class/method/line/count records (`plateau-before.tsv`). The scope and every remaining explanation are in `../kinetics-core.md`.

## Checked case namespaces

The Java harness now runs cases serially, including cases submitted together. Each case retains its requested physical slot but receives a fresh network namespace and player identity. Disposal clears blocks, non-player entities across the level (including fallen/escaped items), owned scheduled ticks and block events. Two settling ticks precede checks for leaked blocks, entities, tickers or ticks. Frozen ticks, changed gamerules and failed namespace resets stop the request with an error.

Reload preserves the existing native entity ticking order. The Rust host adapter tracks and restores the same registration order; it does not alter the port engine. Network-key pruning trials were discarded, and comparisons and allowances remain unchanged.

New servers disable dynamic agent loading. The driver checks the Java source hash and boot ID, so a stale server build or a restart during a campaign fails loudly. The server owner must build and restart matching reviewed Java code; do not hot-redefine a shared test JVM.

`repeat-java 4 <seed|case>...` compares Java snapshots alone and in the original 128-case request groups, with shifted slots and requested save phases. `fuzz` checks alternate contexts even when its first Java snapshot matches the port. Repeated coverage corpus inputs also receive full Java-vs-Java comparisons.

## Verification status

The guarded private reference passed a coverage replay containing baseline reload fixtures, all eight suspicious inputs four times, and the baseline fixtures again: 42 matches and 30 matching Java-vs-Java replay comparisons on one unchanged server boot, against engine `cc43ad7`. Startup fault-injection tests rejected escaped entities, pending scheduled ticks, changed gamerules and leaked blocks before restoring the empty environment. A negative build-stamp test failed before any case executed.

Approved final validation used one unchanged private server boot after fixing unregistered actor cache disposal:

- `repeat-java 4`: all 48 comparisons matched for seeds 378, 1338, 1466, 1578, 1642, 1770, 1898 and case 2876, alone and in shifted request groups/save phases.
- `cc43ad7`, `fuzz 0 2000`: 1996 matched, four differed, zero Java-unstable. Differences: #59 (1328/1386/1402) and #60 (1256).
- `0185633`, `fuzz 0 2000`: 1992 matched, eight differed, zero Java-unstable. The four additional seeds (684/700/812/828) are #55.
- After both sweeps, 76 of 77 fixture inputs matched (all repository cases plus case 2876); only known #7 differed.
- The boot ID was identical before the repeats and after both sweeps/fixtures. Final namespace checks found zero entities, block entities, scheduled ticks and kinetic networks. The private server was then stopped.

The failed 2.5 GB attempt and the advancement-cache retention diagnosis are not counted as passing runs. Fresh-UUID actors are now constructed lazily; disposal stops advancement listeners and evicts only their own statistics/advancement cache entries. Final comparisons use the guarded build with this fix. The shared JVM was not attached to or modified during this approved validation.

Port differences belong in issues #7, #55, #59 and #60; findings files and case records are not stored here.
