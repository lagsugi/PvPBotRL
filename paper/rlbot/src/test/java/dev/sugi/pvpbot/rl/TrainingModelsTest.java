package dev.sugi.pvpbot.rl;

import static org.junit.jupiter.api.Assertions.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.logging.Logger;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class TrainingModelsTest {
    private static final Logger LOGGER = Logger.getLogger(TrainingModelsTest.class.getName());
    @TempDir Path directory;

    @Test void rejectsInvalidWeightsAndDrawsGenerationBeforeModel() throws Exception {
        Files.createDirectory(directory.resolve("weights"));
        byte[] weights;
        try (var stream = getClass().getResourceAsStream("/models/recurrent_ppo.json")) {
            assertNotNull(stream);
            weights = stream.readAllBytes();
        }
        String json = new String(weights, StandardCharsets.UTF_8);
        var early = add("Recurrent PPO", json);
        var late = add("Recurrent PPO", json + "\n");
        var next = add("Next-gen GRU", json + "\n\n");
        var unsupported = add("Next-gen GRU", json.replace("\"full\"", "\"legacy\""));
        Files.writeString(weightPath(early), json + "\n\n\n");
        manifest(List.of(early, late, next, unsupported, next));
        var catalog = TrainingModels.load(directory, LOGGER);
        assertNull(catalog.get(early.sha256()));
        assertNull(catalog.get(unsupported.sha256()));
        assertEquals(late, catalog.draw(fixed(0, 0)).provenance());
        assertEquals(next, catalog.draw(fixed(1, 0)).provenance());

        Files.write(weightPath(early), weights);
        manifest(List.of(early, late, next, next));
        catalog = TrainingModels.load(directory, LOGGER);
        assertEquals(early, catalog.draw(fixed(0, 0)).provenance());
        assertEquals(late, catalog.draw(fixed(0, 1)).provenance());
        assertEquals(next, catalog.draw(fixed(1, 0)).provenance());
        assertEquals(next.sha256(), catalog.get(next.sha256()).weights().newPolicy("random").hash());
        assertEquals("Training opponent · Next-gen GRU · High-ping fine-tuning", next.heading());
        assertEquals("Seed 73002 · Update 361 · 47,316,992 steps · File saved Nov 13, 2026, 21:27 JST", next.details());

        manifest(List.of(next));
        assertEquals(next, TrainingModels.load(directory, LOGGER).draw(fixed(0, 0)).provenance());
        assertNull(TrainingModels.load(directory.resolve("missing"), LOGGER).draw(fixed()));
    }

    private TrainingModels.Provenance add(String generation, String json) throws Exception {
        var bytes = json.getBytes(StandardCharsets.UTF_8);
        var hash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        var provenance = new TrainingModels.Provenance(hash, generation, "High-ping fine-tuning",
                "finish-high-ping", 73002, 361, 47_316_992, "2026-11-13T12:27:00Z", "file", "candidate-0000361.json");
        Files.write(weightPath(provenance), bytes);
        return provenance;
    }

    private Path weightPath(TrainingModels.Provenance provenance) {
        return directory.resolve("weights").resolve(provenance.sha256() + ".json");
    }

    private void manifest(List<TrainingModels.Provenance> entries) throws Exception {
        StateFiles.writeJson(directory.resolve("catalog.json"), Map.of("version", 1, "entries", entries));
    }

    private static Random fixed(int... draws) {
        return new Random(0) {
            private int index;
            @Override public int nextInt(int bound) {
                assertTrue(index < draws.length);
                int draw = draws[index++];
                assertTrue(draw < bound);
                return draw;
            }
        };
    }
}
