# Kinetics observation boundary

The manifest retains the binary's full state domains. Generation uses only dry
states; both adapters reject waterlogged=true rather than deleting fluid results.
ProperWaterloggedBlock.updateWater, AbstractShaftBlock.updateShape,
HandCrankBlock.updateShape, DrillBlock.updateShape, PumpBlock.updateShape,
FluidValveBlock.updateShape, BacktankBlock.updateShape and
GirderEncasedShaftBlock.updateShape schedule vanilla water ticks. No fluid-world
adapter is present. This suite cannot establish water sensing/transfer parity.

The per-id observation_scope map contains property names and deciding MIT Java
methods. It applies symmetrically at comparison time, including checkpoints;
raw observations retain all values. It never removes kinetic fields.

| Blocks | Excluded output | Deciding method |
|---|---|---|
| display_board | up, down | FlapDisplayBlock.updateShape -> updatedShapeInner, updateColumn |
| metal_girder_encased_shaft | top, bottom | GirderEncasedShaftBlock.updateShape -> GirderBlock.updateVerticalProperty |
| encased_chain_drive, adjustable_chain_gearshift | part, axis_along_first | ChainDriveBlock.updateShape (inherited by ChainGearshiftBlock) |
| gantry_shaft | part | GantryShaftBlock.updateShape |
| gantry_carriage | axis_along_first | GantryCarriageBlock.updateShape -> cycleAxisIfNecessary |
| large_water_wheel | extension | LargeWaterWheelBlock.updateShape |
| fluid_valve | enabled | FluidValveBlockEntity.tick, fluids phase |

ChainDriveBlock's CONNECTED_ALONG_FIRST_COORDINATE is serialized axis_along_first.
GantryCarriageBlock cycles the same serialized property. These shape outputs can
affect topology; chain/gantry/wheel generation remains excluded pending the relevant
subsystem adapters. Masking properties does not make their missing topology
adapters supported, nor excuse speed/source/network discrepancies.

The remaining manifest-block updateShape overrides either return the state after
scheduling water (including inherited AbstractShaftBlock and HandCrankBlock), or
schedule subsystem work: WaterWheelBlock schedules a flow check; BeltBlock updates
tunnel connections and block-entity covered status, without changing its state.
PulleyBlock's waterlogged updateShape is on its nested rope class. Other kinetic
classes have no additional state-changing updateShape override. Placement methods
select facing, axis, ceiling, crafter pointing and shaft flags as input choices;
direct state injection supplies these, so those properties stay compared.
Waterlogging selected during placement remains prohibited for this dry phase.

FluidValveBlockEntity.onSpeedChanged chases its pointer toward 1 for positive speed
and toward 0 otherwise; tick changes enabled at those endpoints. This driver runs
the kinetic solver, not the valve pointer/fluid subsystem. The manifest's
state_phase records that separation. Valve kinetic observations remain required.

Network counts read TorquePropagator.networks[level][entity.network].getSize().
KineticNetwork.getSize adds unloadedMembers to members.size. Reflection is confined
to the MIT registry field; a missing named network fails rather than constructing
one. Partitions still use block-entity network ids. Split-shaft member-count
differences remain findings: Java owns one KineticBlockEntity per split shaft.

Motor and controller kinetic-board selections now appear as `kinetic.speed_setting`
in both snapshots and are compared with the ordinary numeric tolerance, including
presence versus absence. No transmission state exclusions were removed: chain
shape, sequencer execution and valve observations still require their adapters.
