// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import dev.sugi.pvpbot.rl.policy.Action;
import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.citizensnpcs.nms.v26_1_R1.entity.EntityHumanNPC;
import net.citizensnpcs.nms.v26_1_R1.network.EmptyPacketListener;
import net.minecraft.network.Connection;
import net.minecraft.network.LpVec3;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.ClientboundSetHealthPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.CommonListenerCookie;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.phys.Vec3;
import org.bukkit.Bukkit;

/** Experimental isolated-server transport for self motion and the idle victim's uplink. */
final class ParityMotionTransport extends EmptyPacketListener {
    private static final MethodHandle PLAYER_HURT = playerHurt();
    private final EntityHumanNPC owner;
    private ParityMotionInbox<Vec3> inbox;
    private final List<Object> events = new ArrayList<>();
    private ParityMotionUplink uplink;
    private ParityMotionInbox<SelfFeedback> selfFeedback;

    private enum Field { SPRINTING, MOVEMENT_SPEED, HEALTH, ATTACK_SPEED }
    private record SelfFeedback(Field field, double value) {}

    ParityMotionTransport(ServerLevel level, EntityHumanNPC owner, Connection connection) {
        super(level.getServer(), connection, owner, CommonListenerCookie.createInitial(owner.getGameProfile(), false));
        this.owner = owner;
        long delayMs = Long.getLong("pvp.parity.motionDownlinkMs", 0L);
        inbox = new ParityMotionInbox<>(Math.multiplyExact(delayMs, 1_000_000L));
    }

    // Pinned Citizens clears hurtMarked and schedules a restoration after hurtServer.
    // Invoke its direct parent implementation explicitly so Player's attack path
    // sends the real motion packet and restores pre-attack server velocity itself.
    private static MethodHandle playerHurt() {
        try {
            return MethodHandles.privateLookupIn(EntityHumanNPC.class, MethodHandles.lookup()).findSpecial(
                    ServerPlayer.class, "hurtServer", MethodType.methodType(boolean.class,
                            ServerLevel.class, DamageSource.class, float.class), EntityHumanNPC.class);
        } catch (ReflectiveOperationException e) { throw new ExceptionInInitializerError(e); }
    }

    boolean hurtServer(ServerLevel level, DamageSource source, float amount) {
        try { return (boolean) PLAYER_HURT.invokeExact(owner, level, source, amount); }
        catch (RuntimeException | Error e) { throw e; }
        catch (Throwable e) { throw new IllegalStateException("Pinned ServerPlayer damage invocation failed", e); }
    }

    @Override public void send(Packet<?> packet) {
        if (selfFeedback != null) captureSelfFeedback(packet);
        if (!(packet instanceof ClientboundSetEntityMotionPacket motion) || motion.id() != owner.getId()) return;
        if (!Bukkit.isPrimaryThread()) throw new IllegalStateException("Motion send outside server thread");
        var buffer = Unpooled.buffer();
        try {
            LpVec3.write(buffer, motion.movement());
            String encoded = ByteBufUtil.hexDump(buffer);
            Vec3 decoded = LpVec3.read(buffer);
            var delivery = inbox.send(now(), decoded);
            record(Map.of("event", "motion_send", "sequence", delivery.sequence(),
                    "server_tick", Bukkit.getCurrentTick(), "sent_ns", delivery.sentNs(), "ready_ns", delivery.readyNs(),
                    "raw_velocity", vector(motion.movement()), "lpvec3_hex", encoded,
                    "decoded_velocity", vector(decoded), "epoch_ms", System.currentTimeMillis(),
                    "monotonic_ns", System.nanoTime()));
        } finally { buffer.release(); }
    }

    private void captureSelfFeedback(Packet<?> packet) {
        if (packet instanceof ClientboundSetEntityDataPacket data && data.id() == owner.getId()) {
            for (var value : data.packedItems()) {
                // The shared Entity flags byte is entry zero on the pinned protocol.
                if (value.id() == 0) enqueueFeedback(new SelfFeedback(Field.SPRINTING, ((((Byte) value.value()) & 8) != 0) ? 1 : 0));
            }
        } else if (packet instanceof ClientboundSetHealthPacket health) {
            enqueueFeedback(new SelfFeedback(Field.HEALTH, health.getHealth()));
        } else if (packet instanceof ClientboundUpdateAttributesPacket attributes && attributes.getEntityId() == owner.getId()) {
            for (var value : attributes.getValues()) {
                Field field;
                if (value.attribute().equals(Attributes.MOVEMENT_SPEED)) field = Field.MOVEMENT_SPEED;
                else if (value.attribute().equals(Attributes.ATTACK_SPEED)) field = Field.ATTACK_SPEED;
                else continue;
                var replica = new AttributeInstance(value.attribute(), ignored -> {});
                replica.setBaseValue(value.base());
                value.modifiers().forEach(replica::addTransientModifier);
                enqueueFeedback(new SelfFeedback(field, replica.getValue()));
            }
        }
    }

    private void enqueueFeedback(SelfFeedback value) {
        if (!Bukkit.isPrimaryThread()) throw new IllegalStateException("Self feedback outside server thread");
        var sent = selfFeedback.send(now(), value);
        uplink.feedbackEvent("self_feedback_sent", sent.sequence(), sent.sentNs(), sent.readyNs(), feedbackValues(value));
    }

    private static Map<String, ?> feedbackValues(SelfFeedback value) {
        return switch (value.field()) {
            case SPRINTING -> Map.of("sprinting", value.value() != 0);
            case MOVEMENT_SPEED -> Map.of("movement_speed", value.value());
            case HEALTH -> Map.of("health", value.value());
            case ATTACK_SPEED -> Map.of("attack_speed", value.value());
        };
    }

    void beforePhysics(Action.Keys keys) {
        if (!Bukkit.isPrimaryThread()) throw new IllegalStateException("Motion application outside server thread");
        long now = now();
        if (uplink != null) uplink.applyDue(now);
        if (selfFeedback != null) {
            ParityMotionInbox.Delivery<SelfFeedback> delivery;
            while ((delivery = selfFeedback.poll(now)) != null) {
                SelfFeedback value = delivery.value();
                switch (value.field()) {
                    case SPRINTING -> uplink.sprintMetadata(value.value() != 0);
                    case MOVEMENT_SPEED -> uplink.movementSpeed((float) value.value());
                    case HEALTH -> uplink.health((float) value.value(), now);
                    case ATTACK_SPEED -> uplink.attackSpeed(value.value());
                }
                uplink.feedbackEvent("self_feedback_applied", delivery.sequence(), now, delivery.readyNs(), feedbackValues(value));
            }
        }
        ParityMotionInbox.Delivery<Vec3> delivery;
        while ((delivery = inbox.poll(now)) != null) {
            Vec3 before;
            if (uplink == null) {
                before = owner.getDeltaMovement();
                owner.setDeltaMovement(delivery.value());
            } else {
                before = uplink.velocity();
                uplink.impulse(delivery.value());
            }
            record(Map.of("event", "motion_apply", "sequence", delivery.sequence(),
                    "server_tick", Bukkit.getCurrentTick(), "applied_ns", now, "ready_ns", delivery.readyNs(),
                    "before_velocity", vector(before), "velocity", vector(delivery.value()),
                    "phase", uplink == null ? "pre_physics" : "client_pre_physics", "epoch_ms", System.currentTimeMillis(),
                    "monotonic_ns", System.nanoTime()));
        }
        // The server drains pending input before this tick's client prediction.
        if (uplink != null) uplink.predictAndSend(now, keys);
    }

    void armUplink(long delayNs, Long downlinkNs, boolean physicalInput, int initialAttackTicks) {
        if (downlinkNs != null) {
            // The diagnostic client begins at an acknowledged, settled spawn.
            // Its measured packet window starts with empty queues at this boundary.
            inbox = new ParityMotionInbox<>(downlinkNs);
            events.clear();
        }
        uplink = new ParityMotionUplink(owner, delayNs, physicalInput, initialAttackTicks);
        if (physicalInput) selfFeedback = new ParityMotionInbox<>(inbox.delayNs());
    }
    boolean separateMotion() { return uplink != null; }
    ParityMotionUplink.SelfMotion selfMotion() { return uplink == null ? null : uplink.selfMotion(); }
    void localSwing() { if (uplink != null) uplink.localSwing(); }
    List<Object> uplinkEvents() { return uplink == null ? List.of() : uplink.events(); }

    List<Object> events() { return List.copyOf(events); }
    private void record(Object event) {
        if (events.size() >= 512) throw new IllegalStateException("Motion transport diagnostic capacity exceeded");
        events.add(event);
    }
    private static long now() { return (long) Bukkit.getCurrentTick() * BotPing.TICK_NS; }
    private static List<Double> vector(Vec3 v) { return List.of(v.x, v.y, v.z); }
}
