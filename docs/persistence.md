# Persistence observations (incomplete)

Snapshots now compare a typed subset of the actual server save:
Speed, NeedsSpeedUpdate, Source (relative to the case origin), and Network's
Stress, Capacity, Size, AddedStress and AddedCapacity. Java calls
saveWithoutMetadata with the registry provider; Rust calls the same saved_state
entry that glue/kinetics.rs serializes through saved_to_nbt. Missing optional
numeric tags normalize to zero. Network Id is deliberately never emitted.
Floating values use the kinetic comparison tolerance. Sequence and all other
subsystem save data remain outside this new observation.

This change does **not** implement save/reload actions. It cannot establish
reload parity. No generated scenario reloads, and no reload API is advertised.

A faithful reload must recreate block entities through their server load path,
not restore their visible snapshot into existing objects. SmartBlockEntity's
chunk unload path differs from destruction: onChunkUnloaded sets chunkUnloaded,
and setRemoved consequently skips remove. KineticBlockEntity.initialize restores
network unloaded figures with initFromTE/addSilently; calling remove would
propagate destruction instead. A replacement adapter must account for the
network registry and the phase of the new entity's first tick.

The Rust counterpart must restore column attachments (motor/controller settings,
belt contents and depot state) before block_loaded and restore_state, then invoke
the sequencer load path. The old-save migration path is separate from a current
save round trip. Retaining the current World or cloning driver caches across a
reload would conceal missing persistence, particularly crank expiry, valve use,
sequencer instruction progress, belt controller/inventory and depot buffers.
These reload paths are outstanding, not unsupported engine claims.

Version 3 `{"kind":"reload"}` is an area action; its event position participates
in the normal region bounds but does not select a single entity. Java saves all
entities in that region before removing any, calls onChunkUnloaded before
removeBlockEntity (which marks the old entity removed), and installs fresh
loadStatic entities with unchanged block states. Initialization runs on their
next normal entity tick. This keeps scheduled world ticks and external entities;
it is an entity reload, not a restart or a chunk-ticket test. Slots share forced
chunks and the shared level, so chunk ticket removal cannot isolate an area.

The Rust reload adapter now uses the MIT glue's kinetic, belt and sequencer NBT
codecs over a local typed tag tree. It encodes and decodes the records before
calling column_unloaded, load_settings/load_belt_items, block_loaded,
restore_state, set_sequencer_instructions and restore_sequencer. Conveyor links
use Chains::unload/add and set_far. Host inventory slots and valve use records
cross the serialization boundary independently of the engine. Depot motion and
engine crank timers are reset exactly where the current glue has no persisted
restore entry; the driver does not invent one. Current-save round trips never
enter legacy migration. Corrupt/old save injection remains a separate scope.
