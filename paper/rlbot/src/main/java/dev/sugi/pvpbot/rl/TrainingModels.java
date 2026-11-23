// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import dev.sugi.pvpbot.rl.policy.BotPolicy;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.logging.Level;
import java.util.logging.Logger;
import java.util.random.RandomGenerator;
import java.util.regex.Pattern;

final class TrainingModels {
    static final String RANDOM = "random";
    private static final String INITIAL = "Recurrent PPO";
    private static final String NEXTGEN = "Next-gen GRU";
    private static final Pattern SHA256 = Pattern.compile("[0-9a-f]{64}");
    private static final DateTimeFormatter JAPAN_TIME = DateTimeFormatter
            .ofPattern("MMM d, uuuu, HH:mm z", Locale.ENGLISH).withZone(ZoneId.of("Asia/Tokyo"));
    private final Map<String, Entry> entries;
    private final List<List<Entry>> generations;

    record Provenance(String sha256, String generation, String process, String branch, int seed,
                      int update, long steps, String savedAt, String timeSource, String source) {
        String heading() {
            return "Training opponent · " + generation + " · " + process;
        }

        String details() {
            return String.format(Locale.US, "Seed %d · Update %,d · %,d steps · %s %s", seed, update, steps,
                    "export".equals(timeSource) ? "Exported" : "File saved", JAPAN_TIME.format(Instant.parse(savedAt)));
        }
    }

    private record Manifest(int version, List<Provenance> entries) {}
    record Entry(Provenance provenance, BotPolicy.RecurrentWeights weights) {}

    private TrainingModels(Map<String, Entry> entries) {
        this.entries = Map.copyOf(entries);
        generations = List.of(INITIAL, NEXTGEN).stream()
                .map(generation -> entries.values().stream()
                        .filter(entry -> generation.equals(entry.provenance().generation())).toList())
                .filter(group -> !group.isEmpty()).toList();
    }

    static TrainingModels load(Path directory, Logger logger) {
        var entries = new LinkedHashMap<String, Entry>();
        Path catalog = directory.resolve("catalog.json");
        if (Files.notExists(catalog)) return new TrainingModels(entries);
        try {
            var manifest = StateFiles.JSON.fromJson(Files.readString(catalog), Manifest.class);
            if (manifest == null || manifest.version() != 1 || manifest.entries() == null) {
                throw new IOException("Invalid training model catalog");
            }
            for (Provenance provenance : manifest.entries()) {
                try {
                    if (provenance == null || provenance.sha256() == null
                            || !SHA256.matcher(provenance.sha256()).matches()
                            || !(INITIAL.equals(provenance.generation()) || NEXTGEN.equals(provenance.generation()))
                            || provenance.process() == null || provenance.process().isBlank()
                            || provenance.update() < 0 || provenance.steps() < 0
                            || !("file".equals(provenance.timeSource()) || "export".equals(provenance.timeSource()))) {
                        throw new IOException("Invalid training model provenance");
                    }
                    Instant.parse(provenance.savedAt());
                    String hash = provenance.sha256();
                    if (entries.containsKey(hash)) continue;
                    Path path = directory.resolve("weights").resolve(hash + ".json");
                    var weights = BotPolicy.loadRecurrent(Files.readAllBytes(path), hash);
                    entries.put(hash, new Entry(provenance, weights));
                } catch (IOException | RuntimeException e) {
                    logger.log(Level.WARNING, "Skipping training model " + provenance, e);
                }
            }
        } catch (IOException | RuntimeException e) {
            logger.log(Level.WARNING, "Could not load PvPBotRL training model catalog", e);
        }
        logger.info("Loaded " + entries.size() + " PvPBotRL training models.");
        return new TrainingModels(entries);
    }

    Entry get(String hash) { return hash == null ? null : entries.get(hash); }
    boolean isEmpty() { return entries.isEmpty(); }

    Entry draw(RandomGenerator random) {
        if (generations.isEmpty()) return null;
        var group = generations.get(random.nextInt(generations.size()));
        return group.get(random.nextInt(group.size()));
    }
}
