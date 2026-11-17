package dev.createfuzz;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.simibubi.create.content.kinetics.crank.HandCrankBlockEntity;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;

import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

/**
 * Runs fuzzing scenarios on a dedicated server without restarting it. A batch is a JSON array of
 * scenarios dropped into {@code <dir>/inbox/<name>.json}; each case gets its own area, its blocks
 * are placed one per tick (block i before tick i), all cases of a batch run at once, and the
 * snapshots are written to {@code <dir>/outbox/<name>.json} in the batch's order. The directory is
 * {@code $FUZZ_DIR}, or {@code fuzz} in the server's directory.
 */
@Mod("createfuzz")
public class CreateFuzz {
    static final Gson GSON = new GsonBuilder().disableHtmlEscaping().create();

    /** Cases sit on a grid, this far apart, at this height (above a flat world's floor). */
    static final int SPACING = 64, Y = 0, CLEAR_RADIUS = 24, CLEAR_HEIGHT = 24;

    final Path dir;
    Batch batch;

    /**
     * The areas cases run in: force-loaded once at start, so no chunk loads while cases run (at a
     * sprint tick rate chunk loading lags, and block entities placed in a chunk not yet in the
     * level wait to tick). A batch holds at most this many cases.
     */
    static final int SLOTS = 128;

    int startup = -1;
    int poll;

    public CreateFuzz(IEventBus modBus) {
        SignalBlock.REGISTRY.register(modBus);
        String env = System.getenv("FUZZ_DIR");
        dir = Path.of(env != null ? env : "fuzz");
        NeoForge.EVENT_BUS.addListener(this::started);
        NeoForge.EVENT_BUS.addListener(this::tick);
    }

    void started(ServerStartedEvent e) {
        try {
            Catalogue.audit();
            ItemInputs.audit();
            Files.createDirectories(dir);
            Files.writeString(
                    dir.resolve("actor-probe.json"),
                    GSON.toJson(PlayerActor.probe(e.getServer().overworld())));
            Files.writeString(dir.resolve("default-states.json"), DefaultStates.json());
            Files.writeString(
                    dir.resolve("runtime-probe.json"),
                    GSON.toJson(RuntimeProbe.angles(e.getServer().overworld())));
            Files.createDirectories(dir.resolve("inbox"));
            Files.createDirectories(dir.resolve("outbox"));
            ServerLevel level = e.getServer().overworld();
            for (int i = 0; i < SLOTS; i++) {
                BlockPos o = slot(i);
                for (int cx = (o.getX() - CLEAR_RADIUS) >> 4;
                        cx <= (o.getX() + CLEAR_RADIUS) >> 4;
                        cx++)
                    for (int cz = (o.getZ() - CLEAR_RADIUS) >> 4;
                            cz <= (o.getZ() + CLEAR_RADIUS) >> 4;
                            cz++) level.setChunkForced(cx, cz, true);
            }
            // Run ticks as fast as the server can.
            e.getServer().tickRateManager().setTickRate(10000f);
            startup = 0;
        } catch (IOException ex) {
            throw new RuntimeException(ex);
        }
    }

    static BlockPos slot(int i) {
        return new BlockPos((i % 16) * SPACING, Y, (i / 16) * SPACING);
    }

    void tick(ServerTickEvent.Post e) {
        MinecraftServer server = e.getServer();
        ServerLevel level = server.overworld();
        // Ready once every slot's blocks tick, and 100 ticks more for their chunks to settle.
        if (startup >= 0) {
            boolean all = true;
            for (int i = 0; i < SLOTS && all; i++) all = level.isPositionEntityTicking(slot(i));
            if (all && ++startup > 100) {
                startup = -1;
                try {
                    Files.writeString(dir.resolve("ready"), "ready\n");
                } catch (IOException ex) {
                    throw new RuntimeException(ex);
                }
            }
            return;
        }
        if (batch == null) {
            if (poll++ % 5 != 0) return;
            Path next = nextInbox();
            if (next == null) return;
            try {
                JsonArray cases = JsonParser.parseString(Files.readString(next)).getAsJsonArray();
                Files.delete(next);
                batch = new Batch(next.getFileName().toString(), cases, level);
                batch.start(level);
                if (batch.name.startsWith("cover-")) coverage(null);
            } catch (Exception ex) {
                fail(level, next.getFileName().toString(), ex);
            }
            return;
        }
        boolean done;
        try {
            done = batch.step(level);
        } catch (Exception ex) {
            fail(level, batch.name, ex);
            return;
        }
        if (done) {
            try {
                if (batch.name.startsWith("cover-"))
                    coverage(dir.resolve("outbox").resolve(batch.name + ".exec"));
                Path tmp = dir.resolve("outbox").resolve(batch.name + ".tmp");
                Files.writeString(tmp, GSON.toJson(batch.results), StandardCharsets.UTF_8);
                Files.move(
                        tmp,
                        dir.resolve("outbox").resolve(batch.name),
                        StandardCopyOption.ATOMIC_MOVE);
            } catch (IOException ex) {
                throw new RuntimeException(ex);
            }
            batch.clear(level);
            batch = null;
        }
    }

    /**
     * Reset after cleanup, dump before publishing the batch response. No agent dependency in the
     * mod.
     */
    static void coverage(Path output) {
        try {
            Object agent =
                    Class.forName(
                                    "org.jacoco.agent.rt.RT",
                                    true,
                                    ClassLoader.getSystemClassLoader())
                            .getMethod("getAgent")
                            .invoke(null);
            if (output == null) agent.getClass().getMethod("reset").invoke(agent);
            else
                Files.write(
                        output,
                        (byte[])
                                agent.getClass()
                                        .getMethod("getExecutionData", boolean.class)
                                        .invoke(agent, true));
        } catch (Exception ex) {
            throw new RuntimeException("coverage requires the JaCoCo agent", ex);
        }
    }

    Path nextInbox() {
        try (Stream<Path> s = Files.list(dir.resolve("inbox"))) {
            return s.filter(p -> p.toString().endsWith(".json")).sorted().findFirst().orElse(null);
        } catch (IOException ex) {
            return null;
        }
    }

    /** A batch that could not run: its answer is the error, and its area is cleared. */
    void fail(ServerLevel level, String name, Exception ex) {
        try {
            Files.deleteIfExists(dir.resolve("inbox").resolve(name));
            JsonObject o = new JsonObject();
            o.addProperty("error", ex + (ex.getCause() != null ? ": " + ex.getCause() : ""));
            Files.writeString(dir.resolve("outbox").resolve(name), GSON.toJson(o));
        } catch (IOException ignored) {
        }
        if (batch != null) batch.clear(level);
        batch = null;
    }

    /** One batch's cases, run side by side. */
    static class Batch {
        final String name;
        final List<JsonObject> cases = new ArrayList<>();
        final List<BlockPos> origins = new ArrayList<>();
        final List<ServerPlayer> actors = new ArrayList<>();
        final JsonArray results = new JsonArray();
        final List<JsonObject> traces = new ArrayList<>();
        int tick, maxTicks;

        Batch(String name, JsonArray in, ServerLevel level) {
            this.name = name;
            for (int i = 0; i < in.size(); i++) {
                cases.add(in.get(i).getAsJsonObject());
                int version =
                        cases.get(i).has("version") ? cases.get(i).get("version").getAsInt() : 1;
                if (version != 1 && version != 3)
                    throw new IllegalArgumentException("unsupported scenario version");
                if (i >= SLOTS)
                    throw new IllegalArgumentException("a batch holds at most " + SLOTS + " cases");
                origins.add(slot(i));
                actors.add(PlayerActor.create(level, i));
                maxTicks = Math.max(maxTicks, in.get(i).getAsJsonObject().get("ticks").getAsInt());
                results.add(new JsonObject());
                traces.add(new JsonObject());
            }
        }

        void start(ServerLevel level) {
            clear(level);
            NetworkIsolation.reset(level);
            // Cuckoo clocks read the scenario's day time, independent of server history.
            long dayTime =
                    cases.isEmpty() || !cases.get(0).has("day_time")
                            ? 0
                            : cases.get(0).get("day_time").getAsLong();
            for (JsonObject c : cases) {
                if ((c.has("day_time") ? c.get("day_time").getAsLong() : 0) != dayTime)
                    throw new IllegalArgumentException("a batch's cases share one day time");
            }
            level.setDayTime(dayTime);

        }

        /** One tick of every case; true when all are done. */
        boolean step(ServerLevel level) {
            if(tick==0) for(var c:cases) c.addProperty("_save_phase",Math.floorMod(level.getGameTime(),10));
            for (int i = 0; i < cases.size(); i++) {
                JsonObject c = cases.get(i);
                if (tick > c.get("ticks").getAsInt()) continue;
                if (c.has("checkpoints"))
                    for (JsonElement t : c.getAsJsonArray("checkpoints"))
                        if (t.getAsInt() == tick)
                            traces.get(i)
                                    .add(
                                            Integer.toString(tick),
                                            Observations.snapshot(level, origins.get(i), c, actors.get(i)));
                if (tick == c.get("ticks").getAsInt()) {
                    JsonObject result = Observations.snapshot(level, origins.get(i), c, actors.get(i));
                    result.add("checkpoints", traces.get(i));
                    results.set(i, result);
                    continue;
                }
                actors.get(i).getCooldowns().tick();
                // A player places the blocks, one each tick.
                JsonArray blocks = c.getAsJsonArray("blocks");
                if (tick < blocks.size())
                    place(level, origins.get(i), blocks.get(tick).getAsJsonObject());
                // A player sets a motor's speed after placing it, on the next tick.
                if (tick >= 1 && tick - 1 < blocks.size())
                    setMotor(level, actors.get(i), origins.get(i), blocks.get(tick - 1).getAsJsonObject());
                if (!c.has("events")) continue;
                for (JsonElement ev : c.getAsJsonArray("events")) {
                    JsonObject o = ev.getAsJsonObject();
                    // A motor placed the tick before gets its speed now.
                    if (o.get("tick").getAsInt() + 1 == tick && o.has("state"))
                        setMotor(level, actors.get(i), origins.get(i), o);
                }
                for (JsonElement ev : c.getAsJsonArray("events")) {
                    JsonObject o = ev.getAsJsonObject();
                    if (o.get("tick").getAsInt() != tick) continue;
                    if (o.has("action") && o.getAsJsonObject("action").get("kind").getAsString().equals("reload"))
                        Persistence.reload(level, origins.get(i), c);
                    else if (o.has("action"))
                        Actions.apply(
                                level,
                                actors.get(i),
                                at(origins.get(i), o),
                                origins.get(i), o.getAsJsonObject("action"));
                    else if (o.has("state")) place(level, origins.get(i), o);
                    // HandCrankBlock.useWithoutItem: turn(sneaking).
                    else if (o.has("turn")
                            && level.getBlockEntity(at(origins.get(i), o))
                                    instanceof HandCrankBlockEntity h)
                        h.turn(o.get("turn").getAsBoolean());
                }
            }
            // What is placed here acts from the next level tick: a case's tick t is level tick
            // start + t + 1,
            // so it is read here once its last tick, `ticks`, has run.

            tick++;
            return tick > maxTicks;
        }

        void clear(ServerLevel level) {
            BlockState air = Blocks.AIR.defaultBlockState();
            for (BlockPos o : origins)
                for (BlockPos p :
                        BlockPos.betweenClosed(
                                o.offset(-CLEAR_RADIUS, 0, -CLEAR_RADIUS),
                                o.offset(CLEAR_RADIUS, CLEAR_HEIGHT, CLEAR_RADIUS)))
                    if (!level.getBlockState(p).isAir()) level.setBlock(p, air, 3);
            // Clearing drops containers' contents (a depot's, a belt's): they must not reach the
            // next case in the slot. Every entity but players goes with the blocks.
            for (BlockPos o : origins) {
                var box =
                        new net.minecraft.world.phys.AABB(
                                        o.offset(-CLEAR_RADIUS, 0, -CLEAR_RADIUS).getCenter(),
                                        o.offset(CLEAR_RADIUS, CLEAR_HEIGHT, CLEAR_RADIUS)
                                                .getCenter())
                                .inflate(2);
                for (var e :
                        level.getEntitiesOfClass(
                                net.minecraft.world.entity.Entity.class,
                                box,
                                e -> !(e instanceof net.minecraft.world.entity.player.Player)))
                    e.discard();
            }
        }

        static BlockPos at(BlockPos origin, JsonObject b) {
            JsonArray p = b.getAsJsonArray("pos");
            return origin.offset(p.get(0).getAsInt(), p.get(1).getAsInt(), p.get(2).getAsInt());
        }

        static void place(ServerLevel level, BlockPos origin, JsonObject b) {
            BlockPos pos = at(origin, b);
            BlockState state;
            try {
                state =
                        BlockStateParser.parseForBlock(
                                        BuiltInRegistries.BLOCK.asLookup(),
                                        b.get("state").getAsString(),
                                        false)
                                .blockState();
            } catch (Exception ex) {
                throw new RuntimeException("bad state " + b.get("state"), ex);
            }
            if (state.hasProperty(
                            net.minecraft.world.level.block.state.properties.BlockStateProperties
                                    .WATERLOGGED)
                    && state.getValue(
                            net.minecraft.world.level.block.state.properties.BlockStateProperties
                                    .WATERLOGGED))
                throw new IllegalArgumentException(
                        "waterlogged=true requires a fluid-world adapter; kinetics scenarios are"
                                + " dry");
            level.setBlock(pos, state, 3);
        }

        static void setMotor(ServerLevel level, ServerPlayer player, BlockPos origin, JsonObject b) {
            if (!b.has("motor")
                    || !(level.getBlockEntity(at(origin, b)) instanceof CreativeMotorBlockEntity m))
                return;
            try {
                BlockState expected =
                        BlockStateParser.parseForBlock(
                                        BuiltInRegistries.BLOCK.asLookup(),
                                        b.get("state").getAsString(),
                                        false)
                                .blockState();
                if (level.getBlockState(at(origin, b)) == expected)
                    Actions.speedSetting(level, player, at(origin, b),
                            b.get("motor").getAsInt() < 0 ? 0 : 1, Math.abs(b.get("motor").getAsInt()));
            } catch (Exception ex) {
                throw new IllegalArgumentException("bad motor placement state", ex);
            }
        }
    }
}
