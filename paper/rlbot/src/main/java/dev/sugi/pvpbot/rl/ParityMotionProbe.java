// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.BufferedWriter;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Map;
import java.util.UUID;
import org.bukkit.Bukkit;
import org.bukkit.Location;

/** Isolated-server-only motion instrumentation, activated by a JVM property. */
final class ParityMotionProbe implements AutoCloseable {
    private static final Gson JSON = new Gson();
    private final Path root;
    private final RlPlugin plugin;
    private final RlRuntime runtime;
    private String lastRequest = "";
    private JsonObject scenario;
    private JsonObject initial;
    private JsonArray feedback;
    private BotAvatar avatar;
    private Path directory;
    private BufferedWriter writer;
    private int warmup;
    private int index;
    private boolean sampling;

    ParityMotionProbe(Path root, RlPlugin plugin, RlRuntime runtime) {
        this.root = root.toAbsolutePath().normalize();
        this.plugin = plugin; this.runtime = runtime;
    }

    void tickStart() {
        try {
            Path request = root.resolve("bot-request.json");
            if (avatar == null && Files.exists(request)) {
                String text = Files.readString(request);
                if (!text.equals(lastRequest)) {
                    lastRequest = text;
                    var spec = JsonParser.parseString(text).getAsJsonObject();
                    directory = root.resolve(spec.get("directory").getAsString()).normalize();
                    if (!directory.startsWith(root)) throw new IllegalArgumentException("Probe path leaves root");
                    scenario = JsonParser.parseString(Files.readString(directory.resolve("scenario.json"))).getAsJsonObject();
                    initial = JsonParser.parseString(Files.readString(directory.resolve("initial.json"))).getAsJsonObject();
                    feedback = null;
                    if (spec.has("measured_feedback") && spec.get("measured_feedback").getAsBoolean()) {
                        feedback = JsonParser.parseString(Files.readString(directory.resolve("measured-feedback.json"))).getAsJsonArray();
                        if (feedback.size() != scenario.getAsJsonArray("actions").size()) {
                            throw new IllegalArgumentException("Movement feedback must cover every action");
                        }
                    }
                    var position = initial.getAsJsonObject("position");
                    avatar = runtime.newAvatar(UUID.randomUUID(), RlRuntime.PROBE, false,
                            new Location(Bukkit.getWorlds().getFirst(),
                            position.get("x").getAsDouble(), position.get("y").getAsDouble(), position.get("z").getAsDouble(),
                            initial.get("yaw").getAsFloat(), initial.get("pitch").getAsFloat()), null);
                    warmup = 40; index = 0;
                }
            }
            if (avatar == null || warmup-- > 0) return;
            if (writer == null) {
                avatar.probeReset(initial);
                var head = JsonParser.parseString(Files.readAllLines(directory.resolve("client.ndjson")).getFirst()).getAsJsonObject();
                head.addProperty("engine", feedback == null ? "paper-citizens-motion-v1" : "paper-citizens-measured-feedback-v1");
                writer = Files.newBufferedWriter(directory.resolve("bot.ndjson"));
                emit(head);
            }
            avatar.probeAction(scenario.getAsJsonArray("actions").get(index).getAsJsonObject(),
                    feedback == null ? null : feedback.get(index).getAsJsonObject());
            sampling = true;
        } catch (Exception e) { fail(e); }
    }

    void tickEnd() {
        if (!sampling || avatar == null) return;
        try {
            emit(Map.of("schema", 1, "event", "state", "step", index, "phase", "post_movement",
                    "actor", 0, "channel", "local", "logical_ns", index * 50_000_000L,
                    "server_tick", Bukkit.getCurrentTick(), "state", avatar.probeState()));
            sampling = false;
            if (++index == scenario.getAsJsonArray("actions").size()) {
                emit(Map.of("schema", 1, "event", "end", "status", "complete", "samples", index, "dropped", 0));
                close();
                Files.writeString(directory.resolve("bot-complete.json"), "{\"status\":\"complete\"}");
            }
        } catch (Exception e) { fail(e); }
    }

    private void emit(Object value) throws Exception { writer.write(JSON.toJson(value)); writer.newLine(); }

    private void fail(Exception e) {
        plugin.getLogger().log(java.util.logging.Level.SEVERE, "Parity motion probe failed", e);
        try {
            close();
            if (directory != null) Files.writeString(directory.resolve("bot-failed.json"), JSON.toJson(Map.of("error", e.toString())));
        } catch (Exception other) { e.addSuppressed(other); }
    }

    @Override public void close() throws Exception {
        sampling = false;
        try {
            if (writer != null) { writer.close(); writer = null; }
        } finally {
            if (avatar != null) { avatar.close(); avatar = null; }
        }
    }
}
