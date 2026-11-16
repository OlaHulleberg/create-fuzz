package dev.createfuzz;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;

import net.minecraft.commands.arguments.blocks.BlockStateParser;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/**
 * Runs fuzzing scenarios on a dedicated server without restarting it. A batch is a JSON array of
 * scenarios dropped into {@code <dir>/inbox/<name>.json}; each case gets its own area, all cases of
 * a batch run at once, and the snapshots are written to {@code <dir>/outbox/<name>.json} in the
 * batch's order. The directory is {@code $FUZZ_DIR}, or {@code fuzz} in the server's directory.
 */
@Mod("createfuzz")
public class CreateFuzz {
	static final Gson GSON = new GsonBuilder().disableHtmlEscaping().create();
	/** Cases sit on a grid, this far apart, at this height (above a flat world's floor). */
	static final int SPACING = 64, Y = 0, CLEAR_RADIUS = 24, CLEAR_HEIGHT = 24;

	final Path dir;
	Batch batch;
	int poll;

	public CreateFuzz(IEventBus modBus) {
		String env = System.getenv("FUZZ_DIR");
		dir = Path.of(env != null ? env : "fuzz");
		NeoForge.EVENT_BUS.addListener(this::started);
		NeoForge.EVENT_BUS.addListener(this::tick);
	}

	void started(ServerStartedEvent e) {
		try {
			Files.createDirectories(dir.resolve("inbox"));
			Files.createDirectories(dir.resolve("outbox"));
			// Run ticks as fast as the server can.
			e.getServer().tickRateManager().setTickRate(10000f);
			Files.writeString(dir.resolve("ready"), "ready\n");
		} catch (IOException ex) {
			throw new RuntimeException(ex);
		}
	}

	void tick(ServerTickEvent.Post e) {
		MinecraftServer server = e.getServer();
		ServerLevel level = server.overworld();
		if (batch == null) {
			if (poll++ % 5 != 0)
				return;
			Path next = nextInbox();
			if (next == null)
				return;
			try {
				JsonArray cases = JsonParser.parseString(Files.readString(next)).getAsJsonArray();
				Files.delete(next);
				batch = new Batch(next.getFileName().toString(), cases, level);
				batch.start(level);
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
				Path tmp = dir.resolve("outbox").resolve(batch.name + ".tmp");
				Files.writeString(tmp, GSON.toJson(batch.results), StandardCharsets.UTF_8);
				Files.move(tmp, dir.resolve("outbox").resolve(batch.name), StandardCopyOption.ATOMIC_MOVE);
			} catch (IOException ex) {
				throw new RuntimeException(ex);
			}
			batch.clear(level);
			batch = null;
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
		if (batch != null)
			batch.clear(level);
		batch = null;
	}

	/** One batch's cases, run side by side. */
	static class Batch {
		final String name;
		final List<JsonObject> cases = new ArrayList<>();
		final List<BlockPos> origins = new ArrayList<>();
		final JsonArray results = new JsonArray();
		int tick, maxTicks;

		Batch(String name, JsonArray in, ServerLevel level) {
			this.name = name;
			int side = (int) Math.ceil(Math.sqrt(in.size()));
			for (int i = 0; i < in.size(); i++) {
				cases.add(in.get(i).getAsJsonObject());
				origins.add(new BlockPos((i % side) * SPACING, Y, (i / side) * SPACING));
				maxTicks = Math.max(maxTicks, in.get(i).getAsJsonObject().get("ticks").getAsInt());
			}
		}

		void start(ServerLevel level) {
			for (BlockPos o : origins) {
				for (int cx = (o.getX() - CLEAR_RADIUS) >> 4; cx <= (o.getX() + CLEAR_RADIUS) >> 4; cx++)
					for (int cz = (o.getZ() - CLEAR_RADIUS) >> 4; cz <= (o.getZ() + CLEAR_RADIUS) >> 4; cz++)
						level.setChunkForced(cx, cz, true);
			}
			clear(level);
			for (int i = 0; i < cases.size(); i++)
				for (JsonElement b : cases.get(i).getAsJsonArray("blocks"))
					place(level, origins.get(i), b.getAsJsonObject());
		}

		/** One tick of every case; true when all are done. */
		boolean step(ServerLevel level) {
			for (int i = 0; i < cases.size(); i++) {
				JsonObject c = cases.get(i);
				if (!c.has("events"))
					continue;
				for (JsonElement ev : c.getAsJsonArray("events")) {
					JsonObject o = ev.getAsJsonObject();
					if (o.get("tick").getAsInt() == tick)
						place(level, origins.get(i), o);
				}
			}
			tick++;
			if (tick < maxTicks)
				return false;
			for (int i = 0; i < cases.size(); i++)
				results.add(snapshot(level, origins.get(i), cases.get(i)));
			return true;
		}

		void clear(ServerLevel level) {
			BlockState air = Blocks.AIR.defaultBlockState();
			for (BlockPos o : origins)
				for (BlockPos p : BlockPos.betweenClosed(o.offset(-CLEAR_RADIUS, 0, -CLEAR_RADIUS),
					o.offset(CLEAR_RADIUS, CLEAR_HEIGHT, CLEAR_RADIUS)))
					if (!level.getBlockState(p).isAir())
						level.setBlock(p, air, 2 | 16);
		}

		static BlockPos at(BlockPos origin, JsonObject b) {
			JsonArray p = b.getAsJsonArray("pos");
			return origin.offset(p.get(0).getAsInt(), p.get(1).getAsInt(), p.get(2).getAsInt());
		}

		static void place(ServerLevel level, BlockPos origin, JsonObject b) {
			BlockPos pos = at(origin, b);
			BlockState state;
			try {
				state = BlockStateParser.parseForBlock(BuiltInRegistries.BLOCK.asLookup(), b.get("state").getAsString(), false)
					.blockState();
			} catch (Exception ex) {
				throw new RuntimeException("bad state " + b.get("state"), ex);
			}
			level.setBlock(pos, state, 3);
			if (b.has("motor") && level.getBlockEntity(pos) instanceof CreativeMotorBlockEntity m)
				m.generatedSpeed.setValue(b.get("motor").getAsInt());
		}

		static JsonObject snapshot(ServerLevel level, BlockPos origin, JsonObject c) {
			JsonObject blocks = new JsonObject();
			List<JsonObject> all = new ArrayList<>();
			for (JsonElement b : c.getAsJsonArray("blocks"))
				all.add(b.getAsJsonObject());
			if (c.has("events"))
				for (JsonElement b : c.getAsJsonArray("events"))
					all.add(b.getAsJsonObject());
			for (JsonObject b : all) {
				JsonArray rel = b.getAsJsonArray("pos");
				String key = rel.get(0).getAsInt() + "," + rel.get(1).getAsInt() + "," + rel.get(2).getAsInt();
				BlockPos pos = at(origin, b);
				BlockState state = level.getBlockState(pos);
				if (state.isAir())
					continue;
				JsonObject snap = new JsonObject();
				snap.addProperty("state", BlockStateParser.serialize(state));
				BlockEntity be = level.getBlockEntity(pos);
				if (be instanceof KineticBlockEntity k)
					snap.add("kinetic", kinetic(k));
				blocks.add(key, snap);
			}
			JsonObject out = new JsonObject();
			out.addProperty("side", "java");
			out.add("blocks", blocks);
			return out;
		}

		static JsonObject kinetic(KineticBlockEntity k) {
			JsonObject o = new JsonObject();
			o.addProperty("speed", k.getSpeed());
			o.addProperty("theoretical", k.getTheoreticalSpeed());
			o.addProperty("overstressed", k.isOverStressed());
			o.addProperty("stress", field(k, "stress"));
			o.addProperty("capacity", field(k, "capacity"));
			o.addProperty("impact", k.calculateStressApplied());
			o.addProperty("added_capacity", k.calculateAddedStressCapacity());
			o.addProperty("generated", k.getGeneratedSpeed());
			return o;
		}

		static float field(KineticBlockEntity k, String name) {
			try {
				Field f = KineticBlockEntity.class.getDeclaredField(name);
				f.setAccessible(true);
				return f.getFloat(k);
			} catch (ReflectiveOperationException ex) {
				throw new RuntimeException(ex);
			}
		}
	}
}
