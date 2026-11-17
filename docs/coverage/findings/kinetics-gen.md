# Unfiled observations from kinetics-gen

Compared against create-horizon `34c781e89feb33133ce44076526291a63046a892` using the shared Java server. Both cases were reduced with `prlimit --as=3000000000` and reproduced in three consecutive checks. No comparison exceptions or engine changes were made.

## Admission after an early network reload

Case: [kinetic-admission-reload.json](kinetic-admission-reload.json), new network-generator seed 1328.

First observation at tick 3, motor `0,8,0`: Java saves stress 0 and network size 1; the port saves stress 256 and size 3. Both report speed 32 and capacity 524288. The press and stressometer are admitted after the tick-1 reload.

Create's `KineticBlockEntity.initialize` calls `network.initFromTE(capacity, stress, networkSize)` followed by `network.addSilently(this, lastCapacityProvided, lastStressApplied)`. `KineticNetwork.addSilently` inserts members and subtracts unloaded contributions without synchronizing every member's cached totals. `KineticBlockEntity.tick` calls `getOrCreateNetwork().updateNetwork()` only when `networkDirty` is set, and `KineticNetwork.updateNetwork` synchronizes when stress or capacity changes. These are the relevant admission/cache timing decisions; the port's saved totals are already updated at the checkpoint.

## Tank placement and reload in the same tick

Case: [tank-placement-reload.json](tank-placement-reload.json), seed 1256. This uses an existing fluid-generator trace rather than a new kinetic generator.

First observation at tick 14, tank `0,4,0`: Java reports height 2 and capacity 16000; the port reports height 1 and capacity 8000. Both contain no fluid and identify the same controller.

Create's `FluidTankBlockEntity.read` restores `updateConnectivity = compound.contains("Uninitialized")`, then reads `Size` and `Height`; its `tick` invokes `updateConnectivity()` when pending. That method calls `ConnectivityHandler.formMulti(this)` for a server-side controller. The case places the upper tank and reloads at tick 13, exercising persistence of pending connectivity work.

## Inconclusive valve membership candidate

Case: [valve-membership-candidate.json](valve-membership-candidate.json), reduced from seed 1578. In the original sweep Java reported three network members and the port six after adding external power to a previously reloaded handle. The reduction retained the network-size witness but differed in two checks and matched in a third.

Original seeds 1578, 1642 and 1898 also alternated between matches and differences in repeated comparisons. Seed 1770 differed in three comparisons, then matched when reduction was attempted; four subsequent standalone Java/port snapshot pairs all reported three members on both sides, including a pair at the original recorded save phase. These observations do not establish which side or shared process state causes the variation. This remains a candidate, not a confirmed port finding.

The relevant Java bookkeeping is `KineticNetwork.getSize`, which returns `members.size() + unloadedMembers`; `addSilently` decrements and clamps `unloadedMembers` while admitting a restored entity. `KineticBlockEntity.initialize` supplies the saved network size. The independent angle-loss symptom is already filed as #56.

The final sweep separately detected genuine Java snapshot variation for seeds 1338 and 1466. Its unstable records are retained under `/mnt/lanes/agent-kgen/final-sweep-found/`.

Existing issues #7 and #55–#58 were checked separately and are not presented here as new findings.
