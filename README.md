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
