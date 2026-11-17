# Version 2 player inputs

Legacy scenarios default to version 1. Version 2 adds an `action` to an event;
exactly one of state, turn and action is required. Actions execute in array order
in the existing before-tick event phase. Checkpoints and shrinking retain them.

```json
{"version":2,"ticks":80,"blocks":[],"events":[
 {"tick":0,"pos":[0,8,0],"action":{"kind":"place","block":"create:shaft","yaw":90,"pitch":0,"face":"up","sneaking":false}},
 {"tick":2,"pos":[0,9,0],"action":{"kind":"signal","strength":7}},
 {"tick":4,"pos":[0,8,0],"action":{"kind":"wrench","face":"up"}},
 {"tick":6,"pos":[0,8,0],"action":{"kind":"remove"}}
]}
```

Placement uses Create getStateForPlacement and setPlacedBy with a player and
face-centred hit. The target is explicitly selected; item placement helpers,
reach checks and replacement of non-air blocks are outside this adapter.
Rust calls the same place::axis/facing and world::has_shaft_towards entry points
as glue/logistics/placement.rs. The supported closed subset is shaft, creative
motor, clutch, gearshift, gearbox and flywheel. Dry context only. Wrench uses
Create IWrenchable.onWrenched and Rust wrench::rotated followed by block_changed.
The subset has no special rotation override requiring other glue entry points.
Removal uses the fake player's gameMode.destroyBlock; Rust commits air through
block_changed and neighbour notifications. Drops/inventory are not observed.

Signal places a registered test-only createfuzz:signal[strength=0..15] block.
Its real Java getSignal/getDirectSignal return that strength to Create's
hasNeighborSignal/getBestNeighborSignal. Rust takes the maximum adjacent input,
and forwards its exact value to set_chain_signal. Existing redstone blocks still
provide 15. This is a controlled external source, not a vanilla wire simulation.
Lever support, weak conduction through other blocks and wire propagation are
not reproduced. Use isolated adjacent source fixtures.

Remaining group 3 work: casing application/removal, bracket application/removal
and bracket observations, sneak wrench, encased cog shaft toggles, other placement
overrides and arbitrary hit positions, sequencer redstone clocking/ticks, chain
shape application. Their port implementations live in private game glue; this
engine-only binary does not invoke them. They are explicitly excluded here,
not established API gaps. Unsupported action kinds and placements fail validation.

The documented API_GAPS redstone-in-state_for_placement discrepancy is retained:
Rust places a clutch/gearshift unpowered and corrects it through the normal tick
input. Cases beside power retain notes and comparisons; no comparator was relaxed.
