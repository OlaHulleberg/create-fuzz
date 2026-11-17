package dev.createfuzz;

import com.google.gson.JsonObject;
import com.simibubi.create.content.equipment.wrench.IWrenchable;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.util.FakePlayerFactory;

/** Version 2 player inputs; the actual Create block handles placement and wrenching. */
final class Actions {
    static void apply(ServerLevel level, BlockPos pos, JsonObject a) {
        var player = new net.minecraft.server.level.ServerPlayer(level.getServer(),level,
            new com.mojang.authlib.GameProfile(java.util.UUID.fromString("00000000-0000-0000-0000-000000000123"),"FuzzPlayer"),net.minecraft.server.level.ClientInformation.createDefault());
        player.setShiftKeyDown(false);
        player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
        switch (a.get("kind").getAsString()) {
            case "remove" -> FakePlayerFactory.getMinecraft(level).gameMode.destroyBlock(pos);
            case "signal" -> {
                int strength=a.get("strength").getAsInt();
                if(strength<0 || strength>15) throw new IllegalArgumentException("signal outside 0..15");
                level.setBlock(pos, SignalBlock.BLOCK.get().defaultBlockState().setValue(SignalBlock.STRENGTH,strength),3);
            }
            case "wrench" -> {
                Direction face=Direction.byName(a.get("face").getAsString());
                var state=level.getBlockState(pos);
                if(state.getBlock() instanceof IWrenchable w)
                    w.onWrenched(state,new UseOnContext(level,player,InteractionHand.MAIN_HAND,ItemStack.EMPTY,new BlockHitResult(Vec3.atCenterOf(pos),face,pos,false)));
            }
            case "place" -> {
                var block=BuiltInRegistries.BLOCK.get(ResourceLocation.parse(a.get("block").getAsString()));
                Direction face=Direction.byName(a.get("face").getAsString());
                player.setYRot(a.get("yaw").getAsFloat()); player.setYHeadRot(a.get("yaw").getAsFloat()); player.setXRot(a.get("pitch").getAsFloat());
                player.setShiftKeyDown(a.get("sneaking").getAsBoolean());
                var stack=new ItemStack(block.asItem());
                player.setItemInHand(InteractionHand.MAIN_HAND,stack);
                // Click the neighbour's face: the context resolves the target air position.
                BlockPos support=pos.relative(face.getOpposite());
                var hit=new BlockHitResult(Vec3.atCenterOf(pos).subtract(Vec3.atLowerCornerOf(face.getNormal()).scale(.5)),face,support,false);
                var ctx=new BlockPlaceContext(level,player,InteractionHand.MAIN_HAND,stack,hit) {
                    @Override public BlockPos getClickedPos() { return pos; }
                };
                var state=block.getStateForPlacement(ctx);
                if(ctx.getClickedPos().equals(pos) && level.getBlockState(pos).isAir() && state!=null && state.canSurvive(level,pos)) {
                    level.setBlock(pos,state,3); block.setPlacedBy(level,pos,state,player,stack);
                }
            }
            default -> throw new IllegalArgumentException("unsupported player action: "+a);
        }
    }
}
