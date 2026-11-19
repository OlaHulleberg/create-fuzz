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

A partial sweep (`fuzz 0 1050`) against the private server differed on 47 seeds, and every
one of them is a wheel or boiler case that sees a reload: seeds congruent to 20 or 21
modulo 64 (the families' reload variants) or to 52 or 53 modulo 64 (the shared
`persistence::schedule` reload). No seed in the range differs for any other input the new
generators produce. `input/differing-seeds.tsv` lists them; the committed baseline listed
185 differing seeds for 0..2000 on the older driver, and those that fall in this range now
match.

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

## Belt phase campaign

`belts.md` and `input/` now come from the union of three exec sets on a private server
(`FUZZ_PORT=25630`, this branch's harness, the default include set with chutes):

1. `fuzz cover 100` over a 292-scenario corpus: 40 seeds each of the depot (1 mod 32),
   funnel (4), transport (9), slicer (22), tunnel (24) and belt surface (25) families, 30 belt
   family seeds (15 mod 16) and the manual belt, depot and spout cases. The replay and one
   guided round were measured; the second round stopped on a Java fluid-boundary error (a
   mutated water bucket use on a belt left water outside the contained region), so the
   campaign history (`seeds.tsv`, campaign 1 rows) records rounds without branch counts.
2. `fuzz cover 0` over 80 funnel and chute seeds (4 and 5 mod 32).
3. `fuzz cover 0` over the previous campaign's `replay-corpus`, so branches other families
   reach stay counted.

`report.sh` merged campaign 2's and 3's `union.exec` into campaign 1's; `campaign.json`
records the three corpora and the union. The belts subsystem now counts
`logistics/chute/` (its classes are in the include set); chutes were not counted before.

Missed branch outcomes by family (all categories / harness-gap), before (the previous
`belts.md`) and after:

| Family | Before | After |
| --- | ---: | ---: |
| funnels | 509 / 393 | 412 / 296 |
| tunnels | 277 / 263 | 103 / 89 |
| brass distribution | 390 / 372 | 339 / 321 |
| ejector | 366 / 244 | 362 / 240 |
| slicer and connector | 322 / 244 | 183 / 100 |
| depot | 240 / 164 | 178 / 102 |
| belt core | 1060 / 573 | 987 / 496 |
| chutes | not counted | 464 / 378 |
| packages | 237 / 183 | 237 / 183 |

Reclassified with the Java: the client-side outcome of side guards in `BeltSlicer`
(lines 79, 187, 196, 204, 312), `BeltBlock` (306, 348, 355) and `BeltBlockEntity.applyColor`
(275), each an `isClientSide` test whose server outcome ran; `ChuteBlockEntity.spawnParticles`
(called only under `level.isClientSide`) and `addToGoggleTooltip`; the chute renderers; and
`ChuteGenerator` (datagen). Every other new chute group is a harness gap with the remaining
work named. Harness-gap explanations of the belt, slicer, connector, funnel, tunnel and depot
classes now name what the new families leave.

### Sweep and findings

`fuzz 0 2000` on this branch (driver 0beb7d4 plus the reasons commit; private server, the
port at 4625fb5): 1878 matched, 120 differed, 1 java-unstable, 1 driver-unsupported
(`input/differing-seeds.tsv`). Every one is in a belt family: depot 5 (no item handler for a
belt segment in the port), funnel 56 (a funnel's emptied stack lingers a tick in Java), chute 22
(the chute hand-over and the full-container wait), slicer 25 (START shortening, the slicer's
kinetic detach, the inventory direction flag, the join's stale length), tunnel 8 (the wrench's
window, the cascade's tick), belt surface 4 (the direction flag). Seed 1366 (a two-motor join,
then a split) is Java-unstable: Java's saved network sizes after the slicer re-attaches its
segments depend on the save phase (phases 8 and 9 differ; the live networks agree). The
unsupported seed 1721 and the differing sideways-belt seed 921 used a water bucket on a
vertical or sideways belt; the surface family no longer does (`fuzz-lane 0 6400 25 32`
afterwards: 196 matched, 4 differed, all the direction flag). Each difference is a port
behaviour with a shrunk case for the lead; none is allowed in `allow.rs`. All 98 `cases/`
match, as before (`input/case-statuses.tsv`).

## Belt phase, second round

Branch `belts-cover2` adds three families: brass tunnel distribution (seeds 38 mod 64),
weighted ejectors (32 mod 64) and funnels pushing from depots (23 mod 64), the halves of the
geometry lanes that reload and half of the actions lane 23. `belts.md` and `input/` come from
the union of the first round's three exec sets with a fourth: `fuzz cover 0` over a 60-scenario
corpus (20 seeds of each new family, now in `replay-corpus`), private server `FUZZ_PORT=25633`,
fuzzer a002a53, the port at 4625fb5. Union: 3961/15274 branches covered (3542 before).

Missed branch outcomes by family (all categories / harness-gap), before (the first round's) and
after:

| Family | Before | After |
| --- | ---: | ---: |
| funnels | 412 / 296 | 381 / 265 |
| tunnels | 103 / 89 | 93 / 79 |
| brass distribution | 339 / 321 | 112 / 96 |
| ejector | 362 / 240 | 281 / 159 |
| slicer and connector | 183 / 100 | 183 / 100 |
| depot | 178 / 102 | 132 / 56 |
| belt core | 987 / 496 | 963 / 472 |
| chutes | 464 / 378 | 464 / 378 |
| packages | 237 / 183 | 237 / 183 |

The harness-gap reasons of the brass tunnel, ejector, depot and funnel classes now name what the
new families leave. All 98 `cases/` keep their statuses (`input/case-statuses.tsv`): the
ejector observation applies to version 3 cases only, as version 1 cases load ejectors for stress.

`fuzz-lane 0 768 <lane> 64` for each new lane (12 seeds each): brass 1 matched / 11 differed,
ejectors 0 / 12, funnel pushing 0 / 12 (two of them from a pushing shape along the belt, which
players cannot make; the generator no longer uses it). The differences are port behaviours with
shrunk cases for the lead (brass: a joining tunnel's mode, stuck items' position, the logistics
phase order, synchronize's gather tick, the item handler without distribution, the saved
distribution state, an emptied stack saved as one item; ejectors: an empty ejector taking one
item, idle wind-up, the ejector's depot never sliding, the whole-block obstacle scan; funnels: the
phase order). None is allowed in `allow.rs`.
