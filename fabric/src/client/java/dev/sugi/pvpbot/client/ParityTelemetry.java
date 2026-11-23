// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client;

import com.google.gson.Gson;
import java.io.BufferedWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.player.RemotePlayer;
import net.minecraft.network.protocol.game.ClientboundEntityPositionSyncPacket;
import net.minecraft.network.protocol.game.ClientboundMoveEntityPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetHealthPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.network.protocol.Packet;
import net.minecraft.world.phys.EntityHitResult;
import net.minecraft.world.entity.ai.attributes.Attributes;

/** Separate diagnostic stream: packet application and local physics never share a state channel. */
public final class ParityTelemetry {
    private static final String DIRECTORY = System.getProperty("pvp.parity.telemetry");
    private static final Gson JSON = new Gson();
    private static BufferedWriter writer;
    private static long sequence;
    private static long tick;
    private static boolean closed;

    private ParityTelemetry() {}

    static long tickIndex() { return tick; }

    public static void afterTick(LocalPlayer player) {
        if (DIRECTORY == null || closed) return;
        emit("local_post_movement", Map.of("entity_id", player.getId(), "state", ParityCapture.initial(player)));
        tick++;
        try {
            if (Files.exists(Path.of(DIRECTORY, "telemetry-stop.json"))) {
                emit("end", Map.of("status", "complete", "dropped", 0));
                if (closed) return;
                writer.close(); closed = true;
                Files.writeString(Path.of(DIRECTORY, "telemetry-complete.json"), "{\"status\":\"complete\"}");
            }
        } catch (Exception e) { fail(e); }
    }

    /** RETURN runs after main-thread dispatch and vanilla lerpMotion, not on Netty receipt. */
    public static void motionApplied(ClientboundSetEntityMotionPacket packet) {
        if (DIRECTORY == null || closed) return;
        var player = Minecraft.getInstance().player;
        if (player == null) return;
        var v = packet.movement();
        if (packet.id() != player.getId()) {
            remotePacket("remote_motion_applied", player.level().getEntity(packet.id()), Map.of("decoded_velocity", v));
            return;
        }
        emit("self_motion_applied", Map.of("entity_id", packet.id(),
                "decoded_velocity", Map.of("x", v.x, "y", v.y, "z", v.z),
                "state", ParityCapture.initial(player)));
    }

    public static void relativeApplied(ClientboundMoveEntityPacket packet) {
        if (DIRECTORY == null || closed) return;
        var level = Minecraft.getInstance().level;
        if (level == null) return;
        var fields = new LinkedHashMap<String, Object>();
        fields.put("on_ground", packet.isOnGround());
        if (packet.hasPosition()) fields.put("delta", List.of(packet.getXa(), packet.getYa(), packet.getZa()));
        if (packet.hasRotation()) fields.put("rotation", List.of(packedAngle(packet.getYRot()), packedAngle(packet.getXRot())));
        remotePacket("remote_relative_applied", packet.getEntity(level), fields);
    }

    private static byte packedAngle(float angle) { return (byte) Math.round(angle * (256.0f / 360.0f)); }

    public static void positionSyncApplied(ClientboundEntityPositionSyncPacket packet) {
        if (DIRECTORY == null || closed) return;
        var level = Minecraft.getInstance().level;
        if (level != null) remotePacket("remote_position_sync_applied", level.getEntity(packet.id()),
                Map.of("position", packet.values().position(), "velocity", packet.values().deltaMovement(),
                        "yaw", packet.values().yRot(), "pitch", packet.values().xRot(), "on_ground", packet.onGround()));
    }

    public static void metadataApplied(ClientboundSetEntityDataPacket packet) {
        if (DIRECTORY == null || closed) return;
        var level = Minecraft.getInstance().level;
        var local = Minecraft.getInstance().player;
        if (local != null && packet.id() == local.getId()) {
            emit("self_metadata_applied", Map.of("entries", packet.packedItems().stream().map(item -> item.id()).toList(),
                    "state", ParityCapture.initial(local)));
        }
        if (level != null) remotePacket("remote_metadata_applied", level.getEntity(packet.id()),
                Map.of("entries", packet.packedItems().stream().map(item -> item.id()).toList()));
    }

    public static void attributesApplied(ClientboundUpdateAttributesPacket packet) {
        if (DIRECTORY == null || closed) return;
        var local = Minecraft.getInstance().player;
        if (local != null && packet.getEntityId() == local.getId()) {
            emit("self_attributes_applied", Map.of("speed", local.getAttributeValue(Attributes.MOVEMENT_SPEED),
                    "sprinting", local.isSprinting(), "updates_movement_speed",
                    packet.getValues().stream().anyMatch(value -> value.attribute().equals(Attributes.MOVEMENT_SPEED))));
        }
    }

    public static void beforeTick(LocalPlayer player) {
        if (DIRECTORY == null || closed) return;
        emit("local_pre_movement", Map.of("speed", player.getAttributeValue(Attributes.MOVEMENT_SPEED),
                "state", ParityCapture.initial(player)));
    }

    public static void healthApplied(ClientboundSetHealthPacket packet) {
        if (DIRECTORY == null || closed) return;
        emit("self_health_applied", Map.of("health", packet.getHealth()));
    }

    public static void remoteTick(RemotePlayer player, int velocitySteps) {
        if (DIRECTORY == null || closed) return;
        remotePacket("remote_post_tick", player, Map.of("interpolating_position", player.isInterpolating(),
                "interpolating_velocity", velocitySteps != 0));
    }

    private static void remotePacket(String event, net.minecraft.world.entity.Entity entity, Map<String, ?> data) {
        if (!(entity instanceof RemotePlayer player)) return;
        emit(event, Map.of("entity_id", player.getId(), "profile", player.getGameProfile().name(), "packet", data,
                "state", ParityCapture.motion(player), "position_codec_base", player.getPositionCodec().getBase(),
                "health", player.getHealth()));
    }

    public static void aim(Minecraft mc, float partialTick, boolean attack) {
        if (DIRECTORY == null || closed || mc.player == null || mc.hitResult == null) return;
        var fields = new LinkedHashMap<String, Object>();
        fields.put("kind", mc.hitResult.getType().name());
        fields.put("point", mc.hitResult.getLocation());
        fields.put("eye", mc.player.getEyePosition(partialTick));
        fields.put("partial_tick", partialTick);
        if (attack) {
            fields.put("local", ParityCapture.motion(mc.player));
            fields.put("replicas", mc.level.players().stream().filter(RemotePlayer.class::isInstance)
                    .map(player -> Map.of("id", player.getId(), "state", ParityCapture.motion(player))).toList());
        }
        if (mc.hitResult instanceof EntityHitResult hit) fields.put("picked_entity_id", hit.getEntity().getId());
        emit(attack ? "attack_input_pick" : "client_pick", fields);
    }

    public static void movementSubmitted(LocalPlayer player, Packet<?> packet) {
        if (DIRECTORY == null || closed || !(packet instanceof ServerboundMovePlayerPacket move)) return;
        var fields = new LinkedHashMap<String, Object>();
        fields.put("on_ground", move.isOnGround());
        fields.put("horizontal_collision", move.horizontalCollision());
        if (move.hasPosition()) fields.put("position", List.of(move.getX(0), move.getY(0), move.getZ(0)));
        if (move.hasRotation()) fields.put("rotation", List.of(move.getYRot(0), move.getXRot(0)));
        emit("movement_submitted", Map.of("entity_id", player.getId(), "packet", fields));
    }

    private static void emit(String event, Object data) {
        try {
            if (writer == null) {
                writer = Files.newBufferedWriter(Path.of(DIRECTORY, "telemetry.ndjson"));
                writer.write(JSON.toJson(Map.of("schema", 1, "event", "manifest",
                        "engine", "minecraft-client-26.1.2", "clock", "process-monotonic-and-epoch-ms",
                        "scope", "client-motion-and-observations")));
                writer.newLine();
            }
            writer.write(JSON.toJson(Map.of("schema", 1, "event", event, "sequence", sequence++,
                    "client_tick", tick, "monotonic_ns", System.nanoTime(), "epoch_ms", System.currentTimeMillis(), "data", data)));
            writer.newLine();
        } catch (Exception e) { fail(e); }
    }

    private static void fail(Exception e) {
        closed = true;
        dev.sugi.pvpbot.PvpBot.LOGGER.error("Parity telemetry failed", e);
        try {
            if (writer != null) writer.close();
            Files.writeString(Path.of(DIRECTORY, "telemetry-failed.json"), JSON.toJson(Map.of("error", e.toString())));
        } catch (Exception suppressed) { e.addSuppressed(suppressed); }
    }
}
