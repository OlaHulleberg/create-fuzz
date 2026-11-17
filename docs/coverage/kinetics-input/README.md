# Kinetics campaign inputs

These files reproduce the generated kinetics report independently of a running server:

```
./run.sh fuzz gaps kinetics --input docs/coverage/kinetics-input/never-reached.tsv --provenance docs/coverage/kinetics-input/campaign.json
```

`initial-corpus.txt` selects the 115 starting scenarios from `../replay-corpus/`; their names and contents reproduce the campaign's recorded corpus SHA-256. Copy only those files into a fresh `FUZZ_CORPUS` before replaying `./run.sh fuzz cover 100`.

The campaign was extended in consecutive fresh 100-round runs after warming the replay corpus. The last two ended at 2230/14674 covered branch outcomes. The final extension left every core missed class/method/line/count record identical to `plateau-before.tsv`, establishing the observed core plateau. One non-core outcome was recovered during the last 100 mutations; it did not increase the final aggregate coverage.

The core scope and all 774 remaining harness-gap groups (1552 branches) are indexed in `../kinetics-core.md`. Those gaps are still open. The reason map separately identifies missing chunk lifecycle, mirror and environmental source adapters; it does not claim the remaining reachable placement, belt, lifecycle or configuration work is complete.

Validation against create-horizon `34c781e89feb33133ce44076526291a63046a892`:

- Formatting and Clippy with `-D warnings` passed; 73 Rust tests passed.
- All three `create-data` committed-asset tests passed.
- 75 of 76 case files matched. Only `actions-placement-power-gap.json` differed (known #7); `reload-kinetic.json` matched.
- `fuzz 0 2000`: 1985 matched, 13 differed, two Java-unstable. Seven differing seeds belong to known issues #55/#56; two reproducible unfiled observations and the four inconclusive valve-membership candidates are described in `../findings/kinetics-gen.md`.
- Shrinks ran under a 3,000,000,000-byte address-space cap. No engine, comparison or allowance changes were made. The shared server was used throughout; no private server was started.
