package dev.createfuzz;

import com.google.gson.JsonObject;
import com.mojang.authlib.GameProfile;

import net.minecraft.network.Connection;
import net.minecraft.network.PacketSendListener;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.PacketFlow;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.CommonListenerCookie;
import net.minecraft.server.network.ServerGamePacketListenerImpl;

import java.util.UUID;

/** An ordinary server player; only its outbound transport is replaced. */
final class PlayerActor {
    static ServerPlayer create(ServerLevel level, int slot) {
        UUID id = UUID.randomUUID();
        var profile = new GameProfile(id, "FuzzActor" + slot);
        var player =
                new ServerPlayer(
                        level.getServer(), level, profile, ClientInformation.createDefault());
        player.connection =
                new ServerGamePacketListenerImpl(
                        level.getServer(),
                        new PacketSink(),
                        player,
                        CommonListenerCookie.createInitial(profile, false));
        if (player.getClass() != ServerPlayer.class)
            throw new IllegalStateException("scenario actor must be an ordinary ServerPlayer");
        return player;
    }

    static JsonObject probe(ServerLevel level) {
        var player = create(level, -1);
        player.displayClientMessage(Component.literal("create-fuzz transport probe"), false);
        var sink = (PacketSink) player.connection.getConnection();
        if (sink.discarded == 0)
            throw new IllegalStateException("player transport did not discard a packet");
        JsonObject result = new JsonObject();
        result.addProperty("actor_class", player.getClass().getName());
        result.addProperty("listener_class", player.connection.getClass().getName());
        result.addProperty("discarded_packets", sink.discarded);
        return result;
    }

    /** Public Connection send overloads: no channel, socket, queued packet or inbound client. */
    private static final class PacketSink extends Connection {
        int discarded;

        PacketSink() {
            super(PacketFlow.SERVERBOUND);
        }

        @Override
        public void send(Packet<?> packet) {
            send(packet, null, true);
        }

        @Override
        public void send(Packet<?> packet, PacketSendListener listener) {
            send(packet, listener, true);
        }

        @Override
        public void send(Packet<?> packet, PacketSendListener listener, boolean flush) {
            discarded++;
            if (listener != null) listener.onSuccess();
        }
    }
}
