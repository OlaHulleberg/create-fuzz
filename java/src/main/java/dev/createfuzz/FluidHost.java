package dev.createfuzz;

import com.google.gson.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.storage.ServerLevelData;

/** Native environmental fluid ticks for the Rust driver, in a separate frozen region.
 * Block entities and scheduled block ticks do not run here. Create's fluid network is run
 * exclusively by Rust; only the public native fluid tick scheduler advances. */
final class FluidHost {
    private static final BlockPos ORIGIN = new BlockPos(2048, 0, 2048);
    private static String owner;
    private static long epoch;
    static boolean busy() {return owner!=null;}
    static void prepare(ServerLevel level) {
        for(int x=(ORIGIN.getX()-24)>>4;x<=(ORIGIN.getX()+24)>>4;x++)
            for(int z=(ORIGIN.getZ()-24)>>4;z<=(ORIGIN.getZ()+24)>>4;z++) level.setChunkForced(x,z,true);
    }
    static void abortIfOwner(ServerLevel level,String key) {if(key.equals(owner)) abort(level);}
    static void abort(ServerLevel level) {clear(level);owner=null;level.getServer().tickRateManager().setFrozen(false);}
    static void clear(ServerLevel level) {
        for(var p:BlockPos.betweenClosed(ORIGIN.offset(-24,0,-24),ORIGIN.offset(24,24,24)))
            if(!level.getBlockState(p).isAir()) level.setBlock(p,Blocks.AIR.defaultBlockState(),3);
        level.getFluidTicks().clearArea(new BoundingBox(ORIGIN.getX()-24,0,ORIGIN.getZ()-24,ORIGIN.getX()+24,24,ORIGIN.getZ()+24));
        level.getBlockTicks().clearArea(new BoundingBox(ORIGIN.getX()-24,0,ORIGIN.getZ()-24,ORIGIN.getX()+24,24,ORIGIN.getZ()+24));
    }
    static JsonObject handle(ServerLevel level, JsonObject request) throws Exception {
        String key=request.get("owner").getAsString();
        if(request.has("begin")) {
            if(owner!=null&&!owner.equals(key)) throw new IllegalStateException("fluid host already owned by "+owner);
            level.getServer().tickRateManager().setFrozen(true);
            owner=key;epoch=level.getGameTime();clear(level);
        }
        if(!key.equals(owner)) throw new IllegalStateException("fluid host owner mismatch");
        if(request.has("close")) {clear(level);owner=null;level.getServer().tickRateManager().setFrozen(false);return new JsonObject();}
        long clock=request.get("clock").getAsLong();
        var data=(ServerLevelData)level.getLevelData();
        data.setGameTime(epoch+clock-1);
        for(var value:request.getAsJsonArray("changes")) {
            var change=value.getAsJsonObject();var xyz=change.getAsJsonArray("pos");
            var pos=ORIGIN.offset(xyz.get(0).getAsInt(),xyz.get(1).getAsInt(),xyz.get(2).getAsInt());
            var state=BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK.asLookup(),change.get("state").getAsString(),false).blockState();
            level.setBlock(pos,state,3);
        }
        for(var value:request.getAsJsonArray("native")) {
            var xyz=value.getAsJsonArray();var pos=ORIGIN.offset(xyz.get(0).getAsInt(),xyz.get(1).getAsInt(),xyz.get(2).getAsInt());
            var state=level.getFluidState(pos);if(!state.isEmpty()) level.scheduleTick(pos,state.getType(),state.getType().getTickDelay(level));
        }
        data.setGameTime(epoch+clock);
        level.getFluidTicks().tick(epoch+clock,65536,(pos,fluid)-> {
            var state=level.getFluidState(pos);
            if(state.getType()==fluid) state.tick(level,pos);
        });
        for(var value:request.getAsJsonArray("manual")) {
            var xyz=value.getAsJsonArray();var pos=ORIGIN.offset(xyz.get(0).getAsInt(),xyz.get(1).getAsInt(),xyz.get(2).getAsInt());
            var state=level.getFluidState(pos);if(!state.isEmpty()) state.tick(level,pos);
        }
        FluidBounds.check(level,ORIGIN,request.getAsJsonArray("region"));
        var out=new JsonObject();var states=new JsonArray();
        for(var value:request.getAsJsonArray("region")) {
            var xyz=value.getAsJsonArray();var pos=ORIGIN.offset(xyz.get(0).getAsInt(),xyz.get(1).getAsInt(),xyz.get(2).getAsInt());
            var entry=new JsonObject();entry.add("pos",xyz);var state=level.getBlockState(pos);entry.addProperty("state",BlockStateParser.serialize(state));
            var facts=new JsonObject();facts.addProperty("replaceable",state.canBeReplaced());facts.addProperty("collision",!state.getCollisionShape(level,pos).isEmpty());
            int sturdy=0;for(var direction:net.minecraft.core.Direction.values())if(state.isFaceSturdy(level,pos,direction))sturdy|=1<<direction.get3DDataValue();facts.addProperty("sturdy",sturdy);
            var fs=state.getFluidState();if(!fs.isEmpty()){var fluid=new JsonObject();fluid.addProperty("fluid",BuiltInRegistries.FLUID.getKey(fs.getType()).toString().replace(":flowing_",":"));fluid.addProperty("source",fs.isSource());fluid.addProperty("amount",fs.getAmount());fluid.addProperty("falling",fs.hasProperty(net.minecraft.world.level.material.FlowingFluid.FALLING)&&fs.getValue(net.minecraft.world.level.material.FlowingFluid.FALLING));facts.add("fluid",fluid);}
            entry.add("facts",facts);states.add(entry);
        }
        out.add("states",states);
        var rules=new JsonObject();
        rules.addProperty("water_source_conversion",level.getGameRules().getBoolean(net.minecraft.world.level.GameRules.RULE_WATER_SOURCE_CONVERSION));
        rules.addProperty("lava_source_conversion",level.getGameRules().getBoolean(net.minecraft.world.level.GameRules.RULE_LAVA_SOURCE_CONVERSION));
        rules.addProperty("evaporates",level.dimensionType().ultraWarm());out.add("rules",rules);
        return out;
    }
}
