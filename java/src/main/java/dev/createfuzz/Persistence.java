package dev.createfuzz;

import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.NbtUtils;

/** Read the actual server save, without runtime network ids or absolute world coordinates. */
final class Persistence {
    /** Fresh block entities, retaining blocks and scheduled world ticks, at the action boundary. */
    static void reload(net.minecraft.server.level.ServerLevel level, BlockPos origin, JsonObject scenario) {
        var lo = new int[]{24,24,24};
        var hi = new int[]{-24,-24,-24};
        var entries = new java.util.ArrayList<com.google.gson.JsonElement>();
        scenario.getAsJsonArray("blocks").forEach(entries::add);
        if (scenario.has("events")) scenario.getAsJsonArray("events").forEach(entries::add);
        for (var e : entries) for (int a=0;a<3;a++) {
            int v=e.getAsJsonObject().getAsJsonArray("pos").get(a).getAsInt();
            lo[a]=Math.min(lo[a],v); hi[a]=Math.max(hi[a],v);
        }
        var tags = new java.util.LinkedHashMap<BlockPos,net.minecraft.nbt.CompoundTag>();
        for(int x=lo[0]-1;x<=hi[0]+1;x++) for(int y=lo[1]-1;y<=hi[1]+1;y++) for(int z=lo[2]-1;z<=hi[2]+1;z++) {
            var pos=origin.offset(x,y,z);
            var be=level.getBlockEntity(pos);
            if(be==null) continue;
            var tag=be.saveWithoutMetadata(level.registryAccess());
            tag.putString("id",net.minecraft.core.registries.BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType()).toString());
            tags.put(pos,tag);
        }
        for(var pos:tags.keySet()) {
            var be=level.getBlockEntity(pos);
            be.onChunkUnloaded();
            level.removeBlockEntity(pos);
        }
        for(var entry:tags.entrySet()) {
            var pos=entry.getKey();
            var fresh=net.minecraft.world.level.block.entity.BlockEntity.loadStatic(pos,level.getBlockState(pos),entry.getValue(),level.registryAccess());
            if(fresh==null) throw new IllegalStateException("reload failed at "+pos);
            level.setBlockEntity(fresh);
        }
    }
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
