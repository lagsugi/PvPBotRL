package dev.sugi.pvpbot.rl.policy;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.Gson;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

class BotPolicyTest {
    @Test void bundledModelsAndScriptsCanStartFromTheStandardDistance() throws Exception {
        var zero = new Observation.Vec(0, 0, 0);
        var observation = new Observation(0, 20, zero, 1, false, false, false, true, false, false,
                0, 0, new Observation.Vec(0, 0, 20), zero, zero, false, false, false,
                true, false, false, 20, 20, 19.7, false, 0, 0, 0, 0, true,
                new boolean[]{true, true, true, true});
        for (String style : BotPolicy.STYLES) {
            var policy = new BotPolicy(style);
            if (BotPolicy.isRecurrent(style)) {
                var input = new RecurrentInput(20, 0, 0, 0, 0, 0, 0, 1);
                assertFalse(policy.act(observation, input).click(), style);
                if (style.equals(BotPolicy.RECURRENT_PPO)) assertEquals("cc78c1ec607ceb39ce0909548083b9d7f10454b73a28cac1b1059291f7407eb1", policy.hash());
                assertEquals("recurrent-ppo-java-v1", policy.version());
                continue;
            }
            Action action = policy.act(observation);
            assertTrue(action.keys().forward(), style);
            assertFalse(action.click(), style);
            if (style.endsWith("-rl") || style.startsWith("league-g")) {
                assertTrue(policy.hash().matches("[a-f0-9]{64}"), style);
            }
        }
    }

    private record Decision(Observation observation, Action action) {}

    @Test void leagueG6MatchesRustActionsAcrossConsecutiveObservations() throws Exception {
        // Rust G6 vs G5, seed 42, RTT 20 vs 0 ms. Expected actions use fresh
        // G6 memory at the start of this consecutive excerpt.
        var input = BotPolicyTest.class.getResourceAsStream("league-g6-rust.ndjson");
        assertNotNull(input);
        var policy = new BotPolicy("league-g6");
        var gson = new Gson();
        int frames = 0;
        try (var reader = new BufferedReader(new InputStreamReader(input, StandardCharsets.UTF_8))) {
            for (String line; (line = reader.readLine()) != null; ) {
                var decision = gson.fromJson(line, Decision.class);
                assertEquals(decision.action(), policy.act(decision.observation()), "frame " + frames);
                frames++;
            }
        }
        assertEquals(18, frames);
    }
}
