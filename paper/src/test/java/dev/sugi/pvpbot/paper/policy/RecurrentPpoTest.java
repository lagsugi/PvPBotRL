package dev.sugi.pvpbot.paper.policy;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.Gson;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.Test;

class RecurrentPpoTest {
    private record Decision(Observation observation, RecurrentInput recurrent, float[] features,
                            float[] logits, Action action) {}

    @Test void fullInputsLogitsAndActionsMatchRustAcrossConsecutiveDecisions() throws Exception {
        // Rust G6 vs Recurrent-PPO, seed 42, 20 ms RTT each. Spawn positions
        // are (-0.35, 0, -1.7) and (0.35, 0, 1.7), with bot yaw -168°.
        // The first observation starts from zero GRU memory and no previous action.
        var policy = new BotPolicy(BotPolicy.RECURRENT_PPO);
        var model = RecurrentPpo.Model.load(modelJson());
        var hidden = new float[RecurrentPpo.WIDTH];
        var gson = new Gson();
        var previous = Action.IDLE;
        Decision first = null;
        int frames = 0;
        var stream = getClass().getResourceAsStream("recurrent-ppo-rust.ndjson");
        assertNotNull(stream);
        try (var reader = new BufferedReader(new InputStreamReader(stream, StandardCharsets.UTF_8))) {
            for (String line; (line = reader.readLine()) != null; ) {
                var decision = gson.fromJson(line, Decision.class);
                if (first == null) first = decision;
                String frame = "frame " + frames++;
                var features = RecurrentPpo.features(decision.observation(), decision.recurrent(), previous);
                assertArrayEquals(decision.features(), features, 1e-6f, frame);
                assertArrayEquals(decision.logits(), model.logits(features, hidden), 1e-4f, frame);
                previous = policy.act(decision.observation(), decision.recurrent());
                assertEquals(decision.action(), previous, frame);
            }
        }
        assertEquals(28, frames);
        assertNotNull(first);
        var rematch = new BotPolicy(BotPolicy.RECURRENT_PPO);
        assertEquals(first.action(), rematch.act(first.observation(), first.recurrent()));
    }

    @Test void actionMasksJumpAndClickOutsideTheirAllowedStates() {
        var gson = new Gson();
        var allowed = gson.fromJson("{\"on_ground\":true,\"eye_to_target\":2.99}", Observation.class);
        var blocked = gson.fromJson("{\"on_ground\":false,\"eye_to_target\":3}", Observation.class);
        float[] logits = {0, 0, 1, 0, 0, 1, 0, 1, 0, 1};
        assertEquals(Action.of(true, false, true, false, true, true), RecurrentPpo.action(allowed, logits));
        var action = RecurrentPpo.action(blocked, logits);
        assertTrue(action.keys().forward());
        assertTrue(action.keys().left());
        assertFalse(action.keys().jump());
        assertFalse(action.click());
    }

    @Test void invalidModelVersionInputShapeAndScaleAreRejected() throws IOException {
        String valid = modelJson();
        var model = JsonParser.parseString(valid).getAsJsonObject();
        model.addProperty("version", 2);
        assertThrows(IOException.class, () -> RecurrentPpo.Model.load(model.toString()));
        model.addProperty("version", 1);
        model.addProperty("input", "no_opponent_hp");
        assertThrows(IOException.class, () -> RecurrentPpo.Model.load(model.toString()));
        model.addProperty("input", "full");
        var scale = model.getAsJsonArray("scale");
        var last = scale.remove(scale.size() - 1);
        assertThrows(IOException.class, () -> RecurrentPpo.Model.load(model.toString()));
        scale.add(new JsonPrimitive(0));
        assertThrows(IOException.class, () -> RecurrentPpo.Model.load(model.toString()));
        scale.set(scale.size() - 1, last);
        var firstActorRow = model.getAsJsonObject("actor").getAsJsonArray("weight").get(0).getAsJsonArray();
        firstActorRow.set(0, JsonParser.parseString("1e99"));
        assertThrows(IOException.class, () -> RecurrentPpo.Model.load(model.toString()));
    }

    private String modelJson() throws IOException {
        try (var stream = getClass().getResourceAsStream("/models/recurrent_ppo.json")) {
            assertNotNull(stream);
            return new String(stream.readAllBytes(), StandardCharsets.UTF_8);
        }
    }
}
