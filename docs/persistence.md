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
