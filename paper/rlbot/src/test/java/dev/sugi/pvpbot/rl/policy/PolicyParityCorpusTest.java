package dev.sugi.pvpbot.rl.policy;

import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.assumeTrue;
import com.google.gson.Gson;
import com.google.gson.JsonParser;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.util.HexFormat;
import org.junit.jupiter.api.Test;

class PolicyParityCorpusTest {
    private record Frame(String model, int episode, int step, Observation observation, RecurrentInput recurrent,
                         float[] features, float[] logits, float[] hidden, Action action) {}

    @Test void suppliedCorpusMatchesFeaturesMemoryLogitsAndActions() throws Exception {
        String source = System.getProperty("pvp.parity.policyFixture");
        assumeTrue(source != null, "Explicit generated corpus required; not evidence when skipped");
        var gson = new Gson();
        int frames = 0, episodes = 0, episode = -1, expectedStep = 0;
        boolean ended = false;
        String trainingPath = System.getProperty("pvp.parity.trainingModel");
        BotPolicy.RecurrentWeights trainingWeights = null;
        if (trainingPath != null) {
            byte[] bytes = Files.readAllBytes(Path.of(trainingPath));
            String hash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
            trainingWeights = BotPolicy.loadRecurrent(bytes, hash);
        }
        BotPolicy policy = null;
        try (var reader = Files.newBufferedReader(Path.of(source))) {
            var manifest = JsonParser.parseString(reader.readLine()).getAsJsonObject();
            assertEquals("manifest", manifest.get("event").getAsString());
            assertEquals(1, manifest.get("schema").getAsInt());
            for (String line; (line = reader.readLine()) != null;) {
                assertFalse(ended, "Trailing data after end");
                var value = JsonParser.parseString(line).getAsJsonObject();
                if (value.get("event").getAsString().equals("end")) {
                    assertEquals(frames, value.get("frames").getAsInt());
                    assertEquals(episodes, value.get("episodes").getAsInt());
                    ended = true; continue;
                }
                var f = gson.fromJson(value, Frame.class);
                if (f.episode() != episode) {
                    assertEquals(episode + 1, f.episode());
                    episode = f.episode(); episodes++; expectedStep = 0;
                    if ("training".equals(f.model())) {
                        assertNotNull(trainingWeights);
                        policy = trainingWeights.newPolicy("random");
                    } else {
                        policy = new BotPolicy(f.model().replace('_', '-'));
                    }
                }
                assertEquals(expectedStep++, f.step());
                String context = f.model() + " episode " + episode + " step " + f.step();
                Action action = policy.act(f.observation(), f.recurrent(), actual -> {
                    assertArrayEquals(f.features(), actual.features(), 1e-6f, context);
                    assertArrayEquals(f.hidden(), actual.hidden(), 1e-4f, context);
                    assertArrayEquals(f.logits(), actual.logits(), 1e-4f, context);
                });
                assertEquals(f.action(), action, context);
                frames++;
            }
        }
        assertTrue(ended, "Truncated corpus");
        assertEquals(trainingWeights == null ? 800 : 200, episodes);
        assertTrue(frames >= episodes, "Empty episodes");
    }
}
