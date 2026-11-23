// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ServerGamePacketListener;
import net.minecraft.network.protocol.game.ServerboundAttackPacket;
import net.minecraft.network.protocol.game.ServerboundSwingPacket;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import org.bukkit.Bukkit;

/** Isolated diagnostic path from the locally selected target to Paper's packet handlers. */
final class ParityAttackUplink {
    private final ServerPlayer owner;
    private final ParityMotionInbox<Packet<ServerGamePacketListener>> packets;
    private final List<Object> events = new ArrayList<>();

    ParityAttackUplink(ServerPlayer owner, long delayNs) {
        if (!owner.connection.hasClientLoaded()) throw new IllegalStateException("Bot connection is not loaded");
        this.owner = owner;
        packets = new ParityMotionInbox<>(delayNs);
    }

    void click(Integer target) {
        long now = time();
        events.add(Map.of("phase", "client_pick", "selected", target != null, "server_tick", Bukkit.getCurrentTick(),
                "monotonic_ns", System.nanoTime()));
        if (target != null) send(now, new ServerboundAttackPacket(target));
        send(now, new ServerboundSwingPacket(InteractionHand.MAIN_HAND));
    }

    private void send(long now, Packet<ServerGamePacketListener> packet) {
        var sent = packets.send(now, packet);
        events.add(Map.of("phase", "attack_uplink_sent", "sequence", sent.sequence(), "deadline_ns", sent.readyNs(),
                "send_ns", now, "server_tick", Bukkit.getCurrentTick(), "monotonic_ns", System.nanoTime(),
                "packet", packet.getClass().getSimpleName()));
    }

    void applyDue() {
        long now = time();
        ParityMotionInbox.Delivery<Packet<ServerGamePacketListener>> ready;
        while ((ready = packets.poll(now)) != null) {
            ready.value().handle(owner.connection);
            events.add(Map.of("phase", "attack_uplink_handler_return", "sequence", ready.sequence(),
                    "server_tick", Bukkit.getCurrentTick(), "applied_ns", now, "monotonic_ns", System.nanoTime(),
                    "packet", ready.value().getClass().getSimpleName()));
        }
    }

    List<Object> events() { return List.copyOf(events); }
    private static long time() { return (long) Bukkit.getCurrentTick() * BotPing.TICK_NS; }
}
