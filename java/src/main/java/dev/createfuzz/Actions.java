package dev.createfuzz;

import com.google.gson.JsonObject;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.util.FakePlayerFactory;

/** Player inputs enter the game-mode pipeline, including RightClickBlock and item placement. */
final class Actions {
    static void apply(ServerLevel level, BlockPos pos, JsonObject action) {
        var player = FakePlayerFactory.getMinecraft(level);
        player.setShiftKeyDown(false);
        player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
        player.setItemInHand(InteractionHand.OFF_HAND, ItemStack.EMPTY);
        switch (action.get("kind").getAsString()) {
            case "use" -> {
                Direction face = Direction.byName(action.get("face").getAsString());
                var local = action.getAsJsonArray("hit");
                Vec3 point =
                        new Vec3(
                                pos.getX() + local.get(0).getAsDouble(),
                                pos.getY() + local.get(1).getAsDouble(),
                                pos.getZ() + local.get(2).getAsDouble());
                var item =
                        BuiltInRegistries.ITEM.get(
                                ResourceLocation.parse(action.get("item").getAsString()));
                var stack = new ItemStack(item, 64);
                player.setYRot(action.get("yaw").getAsFloat());
                player.setYHeadRot(action.get("yaw").getAsFloat());
                player.setXRot(action.get("pitch").getAsFloat());
                player.setShiftKeyDown(action.get("sneaking").getAsBoolean());
                player.setPos(pos.getX() + .5, pos.getY() + 1, pos.getZ() + 2);
                player.setItemInHand(InteractionHand.MAIN_HAND, stack);
                player.gameMode.useItemOn(
                        player,
                        level,
                        stack,
                        InteractionHand.MAIN_HAND,
                        new BlockHitResult(point, face, pos, false));
            }
            case "remove" -> player.gameMode.destroyBlock(pos);
            case "signal" -> {
                int strength = action.get("strength").getAsInt();
                if (strength < 0 || strength > 15)
                    throw new IllegalArgumentException("signal outside 0..15");
                level.setBlock(
                        pos,
                        SignalBlock.BLOCK
                                .get()
                                .defaultBlockState()
                                .setValue(SignalBlock.STRENGTH, strength),
                        3);
            }
            default -> throw new IllegalArgumentException("unsupported player action: " + action);
        }
    }
}
