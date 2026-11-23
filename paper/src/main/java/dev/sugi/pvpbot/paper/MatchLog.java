// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import dev.sugi.pvpbot.paper.policy.Action;
import dev.sugi.pvpbot.paper.policy.Observation;
import java.io.BufferedWriter;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;

final class MatchLog implements AutoCloseable {
    record Fighter(String name, String player_uuid, String bot, String model_sha256,
                   String policy_version, Integer bot_ping_ms, double hp, double max_hp, double damage) {}
    record Result(int schema_version, String match_id, String started_at, String ended_at,
                  String mode, String host_uuid, String arena, Fighter a, Fighter b,
                  String outcome, String winner, String reason, int elapsed_ticks) {
        Fighter winningFighter() {
            return switch (winner == null ? "" : winner) {
                case "A" -> a;
                case "B" -> b;
                default -> null;
            };
        }

        Result ended(String outcome, String winner, String reason, int ticks, Fighter a, Fighter b) {
            return new Result(schema_version, match_id, started_at, Instant.now().toString(), mode, host_uuid,
                    arena, a, b, outcome, winner, reason, ticks);
        }
    }

    private final Path directory;
    private final Result initial;
    private BufferedWriter trace;

    MatchLog(Path directory, Result initial) throws IOException {
        this.directory = directory;
        this.initial = initial;
        StateFiles.writeJson(pending(), initial);
    }

    void openTrace() throws IOException { trace = Files.newBufferedWriter(directory.resolve(initial.match_id + ".ndjson")); }

    private Path pending() { return directory.resolve(initial.match_id + ".active.json"); }

    static void recover(Path directory) throws IOException {
        Files.createDirectories(directory);
        try (var files = Files.newDirectoryStream(directory, "*.active.json")) {
            for (Path file : files) {
                Result active = StateFiles.JSON.fromJson(Files.readString(file), Result.class);
                if (active == null || !file.getFileName().toString().equals(active.match_id + ".active.json")) {
                    throw new IOException("Invalid match journal " + file);
                }
                Path result = directory.resolve(active.match_id + ".json");
                if (!Files.exists(result)) {
                    StateFiles.writeJson(result, active.ended("aborted", null, "server_restart",
                            active.elapsed_ticks, active.a, active.b));
                }
                Files.delete(file);
            }
        }
    }

    private record Step(String side, Observation observation, Action action, boolean attack_attempted, long applied_at_ns) {}

    void step(String side, BotAvatar.Sample sample, Action action, boolean attempted, long appliedAtNs) throws IOException {
        if (trace == null) return;
        var row = StateFiles.JSON.toJsonTree(new Step(side, sample.observation(), action, attempted, appliedAtNs)).getAsJsonObject();
        if (sample.recurrent() != null) row.add("recurrent", StateFiles.JSON.toJsonTree(sample.recurrent()));
        trace.write(StateFiles.JSON.toJson(row));
        trace.newLine();
    }

    void result(Result result) throws IOException {
        StateFiles.writeJson(directory.resolve(initial.match_id + ".json"), result);
        Files.deleteIfExists(pending());
    }

    @Override public void close() throws IOException { if (trace != null) trace.close(); }
}
