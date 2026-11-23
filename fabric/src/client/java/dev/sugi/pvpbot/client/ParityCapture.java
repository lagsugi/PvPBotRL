// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import dev.sugi.pvpbot.client.mixin.LivingAccess;
import dev.sugi.pvpbot.client.mixin.PlayerAccess;
import dev.sugi.pvpbot.client.mixin.LocalAccess;
import dev.sugi.pvpbot.client.mixin.KeyAccess;
import java.io.BufferedWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.phys.Vec3;

/** Opt-in input recording. No movement, damage, or tick implementation is replaced. */
public final class ParityCapture {
    private static final Gson JSON = new Gson();
    private static final String DIRECTORY = System.getProperty("pvp.parity.dir");
    private static String lastRequest = "";
    private static JsonObject scenario;
    private static JsonArray actions;
    private static Path output;
    private static BufferedWriter writer;
    private static int index;
    private static int warmup;
    private static long started;
    private static boolean armed;

    private ParityCapture() {}

    public static void beforeTick(Minecraft mc) {
        if (DIRECTORY == null || mc.player == null || mc.level == null) return;
        try {
            Path directory = Path.of(DIRECTORY).toAbsolutePath();
            Path request = directory.resolve("request.json");
            if (scenario == null && Files.exists(request)) {
                String text = Files.readString(request);
                if (!text.equals(lastRequest)) {
                    lastRequest = text;
                    scenario = JsonParser.parseString(text).getAsJsonObject();
                    if (scenario.get("schema").getAsInt() != 1) throw new IllegalArgumentException("scenario schema");
                    String id = scenario.get("id").getAsString();
                    if (!id.matches("[a-zA-Z0-9_-]+")) throw new IllegalArgumentException("scenario id");
                    actions = scenario.getAsJsonArray("actions");
                    if (actions.isEmpty()) throw new IllegalArgumentException("empty actions");
                    output = directory.resolve(id);
                    Files.createDirectories(output);
                    Files.writeString(output.resolve("scenario.json"), text);
                    index = 0;
                    warmup = scenario.has("warmup") ? scenario.get("warmup").getAsInt() : 40;
                    mc.options.pauseOnLostFocus = false;
                    mc.options.autoJump().set(false);
                    mc.options.enableVsync().set(false);
                    mc.options.framerateLimit().set(scenario.has("fps") ? scenario.get("fps").getAsInt() : 60);
                    mc.setScreen(null);
                }
            }
            if (scenario == null) return;
            if (scenario.has("start_epoch_ms") && System.currentTimeMillis() < scenario.get("start_epoch_ms").getAsLong()) {
                keys(mc, new JsonObject());
                return;
            }
            if (warmup-- > 0) {
                keys(mc, new JsonObject());
                return;
            }
            if (writer == null) {
                String initial = JSON.toJson(initial(mc.player));
                Files.writeString(output.resolve("initial.json"), initial);
                writer = Files.newBufferedWriter(output.resolve("client.ndjson"));
                started = System.nanoTime();
                emit(Map.of("schema", 1, "event", "manifest", "engine", "minecraft-client-26.1.2",
                        "scenario", scenario.get("id").getAsString(), "scenario_sha256", hash(lastRequest),
                        "initial_sha256", hash(initial), "expected_samples", actions.size(),
                        "first_input_epoch_ms", System.currentTimeMillis(), "first_input_monotonic_ns", started));
            }
            JsonObject action = actions.get(index).getAsJsonObject();
            keys(mc, action.has("keys") ? action.getAsJsonObject("keys") : new JsonObject());
            if (action.has("yaw")) mc.player.setYRot(action.get("yaw").getAsFloat());
            if (action.has("pitch")) mc.player.setXRot(action.get("pitch").getAsFloat());
            mc.options.keyAttack.setDown(flag(action, "attack_held"));
            ((KeyAccess) mc.options.keyAttack).parity$clicks(flag(action, "click") ? 1 : 0);
            armed = true;
        } catch (Exception e) { fail(e); }
    }

    private static void keys(Minecraft mc, JsonObject k) {
        mc.options.keyUp.setDown(flag(k, "forward"));
        mc.options.keyDown.setDown(flag(k, "backward"));
        mc.options.keyLeft.setDown(flag(k, "left"));
        mc.options.keyRight.setDown(flag(k, "right"));
        mc.options.keyJump.setDown(flag(k, "jump"));
        mc.options.keyShift.setDown(flag(k, "sneak"));
        mc.options.keySprint.setDown(flag(k, "sprint"));
    }

    public static void afterPlayerTick(LocalPlayer player) {
        if (!armed || writer == null) return;
        try {
            if (player.horizontalCollision) throw new IllegalStateException("Flat-motion profile violated: horizontal collision");
            emit(Map.of("schema", 1, "event", "state", "step", index, "phase", "post_movement",
                    "actor", 0, "channel", "local", "logical_ns", index * 50_000_000L,
                    "wall_ns", System.nanoTime() - started, "client_tick", ParityTelemetry.tickIndex(), "state", motion(player)));
            armed = false;
            if (++index == actions.size()) {
                emit(Map.of("schema", 1, "event", "end", "status", "complete", "samples", index, "dropped", 0));
                writer.close();
                writer = null;
                scenario = null;
                keys(Minecraft.getInstance(), new JsonObject());
                Minecraft.getInstance().options.keyAttack.setDown(false);
                Files.writeString(output.resolve("complete.json"), "{\"status\":\"complete\"}");
            }
        } catch (Exception e) { fail(e); }
    }

    public static JsonObject motion(Player p) {
        var state = new JsonObject();
        state.add("position", vector(p.position()));
        state.add("velocity", vector(p.getDeltaMovement()));
        state.addProperty("yaw", p.getYRot());
        state.addProperty("pitch", p.getXRot());
        state.addProperty("on_ground", p.onGround());
        state.addProperty("sprinting", p.isSprinting());
        state.addProperty("pose", p.getPose() == Pose.CROUCHING ? "crouching" : "standing");
        state.addProperty("fall_distance", p.fallDistance);
        state.addProperty("jump_delay", ((LivingAccess) p).parity$jumpDelay());
        return state;
    }

    public static JsonObject initial(LocalPlayer p) {
        JsonObject state = motion(p);
        state.addProperty("attack_ticks", ((PlayerAccess) p).parity$attackTicks());
        state.addProperty("sprint_trigger", ((LocalAccess) p).parity$sprintTrigger());
        state.addProperty("crouching", p.isCrouching());
        var keys = new JsonObject();
        var k = p.input.keyPresses;
        keys.addProperty("forward", k.forward()); keys.addProperty("backward", k.backward());
        keys.addProperty("left", k.left()); keys.addProperty("right", k.right());
        keys.addProperty("jump", k.jump()); keys.addProperty("sneak", k.shift());
        keys.addProperty("sprint", k.sprint());
        state.add("input", keys);
        return state;
    }

    private static JsonObject vector(Vec3 v) {
        var o = new JsonObject(); o.addProperty("x", v.x); o.addProperty("y", v.y); o.addProperty("z", v.z); return o;
    }
    private static boolean flag(JsonObject object, String key) { return object.has(key) && object.get(key).getAsBoolean(); }
    private static String hash(String text) throws Exception {
        return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(text.getBytes(StandardCharsets.UTF_8)));
    }
    private static void emit(Object value) throws Exception { writer.write(JSON.toJson(value)); writer.newLine(); }
    private static void fail(Exception e) {
        dev.sugi.pvpbot.PvpBot.LOGGER.error("Parity capture failed", e);
        try {
            if (writer != null) writer.close();
            if (output != null) Files.writeString(output.resolve("failed.json"), JSON.toJson(Map.of("error", e.toString())));
        } catch (Exception suppressed) { e.addSuppressed(suppressed); }
        writer = null; scenario = null; armed = false;
        keys(Minecraft.getInstance(), new JsonObject());
    }
}
