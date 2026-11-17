package dev.createfuzz;

import com.google.gson.JsonObject;
import com.simibubi.create.content.kinetics.TorquePropagator;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.structure.BoundingBox;

import java.lang.management.ManagementFactory;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Map;
import java.util.UUID;

/** A case owns the entire empty test level; no two cases simulate concurrently. */
final class CaseIsolation {
    static CompoundTag rules;
    static Path directory;
    static final String BOOT = UUID.randomUUID().toString();
    static long generation;
    static long discardedEntities, escapedEntities;
    static final java.util.Set<BlockPos> cleared = new java.util.LinkedHashSet<>();

    static void initialize(ServerLevel level, Path dir) throws Exception {
        directory = dir;
        rules = level.getGameRules().createTag().copy();
        var options = ManagementFactory.getPlatformMXBean(com.sun.management.HotSpotDiagnosticMXBean.class);
        if (!options.getVMOption("EnableDynamicAgentLoading").getValue().equals("false"))
            throw new IllegalStateException("harness requires -XX:-EnableDynamicAgentLoading; runtime redefinition invalidates comparisons");
        if ("1".equals(System.getenv("FUZZ_ISOLATION_SELF_TEST"))) selfTest(level);
        String hash = System.getenv("FUZZ_HARNESS_HASH");
        if (hash == null || hash.isBlank()) throw new IllegalStateException("start the harness with run.sh server");
        var stamp = new JsonObject();
        stamp.addProperty("boot", BOOT);
        stamp.addProperty("source_hash", hash);
        stamp.addProperty("protocol", 1);
        Files.writeString(dir.resolve("harness-runtime.json"), CreateFuzz.GSON.toJson(stamp));
    }

    static void settings(ServerLevel level) {
        if (level.getServer().tickRateManager().isFrozen())
            throw new IllegalStateException("case isolation: server ticks are frozen or stepping");
        if (!level.players().isEmpty())
            throw new IllegalStateException("case isolation: connected players share the test level");
    }

    static void clear(ServerLevel level, BlockPos origin) {
        cleared.add(origin.immutable());
        for (var pos : BlockPos.betweenClosed(origin.offset(-24, 0, -24), origin.offset(24, 24, 24)))
            if (!level.getBlockState(pos).isAir()) level.setBlock(pos, Blocks.AIR.defaultBlockState(), 3);
        // The old AABB missed drops which fell below zero or escaped a slot.
        var entities = new ArrayList<Entity>();
        for (var entity : level.getAllEntities()) if (!(entity instanceof Player)) entities.add(entity);
        for (var entity : entities) {
            discardedEntities++;
            if (entity.getY() < -1.5 || Math.abs(entity.getX() - origin.getX()) > 26
                    || Math.abs(entity.getZ() - origin.getZ()) > 26) escapedEntities++;
            entity.discard();
        }
        var bounds = new BoundingBox(origin.getX() - 24, level.getMinBuildHeight(), origin.getZ() - 24,
                origin.getX() + 24, level.getMaxBuildHeight(), origin.getZ() + 24);
        level.getBlockTicks().clearArea(bounds);
        level.getFluidTicks().clearArea(bounds);
        // With one active case, these callbacks cannot belong to another case.
        try {
            for (String name : new String[]{"blockEvents", "blockEventsToReschedule"}) {
                var field = ServerLevel.class.getDeclaredField(name);
                field.setAccessible(true);
                ((Collection<?>) field.get(level)).clear();
            }
        } catch (ReflectiveOperationException e) { throw new IllegalStateException("cannot dispose case block events", e); }
    }

    static void empty(ServerLevel level) {
        settings(level);
        if (!rules.equals(level.getGameRules().createTag()))
            throw new IllegalStateException("case isolation: gamerules changed since harness startup");
        for (var origin : cleared)
            for (var pos : BlockPos.betweenClosed(origin.offset(-24, 0, -24), origin.offset(24, 24, 24)))
                if (!level.getBlockState(pos).isAir())
                    throw new IllegalStateException("case isolation: leaked block at " + pos);
        for (var entity : level.getAllEntities())
            if (!(entity instanceof Player)) throw new IllegalStateException("case isolation: leaked entity " + entity.getType() + " at " + entity.blockPosition());
        if (level.getBlockTicks().count() != 0 || level.getFluidTicks().count() != 0)
            throw new IllegalStateException("case isolation: scheduled ticks survived disposal");
        try {
            for (String name : new String[]{"blockEntityTickers", "pendingBlockEntityTickers"}) {
                var field = net.minecraft.world.level.Level.class.getDeclaredField(name);
                field.setAccessible(true);
                for (var entry : (Collection<?>) field.get(level)) {
                    var ticker = (net.minecraft.world.level.block.entity.TickingBlockEntity) entry;
                    if (!ticker.isRemoved()) throw new IllegalStateException("case isolation: leaked block entity at " + ticker.getPos());
                }
            }
        } catch (ReflectiveOperationException e) { throw new IllegalStateException("cannot verify case tickers", e); }
    }

    static void begin(ServerLevel level) {
        empty(level);
        NetworkIsolation.reset(level);
        try {
            var field = TorquePropagator.class.getDeclaredField("networks");
            field.setAccessible(true);
            if (!((Map<?, ?>) ((Map<?, ?>) field.get(null)).get(level)).isEmpty())
                throw new IllegalStateException("case isolation: kinetic namespace was not reset");
        } catch (ReflectiveOperationException e) { throw new IllegalStateException(e); }
        cleared.clear();
        generation++;
        var health = new JsonObject();
        health.addProperty("boot", BOOT);
        health.addProperty("generation", generation);
        health.addProperty("game_time", level.getGameTime());
        health.addProperty("day_time", level.getDayTime());
        health.addProperty("entities", 0);
        health.addProperty("discarded_entities", discardedEntities);
        health.addProperty("escaped_entities_discarded", escapedEntities);
        health.addProperty("block_entities", 0);
        health.addProperty("scheduled_ticks", 0);
        health.addProperty("kinetic_networks", 0);
        try { Files.writeString(directory.resolve("harness-health.json"), CreateFuzz.GSON.toJson(health)); }
        catch (java.io.IOException e) { throw new IllegalStateException(e); }
    }
    /** Opt-in fault injection before ready; no scenario runs with the injected state. */
    static void selfTest(ServerLevel level) {
        var origin = BlockPos.ZERO;
        var escaped = new net.minecraft.world.entity.item.ItemEntity(level, -33, -10, -1,
                new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.COBBLESTONE));
        level.addFreshEntity(escaped);
        expectLeak("leaked entity", () -> empty(level));
        clear(level, origin);
        level.scheduleTick(origin.above(8), Blocks.STONE, 100);
        expectLeak("scheduled ticks", () -> empty(level));
        clear(level, origin);
        var dropRule = level.getGameRules().getRule(net.minecraft.world.level.GameRules.RULE_DOBLOCKDROPS);
        boolean previous = dropRule.get();
        try {
            dropRule.set(!previous, level.getServer());
            expectLeak("gamerules", () -> empty(level));
        } finally { dropRule.set(previous, level.getServer()); }
        level.setBlock(origin.above(8), com.simibubi.create.AllBlocks.SHAFT.get().defaultBlockState(), 3);
        expectLeak("leaked", () -> empty(level));
        clear(level, origin);
        System.out.println("isolation self-test: all injected leaks rejected and disposed");
    }

    static void expectLeak(String expected, Runnable check) {
        try { check.run(); }
        catch (IllegalStateException e) {
            if (!e.getMessage().contains(expected)) throw e;
            System.out.println("isolation self-test: rejected " + expected);
            return;
        }
        throw new IllegalStateException("isolation self-test missed " + expected);
    }

}
