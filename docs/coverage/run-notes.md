# Kinetics and belt coverage review

The source review covers every missed group in both package-defined subsystems.
`fuzz gaps` succeeds with zero UNCLASSIFIED groups and no missing denominator
packages. Belts include depot, funnel, tunnel and package transport code as well
as `content.kinetics.belt`. Belt classes are also part of the kinetics report.

The reason map pins Create b62e837. Every rule has a SHA-256 of its owning Java
source, checked during review. Whole classes describe uniform client, datagen or
contraption behavior. Mixed classes use method and narrow line overrides. A line
can include several outcomes: descriptions explicitly identify mixed gameplay and
defensive/dead edges rather than calling the entire group unreachable. Static
machine connection states already exist in the generator; missing processing and
entity workflows remain harness work items.

## Campaign and reproducibility

The campaign used this worktree's dedicated server on port 25621, with its world,
Java build, Rust build, corpus, found cases and JaCoCo tools under
`/mnt/lanes/agent-gaps`. The shared server on 25599 was never restarted or stopped.
The engine was an immutable source archive of the brief's 8e78a89 because the
shared create-horizon checkout has since advanced. No port changes were made.

A fresh 300-round campaign was followed by 100 additional mutations (replaying
66 saved cases), then the final 100 mutations with 72 replay cases, including the
two focused input histories. The final union is 2107/14674 branch outcomes covered.
All 100 final mutations added zero branches; the final report includes the
additional depot rejection branch from the focused replay. JaCoCo analyzes loaded
transformed Create classes; its denominator can change as newly loaded classes
replace jar-only class definitions, so counts use the final captured denominator.

`input/campaign.json` contains the source commits, includes, initial corpus hash,
completion marker and input hash. `replay-corpus/` preserves the exact initial 72 files.
The Java harness, Create binary and initial server configuration must match.
With the isolated server ready and an engine source archive at the pinned commit,
copy `replay-corpus/` to a disposable corpus directory and run:

```
CREATE_DIR=/root/Create CREATE_HORIZON=/mnt/lanes/agent-gaps/engine FUZZ_ENGINE_REVISION=8e78a89be9f660b3dfef59f9620b73859f910921 FUZZ_PORT=25621 FUZZ_DIR=/mnt/lanes/agent-gaps/work FUZZ_BUILD=/mnt/lanes/agent-gaps/build FUZZ_FOUND=/mnt/lanes/agent-gaps/found CARGO_TARGET_DIR=/mnt/lanes/agent-gaps/target CARGO_BUILD_JOBS=4 FUZZ_COVERAGE_CACHE=/mnt/lanes/agent-gaps/jacoco FUZZ_CORPUS=/mnt/lanes/agent-gaps/replay ./run.sh fuzz cover 100
```

For offline report regeneration from committed measurements:

```
./run.sh fuzz gaps --input docs/coverage/input/never-reached.tsv
```

## Closed inexpensive inputs

- Depot use from each side face, with an offered stack and an empty hand, before
  valid top insertion. `replay-corpus/zz-depot-side.json` matches Java and the port and
  covers both outcomes of SharedDepotBlockMethods.onUse's face rejection guard.
- Cancel a selected connector endpoint after its five-tick cooldown, then perform
  the ordinary two-click connection. `replay-corpus/zz-cancel.json` matches Java and the
  port. This makes the cancellation history deterministic in coverage replay;
  earlier guided mutations had already reached the sneaking branch. The remaining
  missed outcome on that line is the defensive null-player context.

Each generator extension has its own signed commit. Existing case files were not
changed. No new kinetics/belt discrepancy was isolated by these added histories;
known baseline differences are not re-reported as new findings.

## Water wheel and steam engine sources

The water wheel family (seeds congruent to 20 modulo 32) and the steam engine family
(seeds congruent to 21 modulo 32) close the generator side of those sources:

- Water wheel (`waterwheel.rs`): a small wheel in a contained falling-water column, both
  flow signs, two columns cancelling, a lava column (the lava advancement input), the
  plank material through player uses (a non-block item, the wheel's own spruce, an invalid
  block item and a valid plank), player placement along and across an existing wheel's axis
  (accepted and refused by `canSurvive`), a stone blocked above the wheel so a
  non-consuming use cannot place, and a reload. Large wheels add the room check, the
  `LargeWaterWheelBlockItem` retry at the clicked face, an extension train, the eight
  controller cells, material through a cell, a sneak wrench that redirects to the wheel,
  a broken cell and a cell obstructed before the wheel's own tick.
- Steam engine (`steam_engine.rs`): a tank with active heat (a blaze burner fed a blaze
  cake) or passive heat (magma, a lit or unlit campfire), a creative actor's water buckets
  at two supply-sample phases, floor/wall engines with a converted shaft and an invalid
  same-axis shaft, a boxed-in waterlogged engine (its source would otherwise spread),
  a second boiler whose shaft is placed through the engine's placement helper and then
  removed with a sneak wrench, an engine over stone (refused), an engine removed and a tank
  block removed.
- Driver support: `FluidHost` answers `FluidState.getFlow` for the wheel's sample cells,
  the driver runs the large wheel's scheduled frame reconstruction and the structural
  cells' own ticks/removals, counts engines per boiler, reads the heat under the tank,
  samples the water supply, converts shafts to powered shafts and back, ticks blaze burners
  and restores a wheel's flow score on reload. Waterlogged engines are boxed in so the
  native fluid cannot reach the fixture boundary.

The measurement is the union of two campaigns: `./run.sh fuzz cover 120` over the shared
corpus (77 scenarios) and `./run.sh fuzz cover 1` replayed from 62 generated
water-wheel/steam cases. JaCoCo unions both exec sets offline
(`tools/coverage/report.sh` with campaign 1's union as the merged input and campaign 2's
union as the new data). The final union is 2834/14674 branch outcomes covered;
`input/campaign.json` records both commands, the corpus hash of the second campaign and
the union input hash. The class-level distribution moves slightly between runs because a
class loaded in one campaign is analyzed from its transformed binary, not the jar.

Core harness-gap groups in these areas: steamEngine 47 groups/121 branches became
44/65; waterwheel 63/138 became 32/51 (the core total went from 1552 to 1483). The
remaining groups are listed in `kinetics-core.md`; every one still has a reviewed
explanation (no `out-of-scope:environment-source-adapter` rule remains, because the
adapters now exist).

### Differences found by the new inputs

All are after a reload, and all are Java-versus-port:

- A flowing water wheel reloaded by `Action::Reload`: the port reports a network size of
  2 while Java keeps 1 (`cases/`-style probe: wheel, contained column, reload).
- A running boiler reloaded: the port stops driving every powered shaft (generated 64 ->
  0) while Java keeps them at 64; `probe-steam-reload` is an 11-block case.
- A large wheel whose frame is rebuilt around a reload: one frame cell differs for a tick
  (seed 436 is a record of this).

These are the only seeds that differ in the 0..700 seed sweep run against the private
server: every seed congruent to 20 or 21 modulo 64 (the families' reload variants).

## Open work items

The generated reports contain every remaining harness-gap explanation and the
exact class/method/line groups it applies to. The main implementation areas are:

- Real and simulated inventory insertion/extraction, side inputs, full output
  buffers and partial acceptance on belts, depots, funnels and tunnels.
- Press/deployer/fan processing over belt/depot items; recipe matches, processing
  locks, held/multiple outputs and interruptions.
- Funnel count modes/filters/power and brass-tunnel groups, routing and distribution.
- Package stacks/entities, weighted-ejector targets and launches, chain package
  routing, player riding and their server packet actions.
- Player placement/wrench/casing/dye/material changes across all axes and faces,
  including constrained placement and survival inventory costs.
- Crafter grids/connected inputs, millstone/mixer/crusher/saw recipes, drill cobble
  collection, trees/crops, deployer tools/combat, arm targets and fan catalysts.
- Unloaded/stale sources, partially loaded network restoration, competing sources,
  stress/capacity transitions, frozen ticks and changed server configuration.
- Entity collision/passenger histories, clock animation time/dimension windows,
  packet configuration and startup probe collection before per-case reset.

## Checks

- Rust fmt --check and clippy --all-targets -- -D warnings pass on pinned 8e78a89.
- All 70 Rust tests pass; all three reporter tests pass, including missing-package,
  incomplete-campaign, stale-input and ambiguous-rule rejection.
- All three committed_assets tests in create-data pass.
- All 75 existing case statuses match the baseline: 72 matches and three existing
  differing cases (listed in input/case-statuses.tsv).
- The 0..2000 sweep matches the baseline seed set exactly: 1815 matched, 185 differed,
  zero Java-unstable, zero newly differing seeds. The baseline driver Rust sources
  were identical to e4ffcde before the two generator extensions.
- Both focused new input histories match Java and the port.
- Both generated gap reports pass; bash syntax and git diff whitespace checks pass.

The isolated server is stopped and temporary build/sweep outputs are removed after
saving these measurements. The shared server and port source remain untouched.
