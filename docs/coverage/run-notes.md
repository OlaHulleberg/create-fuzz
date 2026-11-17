# Coverage review status

This is an incomplete source review. The command fails deliberately: many measured
branch groups still need reviewed explanations, and the captured denominator does
not include `content/logistics/depot`. No reachable gameplay group was relabelled
out of scope. No generator gaps were closed in this change.

The first campaign ran 300 fresh scenarios, with an initially empty corpus:
265 matched, 35 differed, zero Java-unstable; 1968/12684 branch outcomes covered.
The last discovery was at round 274. A further campaign replays that corpus and
explores 50 additional mutations; its measurements are recorded in `continued/`. It found zero new branches: the
union remains 1968/12684.

The shared create-horizon main moved from 8e78a89 to f37b872 during validation.
Clippy and tests against the moved source initially failed because tick_valves now
takes a second argument. Validation was then repeated against an isolated archive
of the brief's 8e78a89, without changing either repository's fluid code.

## Findings

Seed 43 in the first campaign: Java retains motor speed 32 after the reload;
the port reports 16, with corresponding network capacity 524288 versus 262144.
The raw scenario is preserved in `findings/43.json`. Deciding Create code:
`CreativeMotorBlockEntity.addBehaviours` initializes the default speed to 16;
`ScrollValueBehaviour.read` restores `ScrollValue` from saved NBT, and
`CreativeMotorBlockEntity.getGeneratedSpeed` uses that restored value. The saved
setting, rather than the constructor default, decides the post-load speed.
This observation also occurs with unchanged main driver sources; it is not claimed
as a newly introduced or newly discovered port defect. No port changes were made.

## Validation

Python report fixture verifies exact line matching, uncovered entries, subsystem
isolation, branch versus group counts, and ambiguous-rule rejection. It passes.
Rust formatting passes. Against pinned engine 8e78a89: clippy with all targets and
warnings denied passes; all 70 tests pass. The create-data committed_assets filter
passes all three matching tests.

The existing 75 cases produce 72 matches and these three differing cases:
`actions-placement-power-gap`, `reload-kinetic`, `scope-outputs`. Driver, adapters,
case files and generator sources are byte-for-byte unchanged from e4ffcde.
The 0..2000 sweep produces 1815 matches, 185 differences, zero Java-unstable.
A second run with the unchanged main driver gives the identical 185-seed
differing set: zero newly differing seeds. These runs do not independently
classify all differences. The gaps command exits 1 as required for the remaining
unexplained groups and missing depot denominator.

## Outstanding work

Finish the source review for every UNCLASSIFIED group. Add depot to the measured
and instrumented packages before claiming coverage for that package. Extend
inventory-capability actions for simulated and real insert/extract calls; add
processing-space/funnel worlds with faithful adapters; exercise null network
lookup and partially loaded network restoration. The generated reports list the
reviewed work items with their individual branch groups.
