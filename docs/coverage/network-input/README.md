# Kinetic network campaign inputs

These inputs reproduce `../kinetics.md` and `../kinetics-core.md` without a running server:

```
./run.sh fuzz gaps kinetics --input docs/coverage/network-input/never-reached.tsv --provenance docs/coverage/network-input/campaign.json
```

The campaign replayed a 452-scenario corpus (`./run.sh fuzz cover 0`) on the shared server at the fuzzer
commit in `campaign.json`. `corpus/` holds the 158 scenarios that are not generated: cases kept by earlier
guided campaigns and the focused belt histories. `generated.txt` names the other 294, each the output of
`./run.sh fuzz gen <seed>` at that commit saved as `<name>.json`: every kinetic network history with its
variants and representatives of the transmission, valve, sequencer, chain, crank, flicker and geometry
generators. Together they reproduce the corpus SHA-256 in `campaign.json`.

An earlier guided run from the same corpus stopped at a mutated fluid fixture whose fluid reached the
scenario boundary (the Java adapter rejects such cases and `cover` stops), so the recorded campaign
replays the corpus without new mutations.
