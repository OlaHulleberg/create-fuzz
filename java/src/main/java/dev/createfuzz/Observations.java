package dev.createfuzz;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.simibubi.create.content.kinetics.base.IRotate;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.gauge.GaugeBlock;
import com.simibubi.create.content.kinetics.transmission.SplitShaftBlockEntity;
import com.simibubi.create.content.kinetics.transmission.sequencer.SequencerInstructions;

import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;

import java.util.ArrayList;
import java.util.List;

/** Normalized semantic observations; saved NBT is diagnostic only. */
final class Observations {
    static JsonObject snapshot(ServerLevel level, BlockPos origin, JsonObject c) {
        JsonObject blocks = new JsonObject();
        var networks = new java.util.TreeMap<Long, java.util.List<String>>();
        JsonObject sizes = new JsonObject();
        List<JsonObject> all = new ArrayList<>();
        for (JsonElement b : c.getAsJsonArray("blocks")) all.add(b.getAsJsonObject());
        if (c.has("events"))
            for (JsonElement b : c.getAsJsonArray("events")) all.add(b.getAsJsonObject());
        int[] lo = {24, 24, 24}, hi = {-24, -24, -24};
        for (JsonObject b : all)
            for (int a = 0; a < 3; a++) {
                int v = b.getAsJsonArray("pos").get(a).getAsInt();
                lo[a] = Math.min(lo[a], v);
                hi[a] = Math.max(hi[a], v);
            }
        for (int a = 0; a < 3; a++) {
            lo[a]--;
            hi[a]++;
        }
        if (!all.isEmpty()
                && (lo[0] < -24
                        || lo[2] < -24
                        || lo[1] < 0
                        || hi[0] > 24
                        || hi[2] > 24
                        || hi[1] > 24)) throw new IllegalArgumentException("region exceeds slot");
        for (int x = lo[0]; x <= hi[0]; x++)
            for (int y = lo[1]; y <= hi[1]; y++)
                for (int z = lo[2]; z <= hi[2]; z++) {
                    String key = x + "," + y + "," + z;
                    BlockPos pos = origin.offset(x, y, z);
                    BlockState state = level.getBlockState(pos);
                    JsonObject snap = new JsonObject();
                    snap.addProperty("state", BlockStateParser.serialize(state));
                    BlockEntity be = level.getBlockEntity(pos);
                    if (be instanceof KineticBlockEntity k) {
                        snap.add("kinetic", kinetic(k));
                        if (k.network != null) {
                            networks.computeIfAbsent(k.network, ignored -> new ArrayList<>())
                                    .add(key);
                            sizes.addProperty(key, networkSize(k));
                        }
                    }
                    var bracket =
                            com.simibubi.create.foundation.blockEntity.behaviour
                                    .BlockEntityBehaviour.get(
                                    level,
                                    pos,
                                    com.simibubi.create.content.decoration.bracket
                                            .BracketedBlockEntityBehaviour.TYPE);
                    if (bracket != null && bracket.getBracket() != null)
                        snap.addProperty(
                                "bracket", BlockStateParser.serialize(bracket.getBracket()));
                    // For reading mismatches only; never compared.
                    if (be != null)
                        snap.addProperty(
                                "debug", be.saveWithoutMetadata(level.registryAccess()).toString());
                    blocks.add(key, snap);
                }
        JsonObject out = new JsonObject();
        out.addProperty("side", "java");
        out.add("blocks", blocks);
        var partitions = new ArrayList<java.util.List<String>>(networks.values());
        for (var group : partitions) java.util.Collections.sort(group);
        partitions.sort(java.util.Comparator.comparing(group -> group.get(0)));
        JsonObject normalized = new JsonObject();
        normalized.add("partitions", CreateFuzz.GSON.toJsonTree(partitions));
        normalized.add("sizes", sizes);
        out.add("networks", normalized);
        return out;
    }

    /** Read the registry without getOrCreateNetworkFor: observing must never mutate it. */
    @SuppressWarnings("unchecked")
    static int networkSize(KineticBlockEntity k) {
        try {
            var f =
                    com.simibubi.create.content.kinetics.TorquePropagator.class.getDeclaredField(
                            "networks");
            f.setAccessible(true);
            var worlds =
                    (java.util.Map<
                                    net.minecraft.world.level.LevelAccessor,
                                    java.util.Map<
                                            Long,
                                            com.simibubi.create.content.kinetics.KineticNetwork>>)
                            f.get(null);
            var networks = worlds.get(k.getLevel());
            var network = networks == null ? null : networks.get(k.network);
            if (network == null)
                throw new IllegalStateException("kinetic entity names an absent network");
            return network.getSize();
        } catch (ReflectiveOperationException ex) {
            throw new RuntimeException(ex);
        }
    }

    static JsonObject kinetic(KineticBlockEntity k) {
        JsonObject o = new JsonObject();
        o.addProperty("speed", k.getSpeed());
        if (k instanceof com.simibubi.create.content.kinetics.crank.ValveHandleBlockEntity valve) {
            JsonObject v=new JsonObject();
            v.addProperty("selected",valve.angleInput.getValue());
            v.addProperty("in_use",valve.inUse);
            v.addProperty("angle",valve.getIndependentAngle(0));
            o.add("valve",v);
        }
        if (k instanceof com.simibubi.create.content.kinetics.chainDrive.ChainGearshiftBlockEntity chain)
            o.addProperty("chain_modifier", chain.getModifier());
        if (k instanceof com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity motor)
            o.addProperty("speed_setting", motor.generatedSpeed.getValue());
        if (k instanceof com.simibubi.create.content.kinetics.speedController.SpeedControllerBlockEntity controller)
            o.addProperty("speed_setting", controller.targetSpeed.getValue());
        o.addProperty("theoretical", k.getTheoreticalSpeed());
        o.addProperty("overstressed", k.isOverStressed());
        o.addProperty("stress", field(k, "stress"));
        o.addProperty("capacity", field(k, "capacity"));
        o.addProperty("impact", k.calculateStressApplied());
        o.addProperty("added_capacity", k.calculateAddedStressCapacity());
        o.addProperty("generated", k.getGeneratedSpeed());
        o.addProperty("has_source", k.hasSource());
        if (k.getBlockState().getBlock() instanceof GaugeBlock gauge)
            o.addProperty(
                    "comparator",
                    gauge.getAnalogOutputSignal(k.getBlockState(), k.getLevel(), k.getBlockPos()));
        if (k.sequenceContext != null) {
            String name =
                    k.sequenceContext.instruction() == SequencerInstructions.TURN_ANGLE
                            ? "sequence_angle"
                            : "sequence_distance";
            o.addProperty(name, k.sequenceContext.getEffectiveValue(k.getTheoreticalSpeed()));
        }
        if (k instanceof com.simibubi.create.content.kinetics.transmission.sequencer.SequencedGearshiftBlockEntity) {
            var tag = k.saveWithoutMetadata(k.getLevel().registryAccess());
            JsonObject run = new JsonObject();
            run.addProperty("current", tag.getInt("InstructionIndex"));
            run.addProperty("duration", tag.getInt("InstructionDuration"));
            run.addProperty("progress", tag.getFloat("InstructionProgress"));
            run.addProperty("timer", tag.getInt("Timer"));
            run.addProperty("powered_before", tag.getBoolean("PrevPowered"));
            o.add("sequencer", run);
        }
        // A clutch's or gearshift's halves: the speed each shaft face turns at.
        if (k instanceof SplitShaftBlockEntity s
                && k.getBlockState().getBlock() instanceof IRotate r) {
            JsonObject faces = new JsonObject();
            Direction.Axis axis = r.getRotationAxis(k.getBlockState());
            for (Direction d : Direction.values())
                if (d.getAxis() == axis)
                    faces.addProperty(
                            d.getSerializedName(),
                            s.getSpeed() == 0 ? 0f : s.getSpeed() * s.getRotationSpeedModifier(d));
            o.add("faces", faces);
        }
        return o;
    }

    static float field(KineticBlockEntity k, String name) {
        try {
            java.lang.reflect.Field f = KineticBlockEntity.class.getDeclaredField(name);
            f.setAccessible(true);
            return f.getFloat(k);
        } catch (ReflectiveOperationException ex) {
            throw new RuntimeException(ex);
        }
    }
}
