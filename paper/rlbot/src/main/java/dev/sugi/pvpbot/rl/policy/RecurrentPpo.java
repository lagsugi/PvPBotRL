// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl.policy;

import com.google.gson.Gson;
import java.io.IOException;
import java.util.function.Consumer;

/** Inference for the full-input GRU model exported by pvp-sim. */
final class RecurrentPpo {
    static final int INPUTS = 44;
    static final int WIDTH = 64;
    static final int OUTPUTS = 10;
    private static final double SINE_INDEX_SCALE = 10430.378350470453;

    private final Model model;
    private final float[] hidden = new float[WIDTH];
    private Action previous = Action.IDLE;

    RecurrentPpo(Model model) { this.model = model; }

    Action act(Observation o, RecurrentInput extra, Consumer<BotPolicy.Inference> trace) {
        float[] features = features(o, extra, previous);
        float[] logits = model.logits(features, hidden);
        previous = action(o, logits);
        if (trace != null) trace.accept(new BotPolicy.Inference(features, hidden.clone(), logits, previous));
        return previous;
    }

    static float[] features(Observation o, RecurrentInput extra, Action previous) {
        // Rust's look_vector uses the Minecraft sine-table indices. Compute the
        // same two table entries for this yaw without keeping a second table.
        float yaw = o.yaw() * (float) (Math.PI / 180.0);
        double index = -yaw * SINE_INDEX_SCALE;
        double forwardX = sineEntry(index);
        double forwardZ = sineEntry(index + 16384);
        var relative = o.relative_position();
        var velocity = o.velocity();
        var remoteVelocity = o.opponent_velocity();
        var displacement = o.opponent_displacement();
        var armor = o.armor_present();
        var keys = previous.keys();
        float[] values = {
            o.health() / 20f, extra.opponent_health() / 20f, o.attack_charge(),
            forward(relative, forwardX, forwardZ) / 8f,
            left(relative, forwardX, forwardZ) / 8f, (float) relative.y() / 8f,
            (float) o.horizontal_distance() / 8f, (float) o.eye_to_target() / 8f,
            bit(o.target_under_crosshair()), o.pitch() / 90f,
            forward(velocity, forwardX, forwardZ), left(velocity, forwardX, forwardZ), (float) velocity.y(),
            forward(remoteVelocity, forwardX, forwardZ), left(remoteVelocity, forwardX, forwardZ), (float) remoteVelocity.y(),
            forward(displacement, forwardX, forwardZ), left(displacement, forwardX, forwardZ), (float) displacement.y(),
            bit(o.sprinting()), bit(o.sneaking()), bit(o.on_ground()),
            bit(o.opponent_sprinting()), bit(o.opponent_sneaking()), bit(o.opponent_on_ground()),
            age(o.position_age_ns()), age(o.metadata_age_ns()), age(o.velocity_age_ns()),
            age(extra.health_age_ns()), age(extra.opponent_health_age_ns()),
            bit(o.sword_present()), bit(armor[0]), bit(armor[1]), bit(armor[2]), bit(armor[3]),
            extra.jump_delay() / 10f, extra.miss_time() / 10f,
            (float) (Math.log1p(Math.clamp(extra.fall_distance(), 0, 64)) / Math.log(65)),
            (float) (extra.height_above_floor() / 8), extra.remaining_fraction(),
            bit(keys.forward()) - bit(keys.backward()), bit(keys.left()) - bit(keys.right()),
            bit(keys.jump()), bit(previous.click())
        };
        for (int i = 0; i < values.length; i++) {
            if (!Float.isFinite(values[i])) throw new IllegalArgumentException("Nonfinite recurrent input");
            values[i] = Math.clamp(values[i], -8f, 8f);
        }
        return values;
    }

    static Action action(Observation o, float[] logits) {
        int longitudinal = argmax(logits, 0, 3);
        int lateral = argmax(logits, 3, 3);
        return Action.of(longitudinal == 2, longitudinal == 0, lateral == 2, lateral == 0,
                o.on_ground() && argmax(logits, 6, 2) == 1,
                o.eye_to_target() < 3 && argmax(logits, 8, 2) == 1);
    }

    private static int argmax(float[] logits, int offset, int count) {
        int best = offset;
        for (int i = offset + 1; i < offset + count; i++) {
            if (logits[i] > logits[best]) best = i;
        }
        return best - offset;
    }

    private static float sineEntry(double index) {
        return (float) Math.sin(((long) index & 65535) / SINE_INDEX_SCALE);
    }

    private static float forward(Observation.Vec v, double x, double z) { return (float) (v.x() * x + v.z() * z); }
    private static float left(Observation.Vec v, double x, double z) { return (float) (v.x() * z - v.z() * x); }
    private static float age(long ns) { return (float) (Math.log1p(Math.min(ns / 1e9, 10)) / Math.log(11)); }
    private static float bit(boolean value) { return value ? 1f : 0f; }

    record Model(int version, String input, Linear encoder, Linear input_gate, Linear hidden_gate,
                 Linear actor, float[] mean, float[] scale) {
        static Model load(String json) throws IOException {
            Model model = new Gson().fromJson(json, Model.class);
            if (model == null || model.version != 1 || !"full".equals(model.input)
                    || model.encoder == null || !model.encoder.valid(WIDTH, INPUTS)
                    || model.input_gate == null || !model.input_gate.valid(3 * WIDTH, WIDTH)
                    || model.hidden_gate == null || !model.hidden_gate.valid(3 * WIDTH, WIDTH)
                    || model.actor == null || !model.actor.valid(OUTPUTS, WIDTH)
                    || !finite(model.mean, INPUTS) || !finite(model.scale, INPUTS)) {
                throw new IOException("Invalid Recurrent-PPO model version, input or weights");
            }
            for (float value : model.scale) {
                if (value <= 0) throw new IOException("Recurrent-PPO scale must be positive");
            }
            return model;
        }

        float[] logits(float[] features, float[] hidden) {
            float[] normalized = new float[INPUTS];
            for (int i = 0; i < INPUTS; i++) {
                normalized[i] = Math.clamp((features[i] - mean[i]) * scale[i], -8f, 8f);
            }
            float[] encoded = encoder.apply(normalized);
            for (int i = 0; i < WIDTH; i++) encoded[i] = (float) Math.tanh(encoded[i]);
            float[] in = input_gate.apply(encoded);
            float[] old = hidden_gate.apply(hidden);
            for (int i = 0; i < WIDTH; i++) {
                float reset = sigmoid(in[i] + old[i]);
                float update = sigmoid(in[WIDTH + i] + old[WIDTH + i]);
                int candidateIndex = 2 * WIDTH + i;
                float candidate = (float) Math.tanh(in[candidateIndex] + reset * old[candidateIndex]);
                hidden[i] = (1f - update) * candidate + update * hidden[i];
            }
            return actor.apply(hidden);
        }

        private static float sigmoid(float value) { return 1f / (1f + (float) Math.exp(-value)); }
    }

    private record Linear(float[][] weight, float[] bias) {
        boolean valid(int rows, int columns) {
            if (weight == null || weight.length != rows || !finite(bias, rows)) return false;
            for (float[] row : weight) if (!finite(row, columns)) return false;
            return true;
        }

        float[] apply(float[] input) {
            float[] output = new float[weight.length];
            for (int row = 0; row < weight.length; row++) {
                float sum = 0;
                for (int col = 0; col < input.length; col++) sum += weight[row][col] * input[col];
                output[row] = bias[row] + sum;
            }
            return output;
        }
    }

    private static boolean finite(float[] values, int length) {
        if (values == null || values.length != length) return false;
        for (float value : values) if (!Float.isFinite(value)) return false;
        return true;
    }
}
