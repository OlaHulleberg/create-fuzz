# Player items and transport (version 3)

An `item` action has a typed `input`. Plain stacks have `item` and `count`;
`transport.json` lists supported items, stack limits and the dry depot block.
The Java server audits this contract against the running binary at startup.
Unknown items, oversized input stacks and unsupported transport-world machines
fail validation before either adapter runs.

```json
{"tick":40,"pos":[0,8,1],"action":{"kind":"item","input":{
  "kind":"drop_on","stack":{"item":"minecraft:iron_ingot","count":16}}}}
```

`drop_on` is a **controlled landing input**, not a simulation of vanilla flight.
Java calls the ordinary actor's `Player.drop`, positions the resulting item entity
at the segment's landing point with zero vertical motion, and invokes the block's
`updateEntityAfterFallOn`. Create's `BeltBlock.entityInside` then performs its
normal capability insertion. A depot invokes `SharedDepotBlockMethods.onLanded`.
No adapter inserts directly into a Java item handler or constructs a transported
stack. The unaccepted entity remainder is recorded and removed from the flight
world, so this action represents exactly one landing, not repeated retries.
Rust follows `logistics::item_landed` and `belt::put_on_from_above`, including the
transportable-slope check, stack limit, insertion-side occupancy, stored direction
bit and deferred inventory insertion. The player drop/flight/collision machinery
outside this boundary is not compared. Rejected drops remain visible in outcomes.

```json
{"tick":160,"pos":[0,8,5],"action":{"kind":"item","input":{
  "kind":"use","stack":null,"face":"up","yaw":0,"sneaking":false}}}
```

`use` calls `ServerPlayerGameMode.useItemOn` with the supplied stack (or empty
hand), face, yaw and sneak state. On a belt, empty main hand takes stacks within
0.55 of the segment centre; a nonempty ordinary ingot use does not place it on
the belt. On a depot, top-face use takes the held stack and puts down the offered
stack. Rust mirrors `belt::use_belt` and `depot::use_depot`; it calls `depot_put`
for entity-owned motion. Existing ordinary empty-hand `use` actions on these
blocks use the same adapter. A non-top depot click passes through.

Snapshots contain cumulative `item_outcomes`: position, offered stack, remainder
and stacks taken. Java measures actual actor inventory changes, treating items
returned to an initially empty selected hand as pickup, rather than an offered
remainder. Counts are aggregated by item id; slot layout is not compared.

`transport` appears on belt controllers and depots, including empty inventories.
Each item records its plain stack, longitudinal position, lock and pending state.
Active belt items also report whether the last tick left their position unchanged
(or the belt is stopped); pending items and depot items use null for that field.
Stack identity/count, presence, pending/lock/stall and positions are compared.
Items are sorted by position, id and count. Positions use the existing numeric
tolerance. Random render angles and their lateral jitter are not longitudinal
transport observations and are not synchronized.

Java observes Create's live queues using reflection confined to MIT classes:
BeltInventory.toInsert/toRemove and DepotBehaviour.heldItem. It does not call
BeltInventory.write, which flushes queues and would change the state being
observed. Belt debug NBT is therefore omitted; all semantic belt fields remain.
Rust reads live items and a cloned `to_save` view to identify pending items.
Repeated snapshots do not flush the engine's queues.

The logistics phase ticks depot sliding, then takes `busy_belts` once, ticks each
inventory using its actual solved speed and surroundings, and preserves items
added during transfers. It runs before the kinetic solve, as the glue does.
Surroundings implement ordinary stone/redstone solid endings and direct belt or
depot input. Belt insertion preserves from-belt offsets, side-entry rules,
occupancy, stopped/opposite-direction refusal and locks. Occupied depots refuse
insertion. Empty receiving belts follow the glue's busy snapshot, including any
observable delay; the driver does not tick extra empty belts to match Java.

The family uses player-connected belts with blocked ends, depots and a second
belt. It covers accepted/refused landings, multiple queued stacks, both motor
signs, reversal, source removal/replacement, stopped/blocked stalls, empty-hand
pickup, depot placement/swap/pickup, and per-tick histories through movement and
handoff. Mutation changes offered counts within the audited limits and retains
the existing interaction mutations. Manual cases isolate depot landings and
same-item swaps. Loaded-belt slicing, pulleys/casing, processing machines,
funnels/tunnels, fluids, item components/packages, depot output buffers and
native ejected-entity flight require further adapters. They are not claimed as
item transport coverage. Unsupported machine worlds/structural use actions are
rejected explicitly; generated ends do not eject into unmodelled native flight.
