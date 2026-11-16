# Findings

Each mismatch the fuzzer found, shrunk, with its status. Create: `mc1.21.1/dev` at `b62e837` (6.0.11).
Port: create-horizon, branch `fix/kinetics-breaks`.

## Fixed in the port

| Shrunk case | Java | Port before | Fix |
|---|---|---|---|
| Two small and two large cogs in a loop whose ratios disagree, nothing turning (seed 111) | nothing breaks | a cog breaks | `ab9a788`: a disagreeing loop breaks only once its side turns (`propagateNewSource` skips pairs with no speed) |
| Small cog, large cog, then a motor on the large one at 183 rpm (seed 113) | large cog breaks | small cog breaks | `b8572d6`: break the part Create's propagation stands on, replayed in its neighbour order |
| Three large cogs meshing at right angles, then a motor (seed 1141) | two cogs break, the third stops | one cog breaks | `983653b`: the replay carries on through each caller's stale neighbour list, as Create's does |
| Motor, shaft, second motor facing the first (cases/opposing-motors.json, seeds 585, 798) | both run; the second is a member and adds its capacity | second motor breaks | `9f4e29d`: a generator joining a turning network turns with it (`attachKinetics` before `initialize`) |
| Motor, gearbox, second motor, the second scrolled faster (seed 1294) | the faster one takes over | stays at 16 | `a4e3e6d`: a block entity setting changes parts in place instead of re-placing the block |

## Allow-listed (not port bugs)

- **A gearbox's own speed, sign only.** Java keeps it in its source face's frame; the port in a fixed frame.
  Every attached shaft matches in all 12 orientations (`cases/gearbox-*.json`). See `rust/src/allow.rs`.
- **A clutch's or gearshift's own speed.** Java's one block entity holds the source half's speed; the port solves
  two halves. Each face's speed is compared instead (`faces`).

## Open

- **`/setblock` of a clutch or gearshift with `powered=true` and no signal.** Java keeps it powered until a
  neighbour changes (it reads power only in `getStateForPlacement` and `neighborChanged`); the port re-reads power
  when placed. Player placement agrees on both. The generator only emits unpowered states.
- **Remaining random mismatches (25 of seeds 0-1299).** Mostly generators and split shafts with redstone, being
  shrunk.

## Harness lessons

- Place one block per tick: Create's outcome depends on which block entity ticks first.
- Never load chunks while cases run: at a sprint tick rate, block entities placed in a chunk not yet in the level
  wait 10-20 ticks to tick. All areas are force-loaded at start.
- Java's tick t is the level tick after the one it was placed in; read each case after its own last tick.
- Set a motor's speed the tick after placing it, as a player does; set at placement it starts before attaching.
