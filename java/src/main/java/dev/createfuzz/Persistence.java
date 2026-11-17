package dev.createfuzz;

import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.NbtUtils;

/** Read the actual server save, without runtime network ids or absolute world coordinates. */
final class Persistence {
    static JsonObject saved(KineticBlockEntity be, BlockPos origin) {
        var data = be.saveWithoutMetadata(be.getLevel().registryAccess());
        var out = new JsonObject();
        out.addProperty("speed", data.getFloat("Speed"));
        out.addProperty("needs_speed_update", data.getBoolean("NeedsSpeedUpdate"));
        out.add("source", JsonNull.INSTANCE);
        if (data.contains("Source")) {
            var source = NbtUtils.readBlockPos(data, "Source").orElseThrow().subtract(origin);
            out.add("source", CreateFuzz.GSON.toJsonTree(new int[]{source.getX(),source.getY(),source.getZ()}));
        }
        out.add("network", JsonNull.INSTANCE);
        if (data.contains("Network")) {
            var n = data.getCompound("Network");
            var network = new JsonObject();
            network.addProperty("stress", n.getFloat("Stress"));
            network.addProperty("capacity", n.getFloat("Capacity"));
            network.addProperty("size", n.getInt("Size"));
            network.addProperty("added_stress", n.getFloat("AddedStress"));
            network.addProperty("added_capacity", n.getFloat("AddedCapacity"));
            out.add("network", network);
        }
        return out;
    }
}
