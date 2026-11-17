# Reviewed branch explanations

`subsystems.json` defines membership by slash-separated Java package prefix. Belts
are deliberately also counted in kinetics; depot classes belong to belts. Adding
fluids requires a new membership entry and reviewed rules, without reporter changes.

`reasons.tsv` uses exact JVM class and method signatures (`*` means the whole
class), inclusive source line bounds, a category, an explanation and a SHA-256 of
the reviewed Create Java source. Explanations currently cover only the reviewed
entries in that file. Unreviewed entries remain UNCLASSIFIED and fail the command;
there is no fallback that labels reachable gameplay out of scope.

Run `./run.sh fuzz gaps [kinetics|belts] --provenance campaign.json` after a coverage
campaign. By default it reads `$FUZZ_COVERAGE_REPORT/never-reached.tsv`, or
`$FUZZ_DIR/coverage/never-reached.tsv`, and writes `docs/coverage/*.md`. The sibling
`classes.tsv` is required to detect missing package denominators. Provenance JSON
must contain `fuzzer`, `engine`, `create` (full commits) and `command`. Preserve the
input TSVs and provenance with generated reports to permit offline regeneration:

```
./run.sh fuzz gaps --input docs/coverage/input/never-reached.tsv --provenance docs/coverage/input/campaign.json
```

Missing explanations, ambiguous rules and missing package denominators fail.
Report counts are branch outcomes, not method counts or inferred edge identities.
The source hash is review evidence; refreshing reasons after a Create upgrade
requires checking their source and replacing the hash. The reporter never reads
Java sources or dependencies.
