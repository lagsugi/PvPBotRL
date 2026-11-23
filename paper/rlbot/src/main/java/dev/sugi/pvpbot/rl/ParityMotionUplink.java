// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import dev.sugi.pvpbot.rl.policy.Action;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.network.protocol.game.ServerboundAcceptTeleportationPacket;
import net.minecraft.network.protocol.game.ServerboundClientTickEndPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerCommandPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerInputPacket;
import net.minecraft.network.protocol.game.ServerGamePacketListener;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.phys.Vec3;
import org.bukkit.Bukkit;

/** Independent flat-ground client motion, feeding Paper's normal server handlers. */
final class ParityMotionUplink {
    private static final VarHandle AWAITING_POSITION = connectionField("awaitingPositionFromClient", Vec3.class);
    private static final VarHandle AWAITING_TELEPORT = connectionField("awaitingTeleport", int.class);
    private final ServerPlayer server;
    private final double floor;
    private final ParityMotionInbox<Packet<ServerGamePacketListener>> queue;
    private final List<Object> events = new ArrayList<>();
    private final boolean physicalInput;
    private final float yaw, pitch;
    private final float sneakSpeed;
    private Vec3 position;
    private Vec3 velocity;
    private Vec3 lastPosition;
    private boolean onGround;
    private boolean lastGround;
    private Input input = Input.EMPTY;
    private boolean sprinting, lastSprint;
    private int jumpDelay;
    private double fallDistance;
    private float movementSpeed;
    private float health;
    private long healthReceived;
    private int attackTicks;
    private double attackSpeed;
    private int reminder;

    ParityMotionUplink(ServerPlayer server, long delayNs, boolean physicalInput, int initialAttackTicks) {
        if (!server.onGround()) throw new IllegalStateException("Expected a grounded victim before the single-hit test");
        this.server = server;
        this.physicalInput = physicalInput;
        // The virtual client starts from the already-issued spawn teleport.
        // Acknowledge it through the same handler before sending movement.
        if (AWAITING_POSITION.get(server.connection) instanceof Vec3 pending) {
            if (!pending.equals(server.position())) throw new IllegalStateException("Spawn teleport does not match the capture boundary");
            int id = (int) AWAITING_TELEPORT.get(server.connection);
            server.connection.handleAcceptTeleportPacket(new ServerboundAcceptTeleportationPacket(id));
            if (AWAITING_POSITION.get(server.connection) != null) throw new IllegalStateException("Spawn teleport was not acknowledged");
        }
        if (!server.connection.hasClientLoaded()) throw new IllegalStateException("Bot play connection is not loaded");
        position = lastPosition = server.position();
        velocity = server.getDeltaMovement();
        onGround = lastGround = server.onGround();
        yaw = server.getYRot();
        pitch = server.getXRot();
        sneakSpeed = (float) server.getAttributeValue(Attributes.SNEAKING_SPEED);
        sprinting = lastSprint = server.isSprinting();
        movementSpeed = (float) server.getAttributeValue(Attributes.MOVEMENT_SPEED);
        health = server.getHealth();
        healthReceived = (long) Bukkit.getCurrentTick() * BotPing.TICK_NS;
        attackTicks = initialAttackTicks;
        attackSpeed = server.getAttributeValue(Attributes.ATTACK_SPEED);
        floor = position.y;
        queue = new ParityMotionInbox<>(delayNs);
        if (physicalInput) {
            events.add(Map.of("event", "client_motion_initial", "server_tick", Bukkit.getCurrentTick(),
                    "monotonic_ns", System.nanoTime(), "state", motionState()));
        }
    }

    Vec3 velocity() { return velocity; }

    record SelfMotion(Vec3 position, Vec3 velocity, float yaw, float pitch, boolean grounded,
                      boolean sprinting, boolean crouching, int jumpDelay, double fallDistance,
                      float health, long healthAge, float charge) {}

    SelfMotion selfMotion() {
        long now = (long) Bukkit.getCurrentTick() * BotPing.TICK_NS;
        return physicalInput ? new SelfMotion(position, velocity, yaw, pitch, onGround,
                sprinting, input.shift(), jumpDelay, fallDistance, health, now - healthReceived,
                Math.min(1.0F, (attackTicks + 0.5F) / (float) (20.0 / attackSpeed))) : null;
    }

    void impulse(Vec3 value) { velocity = value; }
    void sprintMetadata(boolean value) { sprinting = value; }
    void movementSpeed(float value) { movementSpeed = value; }
    void attackSpeed(double value) { attackSpeed = value; }
    void health(float value, long now) { health = value; healthReceived = now; }
    void localSwing() { attackTicks = 0; }

    void feedbackEvent(String event, long sequence, long now, long ready, Map<String, ?> value) {
        events.add(Map.of("event", event, "sequence", sequence, "server_tick", Bukkit.getCurrentTick(),
                "logical_ns", now, "ready_ns", ready, "monotonic_ns", System.nanoTime(), "value", value));
    }

    void applyDue(long now) {
        ParityMotionInbox.Delivery<Packet<ServerGamePacketListener>> delivery;
        while ((delivery = queue.poll(now)) != null) {
            if (!(delivery.value() instanceof ServerboundMovePlayerPacket move)) {
                delivery.value().handle(server.connection);
                events.add(Map.of("event", "input_apply", "sequence", delivery.sequence(), "server_tick", Bukkit.getCurrentTick(),
                        "applied_ns", now, "ready_ns", delivery.readyNs(), "monotonic_ns", System.nanoTime(),
                        "packet", delivery.value().getClass().getSimpleName()));
                continue;
            }
            Vec3 before = server.position();
            Object connectionBefore = ParityPlayerConnection.movementState(server.connection);
            server.connection.handleMovePlayer(move);
            events.add(Map.ofEntries(Map.entry("event", "movement_apply"), Map.entry("sequence", delivery.sequence()),
                    Map.entry("server_tick", Bukkit.getCurrentTick()), Map.entry("applied_ns", now), Map.entry("ready_ns", delivery.readyNs()),
                    Map.entry("position_before", vector(before)), Map.entry("position_after", vector(server.position())),
                    Map.entry("on_ground", server.onGround()), Map.entry("monotonic_ns", System.nanoTime()),
                    Map.entry("phase", "before_server_travel"), Map.entry("connection_before", connectionBefore),
                    Map.entry("connection_after", ParityPlayerConnection.movementState(server.connection))));
        }
    }

    void predictAndSend(long now, Action.Keys keys) {
        if (physicalInput) {
            events.add(Map.of("event", "client_pre_movement", "server_tick", Bukkit.getCurrentTick(),
                    "monotonic_ns", System.nanoTime(), "sprinting", sprinting, "movement_speed", movementSpeed));
        }
        boolean stopped = velocity.horizontalDistanceSqr() < 9e-6;
        double x = stopped ? 0 : velocity.x;
        double y = Math.abs(velocity.y) < 0.003 ? 0 : velocity.y;
        double z = stopped ? 0 : velocity.z;
        float friction = onGround ? 0.6f : 1.0f;
        float drag = friction * 0.91f;
        var desired = new Vec3(x, y, z);
        if (physicalInput) desired = applyKeys(now, desired, friction, keys);
        double dy = desired.y < 0 && position.y + desired.y < floor ? floor - position.y : desired.y;
        var movement = new Vec3(desired.x, dy, desired.z);
        double moved = movement.lengthSqr();
        if (moved > 1e-7 || desired.lengthSqr() - moved < 1e-7) position = position.add(movement);
        onGround = dy != desired.y && desired.y < 0;
        velocity = new Vec3(desired.x * drag, ((dy != desired.y ? 0 : desired.y) - 0.08) * 0.98f, desired.z * drag);
        if (dy < 0) fallDistance -= (float) dy;
        if (onGround) fallDistance = 0;

        boolean movedEnough = ++reminder >= 20 || position.distanceToSqr(lastPosition) > 2e-4 * 2e-4;
        sendPosition(now, movedEnough);
        if (physicalInput) {
            attackTicks++;
            sendInput(now, ServerboundClientTickEndPacket.INSTANCE);
            var own = selfMotion();
            events.add(Map.of("event", "client_post_movement", "server_tick", Bukkit.getCurrentTick(),
                    "monotonic_ns", System.nanoTime(), "keys", keys, "state", motionState(),
                    "health", own.health(), "health_age_ns", own.healthAge(), "attack_charge", own.charge()));
        }
    }

    private Map<String, ?> motionState() {
        return Map.of("position", vector(position), "velocity", vector(velocity), "on_ground", onGround,
                "sprinting", sprinting, "pose", input.shift() ? "crouching" : "standing", "jump_delay", jumpDelay,
                "fall_distance", fallDistance, "yaw", yaw, "pitch", pitch);
    }

    private Vec3 applyKeys(long now, Vec3 desired, float friction, Action.Keys keys) {
        boolean crouching = input.shift();
        Input next = new Input(keys.forward(), keys.backward(), keys.left(), keys.right(), keys.jump(), keys.sneak(), true);
        int forward = Boolean.compare(keys.forward(), keys.backward());
        int sideways = Boolean.compare(keys.left(), keys.right());
        boolean sprintBeforeInput = sprinting;
        if (forward <= 0) sprinting = false;
        else if (!crouching) sprinting = true;
        if (sprinting != sprintBeforeInput) movementSpeed = sprinting ? (float) (0.1f * (1.0 + 0.3f)) : 0.1f;
        if (!next.equals(input)) {
            input = next;
            sendInput(now, new ServerboundPlayerInputPacket(next));
        }
        if (sprinting != lastSprint) {
            lastSprint = sprinting;
            sendInput(now, new ServerboundPlayerCommandPacket(server, sprinting
                    ? ServerboundPlayerCommandPacket.Action.START_SPRINTING : ServerboundPlayerCommandPacket.Action.STOP_SPRINTING));
        }
        float angle = yaw * ((float) Math.PI / 180.0f);
        double sin = Mth.sin(angle), cos = Mth.cos(angle);
        jumpDelay = keys.jump() ? Math.max(0, jumpDelay - 1) : 0;
        if (keys.jump() && onGround && jumpDelay == 0) {
            jumpDelay = 10;
            desired = new Vec3(desired.x, Math.max(desired.y, 0.42f), desired.z);
            if (sprinting) desired = desired.add(-sin * 0.2, 0, cos * 0.2);
        }
        float acceleration = onGround ? movementSpeed * (0.21600002f / (friction * friction * friction))
                : sprinting ? 0.025999999f : 0.02f;
        var axes = PlayerMovementInput.shape(sideways, forward, crouching, sneakSpeed);
        var direction = new Vec3(axes.sideways(), 0, axes.forward());
        double squared = direction.lengthSqr();
        if (squared < 1e-7) return desired;
        if (squared > 1.0) direction = direction.normalize();
        direction = direction.scale(acceleration);
        return desired.add(direction.x * cos - direction.z * sin, 0, direction.z * cos + direction.x * sin);
    }

    private void sendInput(long now, Packet<ServerGamePacketListener> packet) {
        var sent = queue.send(now, packet);
        events.add(Map.of("event", "input_send", "sequence", sent.sequence(), "server_tick", Bukkit.getCurrentTick(),
                "sent_ns", now, "ready_ns", sent.readyNs(), "monotonic_ns", System.nanoTime(),
                "packet", packet.getClass().getSimpleName()));
    }

    private void sendPosition(long now, boolean movedEnough) {
        if (!movedEnough && onGround == lastGround) return;
        ServerboundMovePlayerPacket packet = movedEnough
                ? new ServerboundMovePlayerPacket.Pos(position, onGround, false)
                : new ServerboundMovePlayerPacket.StatusOnly(onGround, false);
        var sent = queue.send(now, packet);
        events.add(Map.of("event", "movement_send", "sequence", sent.sequence(), "server_tick", Bukkit.getCurrentTick(),
                "sent_ns", now, "ready_ns", sent.readyNs(), "position", vector(position), "on_ground", onGround,
                "has_position", movedEnough, "monotonic_ns", System.nanoTime(), "phase", "after_client_prediction"));
        if (movedEnough) {
            lastPosition = position;
            reminder = 0;
        }
        lastGround = onGround;
    }

    List<Object> events() { return List.copyOf(events); }

    private static VarHandle connectionField(String name, Class<?> type) {
        try {
            return MethodHandles.privateLookupIn(ServerGamePacketListenerImpl.class, MethodHandles.lookup())
                    .findVarHandle(ServerGamePacketListenerImpl.class, name, type);
        } catch (ReflectiveOperationException e) { throw new ExceptionInInitializerError(e); }
    }

    private static List<Double> vector(Vec3 v) { return List.of(v.x, v.y, v.z); }
}
