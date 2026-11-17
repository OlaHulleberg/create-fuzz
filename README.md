# create-fuzz

Differential fuzzer for [create-horizon](https://github.com/OlaHulleberg/create-horizon), the Rust port of
[Create](https://github.com/Creators-of-Create/Create). It runs the same random setups in Create's Java (on
NeoForge 1.21.1) and in the port's engine, compares what they settle to, and shrinks every difference to a minimal
case.

- `java/`: a small NeoForge test mod. One headless dedicated server runs batches of scenarios side by side.
- `rust/`: the generator, the engine driver, the comparison and the shrinker.
- `cases/`: hand-written scenarios.

Kinetics first: motors, cranks, shafts, cogs, gearboxes, clutches, gearshifts, gauges and consumers.

## Setup

Put these next to this repository:

- `../Create`: Create's `mc1.21.1/dev` branch. Build it with its own Gradle on JDK 21 and publish it locally:
  `./gradlew publishToMavenLocal`.
- `../create-horizon`: the port (the driver builds its engine, `create/src/engine`).

You also need JDK 21 and Rust 1.99.1.

## Run

```sh
./run.sh server                          # start the Java side; waits until it is ready
./run.sh fuzz fuzz 0 1000                # seeds 0-999; differences go to found/<seed>.json
./run.sh fuzz shrink found/123.json      # the smallest case that still differs: found/123.min.json
./run.sh fuzz check cases/*.json         # hand-written cases
./run.sh stop
```

To drive another checkout's engine (a worktree), set `CREATE_HORIZON=/path/to/checkout`: the driver is built
against it in its own folder (`FUZZ_BUILD`, default `/mnt/lanes/fuzz-build`), and several drivers can share one
server. `FUZZ_FOUND` sets where `fuzz` saves differences.

`./run.sh server` accepts Minecraft's EULA for its local test server (`java/run/eula.txt`).

Differences are filed as issues on [create-horizon](https://github.com/OlaHulleberg/create-horizon/issues) with
the shrunk case, not kept here.

## Scenarios

```json
{"seed": 0, "ticks": 40,
 "blocks": [{"pos": [0, 1, 0], "state": "create:creative_motor[facing=east]", "motor": 32}],
 "events": [{"tick": 20, "pos": [1, 1, 0], "state": "minecraft:redstone_block"},
            {"tick": 25, "pos": [0, 2, 0], "turn": false}]}
```

Blocks are placed one per tick, in order. `motor` sets a creative motor's speed the tick after it is placed.
Each event happens before its tick runs: `state` places a block (`minecraft:air` removes it), `turn` turns a hand
crank (true: backwards). A `minecraft:redstone_block` powers the blocks beside it.

## How the two sides are kept comparable

- Blocks are placed one per tick, as a player does: Create's outcome depends on which block entity ticks first.
- A motor's speed is set the tick after it is placed, as a player scrolls it; set at placement, Create starts it
  before it attaches.
- Java's tick t is the level tick after the one it is placed in; each case is read after its own last tick.
- Every case area is force-loaded when the server starts. At a sprint tick rate, block entities placed in a chunk
  that is still loading wait 10-20 ticks before they tick.
- A difference is rerun three times on Java first; a case Java disagrees with itself on is reported as unstable.
- `rust/src/allow.rs` lists the differences confirmed not to be port bugs, each with its evidence.
- The port's glue rules the engine lacks (a crank breaks when its support goes) are mirrored in `rust/src/engine.rs`.

## What is not here

No Minecraft, NeoForge or Create jars, no Create assets, and no world saves. Gradle downloads the dependencies.
The licence is MIT.

## Coverage guidance

```sh
CREATE_DIR=/path/to/Create FUZZ_PORT=25611 ./run.sh server
CREATE_HORIZON=/path/to/create-horizon ./run.sh fuzz cover 300
./run.sh stop
```

The server uses pinned JaCoCo 0.8.14 (downloaded into `work/jacoco`, never committed).
JaCoCo and ASM jars are checked against pinned Maven Central SHA-1 values on every use, including
cached jars; mismatches are deleted and stop the command.
`FUZZ_COVERAGE_INCLUDES` is a colon-separated JaCoCo include list; it defaults to
`com.simibubi.create.content.kinetics.*:com.simibubi.create.api.stress.*:com.simibubi.create.api.registry.*`,
including the kinetic network, torque propagator, stress values and their registry helpers. Extend it for belts or fluids. `FUZZ_CREATE_JAR` selects the matching published Create binary.
Each guided case resets the agent after area cleanup and dumps probes before publishing its snapshot.
Offline analysis unions those probes before counting branches, so discoveries on opposite edges of the
same line count separately. The agent dumps only included Create classes into `work/coverage-classes`: analysis uses these
pre-instrumentation binaries where runtime transformers changed Create, and the published jar for
classes never loaded. Class ID mismatches fail analysis.

`corpus/` (override `FUZZ_CORPUS`) keeps full scenarios that increase cumulative branch coverage. It is
ignored because it is generated, binary-version-dependent run data. Existing scenarios are replayed on
each run before new rounds; 80% of new rounds mutate a corpus member, the rest explore fresh seeds.
Mutations add/remove/replace blocks, adjust motor speeds or event ticks, keeping legal generated states,
bounded positions and a settling window. Differences and instability use the same recording path as
`fuzz` (`FUZZ_FOUND`). `work/coverage/classes.tsv` lists every included class, `never-reached.tsv` lists
class/method/line and the number of missed branch edges there, `summary.txt` totals the edges, and
`seeds.tsv` records each case's new edges. JaCoCo does not assign source-level names to individual
edges: the missed list groups edges by method and line. The union `.exec` and retained scenarios allow
further analysis; `cases/` in the report keeps every scenario and its isolated `.exec` probes. Coverage includes background execution in the measured interval; it describes the
whole server's Create kinetics, including classes unsupported by the current scenario grammar.

## Coverage contract and observations

`./run.sh fuzz manifest cases/*.json` reports all 73 kinetic ids, Cartesian state-domain
sizes, distinct generated specifications (seeds 0–1999), manual specifications and supported
interactions. Specifications may omit properties and use Java defaults; these counts are not
exhaustive state coverage. `rust/src/kinetics.json` is the explicit MIT source-derived contract.
The Java adapter audits every id/property/value against the loaded Create binary at startup,
including detecting additional kinetic registrations. The audit corrected the research table:
`rope_pulley` has only `axis`; waterlogging belongs to its nested rope/magnet blocks.
Unknown ids, properties, values and external inputs fail before either adapter runs.
Direct state placement is manual solver coverage, not player interaction coverage. Only motor
settings and hand-crank turns currently have interaction adapters. The manifest does not claim
execution of any omitted subsystem or player-reachable state closure.

Scenarios additionally accept `checkpoints: [0, 1, 2, ...]`, measured after that many completed
ticks and before the next actions. The extension is backward compatible. Checkpoints are opt-in:
transient propagation may depend on Java block-entity iteration order; the usual repeated Java
runs still classify instability. A checkpoint at `ticks` duplicates the final semantic observation.
At tick t both adapters first place initial block t, then set initial block t−1's motor, then
apply all delayed event motor settings in array order, then execute tick-t events in array order,
then run the level/engine tick. Final observations occur before any further actions. In particular,
a same-tick place followed by air removes the block. Ambiguous events and out-of-range checkpoint
schedules are rejected. Shrinking retains checkpoint history.

Snapshots include **every position**, including air, in the closed bounding box of all initial
and event positions expanded by one block on each side. The box must fit the isolated Java slot
(x/z −24..24, y 0..24). This observes neighbour-created blocks and surviving blocks outside the
submitted positions. Empty scenarios observe an empty region. Gauge comparator output is read
from the actual Java gauge and Rust gauge state; it is compared along with source presence and
sequence angle/distance effective limits. Rust part reconstruction uses saved motor settings and
actual steam output for dynamic capacity, rather than an unconditional default settings object.
Java continues to call the actual impact/capacity methods. Steam sensing/programming itself is
outside this group's action grammar and is not covered by these observations.

Both adapters record network membership as sorted position partitions and per-position
network sizes. Rust reads the selected part's actual membership through
`Kinetics::network_membership`; split shafts select the same driven half used for kinetic
figures. Raw network ids are used only to group positions within each snapshot and are never
serialized or compared. An empty network observation is emitted as empty partitions and sizes.
Dynamic settings other than the currently driven motor/crank inputs and readable steam output,
sequencer/valve programs, environment sensing, belt/conveyor links, player placement and machine
work remain outside differential coverage until their action adapters and required accessors exist.

The shrinker tries contiguous block/event chunks before individual removals so long checkpoint
histories remain practical to minimize. It still finishes with the individual candidates; reducing
case duration never drops a requested checkpoint. Submission counts in `manifest` distinguish
crank-turn and motor-setting actions from state specifications.

Static geometry now comes from each manifest entry's typed `connection` rules (`geometry.rs`):
axis selection, shaft face predicate and small/large cog classification. The Java startup audit
checks these rules against every generated block state's actual `IRotate` and `ICogWheel`
methods. Adding a block requires its manifest contract, rather than a generator kind list.

The seed schedule alternates catalogue domain enumeration with weighted crank stress boundaries
and networks grown through compatible shaft faces. Geometry cases probe all six adjacent faces
(including nonconnections), output shafts, planar and diagonal gears and perpendicular large
gears. Repeated domain passes reverse insertion order. Motors reverse after a source removal;
a settled checkpoint preserves the initial drive observation. Every eighth seed tests loads
4, 8 or 12 per source rpm against crank capacity 8, with a 2:1 cog speed ratio. These are the
adjacent load steps in that fixture. Static specifications include waterlogging; resulting Java
fluid spread is observed as a difference because the Rust adapter has no fluid world simulation.
No fluid differences are suppressed. This is state injection, not player placement coverage.

`manifest` includes an explicit static exclusion reason for each omitted id: belt/conveyor
links (group 5), environmental generators (group 6), moving/assembling machines (group 7),
and transmission settings/topology requiring group 4 adapters. Generated valve handles are
passive nodes only; inherited crank turns are never used as valve activation. Machine work,
fluid transfer, crafting, inventory, crushing entities and equipment filling remain outside the
static solver tests. The scenario JSON format and comparisons are unchanged.

Observation scope is explicit per block in the manifest's `observation_scope` map:
only its named properties are omitted by the shared comparison; raw snapshots retain
them. Display up/down and girder top/bottom belong to shape updates, chain part and
connection coordinate to ChainDriveBlock.updateShape, gantry part/carriage axis to
their updateShape methods, and large-wheel extension to LargeWaterWheelBlock.updateShape.
Placement-selected facing/axis/shaft flags remain compared: state injection supplies
those inputs, rather than invoking player placement. FluidValveBlockEntity.tick's
enabled output belongs to the fluids phase (`state_phase`); its speed, stress,
capacity, source and network observations remain compared.

Kinetics scenarios now require dry states on both adapters: waterlogged=true is
rejected explicitly, and generation enumerates only waterlogged=false. Create's
ProperWaterloggedBlock.updateWater and AbstractShaftBlock.updateShape schedule
vanilla water ticks; the standalone driver has no fluid world. The manifest retains
the full binary domain for auditing. Fluid-world tests require a separate adapter.
Network sizes read the existing TorquePropagator registry's KineticNetwork.getSize(),
without constructing networks; partitions still group block entities by network id.
