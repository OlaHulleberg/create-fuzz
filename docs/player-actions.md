# Version 3 player inputs

Version 1 state-injection cases remain readable. The direct-method version 2
format is rejected: its action positions were placement targets and cannot be
interpreted as clicked blocks without migration. The checked-in cases now use
version 3. Exactly one of `state`, `turn` and `action` is required per event.
Events retain array order and checkpoints observe completed ticks.

```json
{"version":3,"day_time":0,"ticks":80,
 "blocks":[{"pos":[0,7,0],"state":"minecraft:stone"}],
 "events":[
  {"tick":2,"pos":[0,7,0],"action":{"kind":"use","item":"create:shaft","face":"up","hit":[0.5,1,0.5],"yaw":90,"pitch":0,"sneaking":false}},
  {"tick":4,"pos":[0,8,0],"action":{"kind":"use","item":"create:wrench","face":"up","hit":[0.1,1,0.5],"yaw":90,"pitch":0,"sneaking":false}},
  {"tick":6,"pos":[0,9,0],"action":{"kind":"signal","strength":7}},
  {"tick":8,"pos":[0,8,0],"action":{"kind":"remove"}}
 ]}
```

`pos` is the clicked block for `use`. `hit` is a point relative to that block's
lower corner, on the named unit-cube face. Yaw/pitch are degrees. The Java adapter
sets the fake player's body/head rotation, sneak state and held item, and calls
**ServerPlayerGameMode.useItemOn**. This runs RightClickBlock, the block's item
interaction, the held item's use, and normal BlockItem placement. There are no
direct adapter calls to Create's wrench/casing/bracket methods, placement
callbacks or overridden clicked positions. A sneaking wrench use removes casing
or picks up an ordinary block through that same chain. `remove` remains a
separate game-mode block break. The fake player holds 64 items for each independent
use; inventory, drops, sounds, reach validation and client screens are not observed.

Rust mirrors the supported glue dispatch in order:

1. Interaction's RightClickBlock value-box handler. Motor geometry uses the same
   `board::motor_slot` and `board::hits_box` engine functions as the glue.
2. Non-sneaking block use-item-on handlers: bracket removal before the wrench;
   shaft encasing and shaft placement assistance (`place::pole_helper`).
3. Item use-on: bracket placement (`decor::bracket_for_click`), wrench subsystem
   overrides then default rotation (`wrench::rotated`), or block placement
   (`place::axis/facing`, neighbour shaft queries and committed world callbacks).

Wrench rotations and casing swaps call **CreateWorld::switching_block_state**
before committing, exactly as `items/wrench.rs::switch_state` does. The
`actions-wrench-network.json` regression verifies member counts 3 → 2 following
end-shaft rotation. The earlier 1511/#14 finding was a missing driver call, and
has no allowance. Network partitions and counts remain compared in full.

The supported item set is shaft, creative motor, clutch, gearshift, gearbox,
flywheel, wrench, both shaft casings and both bracket items. Shaft bracket state
is separately compared. Casing items that are not consumed by an encasing handler fall through to normal
block placement, including when sneaking. Unsupported targeted block overrides
produce explicit notes, keeping the case and comparison. Other value
boxes, cog/pipe brackets, cog casing/toggles, fluid placement, world collision,
lever/wire/conductor simulation, sequencer programs and chain-shape processing
remain outside this driver's supported transactions; the manifest names these
limits. The standalone binary cannot instantiate the SDK host glue itself.

Java Create's **FakePlayer** path deliberately skips value-box `testHit`: on any
active motor side it cancels even an off-centre click and calls onShortInteract.
The port's player glue uses the geometric hit test. Centre and off-centre motor
cases are both retained. The latter is an explicit adapter limitation and is not
allowed or disguised as a port bug. The motor centre regression confirms that the
RightClickBlock handler consumes the wrench before it can rotate the motor.

Signals use `createfuzz:signal[strength=0..15]`, whose real Java getSignal and
getDirectSignal supply the requested strength to Create's neighbour queries. Rust
uses the strongest adjacent source and passes its exact strength to
set_chain_signal. Redstone blocks supply 15. Strength sweeps remain unchanged.
Placement beside power retains the documented API_GAPS note for unavailable
placement-time redstone; no persistent kinetic difference is suppressed.

At yaw 45°, pitch 0°, Java 1.21.1's measured Mth.cos differs from the port's
26.3 double-index table and selects the other horizontal axis. `allow.rs` records
the measured bits and restricts the allowance to that isolated-shaft placement's
tracked state pair and subsequent wrench rotations. Exact angles remain generated
and mutated, raw states remain visible, and all kinetic/network/bracket figures
remain compared. `RuntimeProbe.java` writes binary measurements at server startup;
the port probe includes the MIT place.rs module in a separate ignored test binary.
