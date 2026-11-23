// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import io.netty.buffer.Unpooled;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;
import net.minecraft.network.LpVec3;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundAddEntityPacket;
import net.minecraft.network.protocol.game.ClientboundBundlePacket;
import net.minecraft.network.protocol.game.ClientboundEntityPositionSyncPacket;
import net.minecraft.network.protocol.game.ClientboundMoveEntityPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.VecDeltaCodec;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.phys.Vec3;
import org.bukkit.Bukkit;

/** Receives and interpolates opponent updates sent by the pinned Paper tracker. */
final class OpponentReplica {
    private static final int FLAGS = dataIndex("DATA_SHARED_FLAGS_ID");
    private static final int POSE = dataIndex("DATA_POSE");
    private final ServerLevel level;
    private final int targetId;
    private final int velocityInterval;
    private final ParityMotionInbox<Packet<?>> packets;
    private final long createdAt;
    private final VecDeltaCodec codec = new VecDeltaCodec();
    private final Consumer<Object> trace;
    private Vec3 position = Vec3.ZERO;
    private Vec3 positionTarget = Vec3.ZERO;
    private Vec3 velocity = Vec3.ZERO;
    private Vec3 velocityTarget = Vec3.ZERO;
    private Vec3 displacement = Vec3.ZERO;
    private float yaw, pitch, yawTarget, pitchTarget;
    private int positionSteps, velocitySteps;
    private boolean loaded, onGround, sprinting;
    private Pose pose = Pose.STANDING;
    private Float health;
    private long positionAt = -1, velocityAt = -1, metadataAt = -1, healthAt = -1;

    OpponentReplica(ServerPlayer target, long delayNs, Consumer<Object> trace) {
        level = target.level();
        targetId = target.getId();
        velocityInterval = target.getType().updateInterval() + 1;
        packets = new ParityMotionInbox<>(delayNs);
        createdAt = time();
        this.trace = trace;
    }

    void receive(Packet<?> packet) {
        if (!Bukkit.isPrimaryThread()) throw new IllegalStateException("Unexpected packet tracking thread");
        if (packet instanceof ClientboundBundlePacket bundle) {
            bundle.subPackets().forEach(this::receive);
            return;
        }
        boolean matches = switch (packet) {
            case ClientboundAddEntityPacket add -> add.getId() == targetId;
            case ClientboundMoveEntityPacket move -> {
                Entity entity = move.getEntity(level);
                yield entity != null && entity.getId() == targetId;
            }
            case ClientboundEntityPositionSyncPacket sync -> sync.id() == targetId;
            case ClientboundSetEntityMotionPacket motion -> motion.id() == targetId;
            case ClientboundSetEntityDataPacket data -> data.id() == targetId;
            default -> false;
        };
        if (!matches) return;
        var sent = packets.send(time(), packet);
        if (trace != null) trace.accept(Map.of("phase", "remote_packet_sent", "sequence", sent.sequence(), "server_tick", Bukkit.getCurrentTick(),
                "packet_class", packet.getClass().getSimpleName(), "payload", payload(packet),
                "send_ns", sent.sentNs(), "deadline_ns", sent.readyNs(), "monotonic_ns", System.nanoTime()));
    }

    void tick() {
        long now = time();
        ParityMotionInbox.Delivery<Packet<?>> delivery;
        while ((delivery = packets.poll(now)) != null) {
            apply(delivery.value(), now);
            if (trace != null) trace.accept(Map.of("phase", "remote_packet_applied", "sequence", delivery.sequence(),
                    "server_tick", Bukkit.getCurrentTick(), "applied_ns", now, "monotonic_ns", System.nanoTime()));
        }
        if (!loaded) return;
        Vec3 before = position;
        if (positionSteps > 0) {
            double weight = 1.0 / positionSteps--;
            position = position.lerp(positionTarget, weight);
            yaw = (float) (yaw + Mth.wrapDegrees((double) yawTarget - yaw) * weight) % 360.0f;
            pitch = (float) (pitch + ((double) pitchTarget - pitch) * weight) % 360.0f;
        }
        if (velocitySteps > 0) {
            Vec3 delta = velocityTarget.subtract(velocity);
            velocity = velocity.add(delta.x / velocitySteps, delta.y / velocitySteps, delta.z / velocitySteps);
            velocitySteps--;
        }
        displacement = position.subtract(before);
        if (trace == null) return;
        var state = new LinkedHashMap<String, Object>();
        state.put("phase", "remote_replica_tick");
        state.put("server_tick", Bukkit.getCurrentTick());
        state.put("monotonic_ns", System.nanoTime());
        state.put("position", vector(position));
        state.put("velocity", vector(velocity));
        state.put("displacement", vector(displacement));
        state.put("rotation", List.of(yaw, pitch));
        state.put("on_ground", onGround);
        state.put("sprinting", sprinting);
        state.put("sneaking", pose == Pose.CROUCHING);
        state.put("pose", pose.name());
        if (health != null) state.put("health", health);
        if (positionAt >= 0) state.put("position_age_ns", now - positionAt);
        if (velocityAt >= 0) state.put("velocity_age_ns", now - velocityAt);
        if (metadataAt >= 0) state.put("metadata_age_ns", now - metadataAt);
        if (healthAt >= 0) state.put("health_age_ns", now - healthAt);
        state.put("velocity_age_since_pairing_ns", now - createdAt);
        trace.accept(state);
    }

    private void apply(Packet<?> packet, long now) {
        switch (packet) {
            case ClientboundAddEntityPacket add -> {
                if (add.getId() != targetId || loaded) throw new IllegalStateException("Unexpected replica spawn packet");
                loaded = true;
                position = positionTarget = new Vec3(add.getX(), add.getY(), add.getZ());
                codec.setBase(position);
                yaw = yawTarget = add.getYRot();
                pitch = pitchTarget = add.getXRot();
                velocity = velocityTarget = decodedMotion(add.getMovement());
                positionAt = velocityAt = now;
            }
            case ClientboundMoveEntityPacket move -> {
                if (move.hasPosition()) codec.setBase(codec.decode(move.getXa(), move.getYa(), move.getZa()));
                interpolate(move.hasPosition() ? codec.getBase() : positionTarget,
                        move.hasRotation() ? move.getYRot() : yawTarget, move.hasRotation() ? move.getXRot() : pitchTarget);
                onGround = move.isOnGround();
                positionAt = now;
            }
            case ClientboundEntityPositionSyncPacket sync -> {
                var values = sync.values();
                codec.setBase(values.position());
                interpolate(values.position(), values.yRot(), values.xRot());
                onGround = sync.onGround();
                positionAt = now;
            }
            case ClientboundSetEntityMotionPacket motion -> {
                velocityTarget = decodedMotion(motion.movement());
                velocitySteps = velocityInterval;
                velocityAt = now;
            }
            case ClientboundSetEntityDataPacket data -> {
                for (var item : data.packedItems()) {
                    if (item.id() == FLAGS) {
                        byte flags = (Byte) item.value();
                        sprinting = (flags & 8) != 0;
                    }
                    if (item.id() == POSE) pose = (Pose) item.value();
                    if (item.id() == LivingEntity.DATA_HEALTH_ID.id()) {
                        health = (Float) item.value();
                        healthAt = now;
                    }
                }
                metadataAt = now;
            }
            default -> throw new IllegalStateException("Invalid replica packet");
        }
    }

    record Frame(Vec3 position, Vec3 velocity, Vec3 displacement, Pose pose, boolean sprinting,
                 boolean sneaking, boolean onGround, float health, long positionAge, long velocityAge,
                 long metadataAge, long healthAge) {}

    Frame frame() {
        if (!loaded || health == null) return null;
        long now = time();
        return new Frame(position, velocity, displacement, pose, sprinting, pose == Pose.CROUCHING, onGround,
                health, now - positionAt, now - (velocityAt < 0 ? createdAt : velocityAt),
                now - metadataAt, now - healthAt);
    }

    private void interpolate(Vec3 at, float atYaw, float atPitch) {
        if (at.equals(positionTarget) && atYaw == yawTarget && atPitch == pitchTarget) return;
        positionTarget = at;
        yawTarget = atYaw;
        pitchTarget = atPitch;
        positionSteps = 3;
    }

    private static Object payload(Packet<?> packet) {
        return switch (packet) {
            case ClientboundAddEntityPacket add -> Map.of("position", List.of(add.getX(), add.getY(), add.getZ()),
                    "rotation", List.of(add.getYRot(), add.getXRot()));
            case ClientboundMoveEntityPacket move -> Map.of("has_position", move.hasPosition(), "has_rotation", move.hasRotation(),
                    "delta", List.of(move.getXa(), move.getYa(), move.getZa()), "on_ground", move.isOnGround(),
                    "rotation", List.of(move.getYRot(), move.getXRot()));
            case ClientboundEntityPositionSyncPacket sync -> Map.of("position", vector(sync.values().position()),
                    "rotation", List.of(sync.values().yRot(), sync.values().xRot()), "on_ground", sync.onGround());
            case ClientboundSetEntityMotionPacket motion -> Map.of("raw_velocity", vector(motion.movement()));
            case ClientboundSetEntityDataPacket data -> Map.of("metadata_ids", data.packedItems().stream().map(item -> item.id()).toList());
            default -> throw new IllegalStateException("Invalid replica packet");
        };
    }

    private static Vec3 decodedMotion(Vec3 motion) {
        var buffer = Unpooled.buffer();
        try {
            LpVec3.write(buffer, motion);
            return LpVec3.read(buffer);
        } finally { buffer.release(); }
    }
    private static long time() { return (long) Bukkit.getCurrentTick() * BotPing.TICK_NS; }
    private static List<Double> vector(Vec3 value) { return List.of(value.x, value.y, value.z); }

    private static int dataIndex(String field) {
        try {
            var accessor = Entity.class.getDeclaredField(field);
            accessor.setAccessible(true);
            return ((EntityDataAccessor<?>) accessor.get(null)).id();
        } catch (ReflectiveOperationException e) { throw new ExceptionInInitializerError(e); }
    }
}
