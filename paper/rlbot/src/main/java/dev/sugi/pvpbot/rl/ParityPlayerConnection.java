// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import io.netty.channel.ChannelDuplexHandler;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelPipeline;
import io.netty.channel.ChannelPromise;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.network.LpVec3;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.ServerboundAttackPacket;
import net.minecraft.network.protocol.game.ServerboundClientTickEndPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerCommandPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerInputPacket;
import net.minecraft.network.protocol.game.ServerboundSwingPacket;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import org.bukkit.craftbukkit.entity.CraftPlayer;
import org.bukkit.entity.Player;

/** Records decoded play packets at the isolated server's Netty pipeline boundary. */
final class ParityPlayerConnection extends ChannelDuplexHandler {
    private static final VarHandle JUST_TELEPORTED = listenerField("justTeleported", boolean.class);
    private static final VarHandle HAS_MOVED = listenerField("hasMoved", boolean.class);
    private static final VarHandle LAST_X = listenerField("lastPosX", double.class);
    private static final VarHandle LAST_Y = listenerField("lastPosY", double.class);
    private static final VarHandle LAST_Z = listenerField("lastPosZ", double.class);
    private static final VarHandle LAST_YAW = listenerField("lastYaw", float.class);
    private static final VarHandle LAST_PITCH = listenerField("lastPitch", float.class);
    private final int entityId;
    private final ChannelPipeline pipeline;
    private final List<Object> events = new ArrayList<>();
    private final Map<Object, ArrayDeque<Integer>> receipts = new IdentityHashMap<>();
    private int dropped;
    private boolean collecting = true;
    private volatile boolean installed;

    ParityPlayerConnection(Player player) {
        var handle = ((CraftPlayer) player).getHandle();
        entityId = handle.getId();
        pipeline = handle.connection.connection.channel.pipeline();
        pipeline.addBefore("packet_handler", "pvpbot_parity", this);
    }

    @Override public void handlerAdded(ChannelHandlerContext ctx) { installed = true; }

    @Override public void channelRead(ChannelHandlerContext ctx, Object message) throws Exception {
        if (message instanceof ServerboundMovePlayerPacket move) {
            var fields = new LinkedHashMap<String, Object>();
            fields.put("on_ground", move.isOnGround());
            fields.put("horizontal_collision", move.horizontalCollision());
            if (move.hasPosition()) fields.put("position", List.of(move.getX(0), move.getY(0), move.getZ(0)));
            if (move.hasRotation()) fields.put("rotation", List.of(move.getYRot(0), move.getXRot(0)));
            record(message, "movement_received", fields);
        } else if (message instanceof ServerboundAttackPacket attack) {
            record(message, "attack_received", Map.of("target_entity_id", attack.entityId()));
        } else if (message instanceof ServerboundSwingPacket swing) {
            record(message, "swing_received", Map.of("hand", swing.getHand().name()));
        } else if (message instanceof ServerboundPlayerInputPacket input) {
            var keys = input.input();
            record(message, "input_received", Map.of("forward", keys.forward(), "backward", keys.backward(),
                    "left", keys.left(), "right", keys.right(), "jump", keys.jump(), "sneak", keys.shift(), "sprint", keys.sprint()));
        } else if (message instanceof ServerboundPlayerCommandPacket command) {
            record(message, "command_received", Map.of("action", command.getAction().name()));
        } else if (message instanceof ServerboundClientTickEndPacket) {
            record(message, "client_tick_end_received", Map.of());
        }
        super.channelRead(ctx, message);
    }

    @Override public void write(ChannelHandlerContext ctx, Object message, ChannelPromise promise) throws Exception {
        if (message instanceof ClientboundSetEntityMotionPacket motion && motion.id() == entityId) {
            var buffer = Unpooled.buffer();
            try {
                LpVec3.write(buffer, motion.movement());
                String hex = ByteBufUtil.hexDump(buffer);
                var decoded = LpVec3.read(buffer);
                record(null, "self_motion_outbound", Map.of("lpvec3_hex", hex,
                        "velocity", List.of(decoded.x, decoded.y, decoded.z)));
            } finally { buffer.release(); }
        }
        super.write(ctx, message, promise);
    }

    private synchronized void record(Object message, String event, Object packet) {
        if (!collecting) return;
        if (events.size() == 50_000) { dropped++; return; }
        int sequence = events.size();
        if (message != null) receipts.computeIfAbsent(message, ignored -> new ArrayDeque<>()).addLast(sequence);
        events.add(Map.of("event", event, "sequence", sequence, "monotonic_ns", System.nanoTime(),
                "epoch_ms", System.currentTimeMillis(), "packet", packet));
    }

    synchronized int takeReceipt(Object packet) {
        var pending = receipts.get(packet);
        if (pending == null) return -1;
        int sequence = pending.removeFirst();
        if (pending.isEmpty()) receipts.remove(packet);
        return sequence;
    }

    synchronized Object finish() {
        collecting = false;
        if (pipeline.context(this) != null) pipeline.remove(this);
        return Map.of("schema", 1, "phase", "server-netty-decoded", "entity_id", entityId,
                "installed", installed, "dropped", dropped, "events", List.copyOf(events));
    }

    static Object movementState(ServerGamePacketListenerImpl connection) {
        return Map.of("just_teleported", JUST_TELEPORTED.get(connection),
                "has_moved", HAS_MOVED.get(connection),
                "last_move_event_position", List.of(LAST_X.get(connection), LAST_Y.get(connection), LAST_Z.get(connection)),
                "last_move_event_rotation", List.of(LAST_YAW.get(connection), LAST_PITCH.get(connection)));
    }

    private static VarHandle listenerField(String name, Class<?> type) {
        try {
            return MethodHandles.privateLookupIn(ServerGamePacketListenerImpl.class, MethodHandles.lookup())
                    .findVarHandle(ServerGamePacketListenerImpl.class, name, type);
        } catch (ReflectiveOperationException e) { throw new ExceptionInInitializerError(e); }
    }
}
