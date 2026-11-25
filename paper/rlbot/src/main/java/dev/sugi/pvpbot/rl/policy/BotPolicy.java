// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl.policy;

import com.google.gson.Gson;
import com.google.gson.JsonParseException;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.List;
import java.util.function.Consumer;

/** Paper adapter for the Rust policies. One instance per match. */
public final class BotPolicy {
    public static final String RECURRENT_PPO = "recurrent-ppo";
    public static final String RECURRENT_G2 = "recurrent-g2";
    public static final String RECURRENT_G2_PLUS = "recurrent-g2-plus";
    public static final String G3 = "g3";
    public static boolean isRecurrent(String style) {
        return RECURRENT_PPO.equals(style) || RECURRENT_G2.equals(style)
                || RECURRENT_G2_PLUS.equals(style) || G3.equals(style);
    }
    public static final List<String> STYLES = List.of("baseline", "pressure", "orbit", "counter",
            "critical", "critical-plus", "duel-rl", "league-rl", "league-g5", "league-g6",
            RECURRENT_PPO, RECURRENT_G2, RECURRENT_G2_PLUS, G3);
    private final String style;
    private final double[][] weights;
    private final RecurrentPpo recurrent;
    private final String hash;
    private long lastSwing = -1, switchAt;
    private boolean strafeLeft;
    private float lastHealth = 20;
    private boolean previous;
    private float previousHealth;
    private double previousDistance, hurt, recentSwing, approach, opponentAir;

    public BotPolicy(String style) throws IOException {
        if (!STYLES.contains(style)) throw new IllegalArgumentException("Unknown bot: " + style);
        this.style = style;
        int features = switch (style) {
            case "duel-rl" -> 10;
            case "league-rl" -> 16;
            case "league-g5" -> 28;
            case "league-g6" -> 36;
            default -> 0;
        };
        if (features != 0 || isRecurrent(style)) {
            String file = style.replace('-', '_') + ".json";
            try (var stream = BotPolicy.class.getResourceAsStream("/models/" + file)) {
                if (stream == null) throw new IOException("Missing model: " + file);
                byte[] bytes = stream.readAllBytes();
                String json = new String(bytes, StandardCharsets.UTF_8);
                if (isRecurrent(style)) {
                    recurrent = new RecurrentPpo(RecurrentPpo.Model.load(json));
                    weights = null;
                } else {
                    recurrent = null;
                    weights = new Gson().fromJson(json, Model.class).weights;
                    if (weights == null || weights.length != 4) throw new IOException("Invalid model rows");
                    for (double[] row : weights) {
                        if (row.length != features) throw new IOException("Invalid model features");
                        for (double w : row) if (!Double.isFinite(w)) throw new IOException("Nonfinite weight");
                    }
                }
                hash = sha256(bytes);
            } catch (JsonParseException e) {
                throw new IOException("Invalid model JSON: " + file, e);
            }
        } else {
            weights = null;
            recurrent = null;
            hash = "script-v1-" + style;
        }
    }

    private BotPolicy(String style, RecurrentWeights weights) {
        this.style = style;
        this.weights = null;
        this.recurrent = new RecurrentPpo(weights.model);
        this.hash = weights.hash;
    }

    /** Validated tensors shared by independent controllers. */
    public static final class RecurrentWeights {
        private final RecurrentPpo.Model model;
        private final String hash;

        private RecurrentWeights(RecurrentPpo.Model model, String hash) {
            this.model = model;
            this.hash = hash;
        }

        public BotPolicy newPolicy(String style) { return new BotPolicy(style, this); }
    }

    public static RecurrentWeights loadRecurrent(byte[] bytes, String expectedHash) throws IOException {
        String hash = sha256(bytes);
        if (!hash.equals(expectedHash)) throw new IOException("Model SHA-256 mismatch");
        try {
            var model = RecurrentPpo.Model.load(new String(bytes, StandardCharsets.UTF_8));
            return new RecurrentWeights(model, hash);
        } catch (JsonParseException e) {
            throw new IOException("Invalid recurrent model JSON", e);
        }
    }

    private static String sha256(byte[] bytes) {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes));
        } catch (NoSuchAlgorithmException e) {
            throw new AssertionError(e);
        }
    }

    private record Model(double[][] weights) {}
    public String hash() { return hash; }
    public String style() { return style; }
    public boolean isRecurrent() { return recurrent != null; }
    public String version() {
        if (recurrent != null) return "recurrent-ppo-java-v1";
        return weights != null ? "java-v1" : hash;
    }
    private static double clamp(double x, double lo, double hi) { return Math.max(lo, Math.min(hi, x)); }
    private static double bit(boolean x) { return x ? 1.0 : 0.0; }

    public Action act(Observation o) {
        return act(o, null);
    }

    public Action act(Observation o, RecurrentInput input) {
        return act(o, input, null);
    }

    public record Inference(float[] features, float[] hidden, float[] logits, Action selected) {}

    public Action act(Observation o, RecurrentInput input, Consumer<Inference> trace) {
        if (recurrent != null) {
            if (input == null) throw new IllegalArgumentException("Missing Recurrent-PPO observation fields");
            return recurrent.act(o, input, trace);
        }
        double d = o.horizontal_distance();
        if (weights != null) return learned(o);
        if (style.equals("baseline")) return Action.of(d > 2.4, d < 1.5, false, false, false,
                o.attack_charge() >= 1.0 && o.eye_to_target() < 2.95);
        long tick = o.time_ns() / 50_000_000L;
        long since = lastSwing < 0 ? Long.MAX_VALUE : Math.max(0, tick - lastSwing);
        boolean reach = o.eye_to_target() < 2.99;
        boolean f = false, b = false, l = false, r = false, j = false;
        boolean click = reach && o.attack_charge() >= 0.99f;
        switch (style) {
            case "critical", "critical-plus" -> {
                boolean plus = style.equals("critical-plus");
                j = o.on_ground() && d < 7.5 && o.attack_charge() > (plus ? 0.95f : 0.35f);
                boolean coasting = !o.on_ground() && d < (plus ? 4.0 : 4.5)
                        && o.velocity().y() < (plus ? 0.0 : 0.25);
                f = d > (plus ? 0.0 : 1.4) && !coasting && !(since >= 1 && since <= (plus ? 4 : 1));
                b = d < (plus ? 1.7 : 1.0);
                boolean ready = reach && o.attack_charge() > (plus ? 0.9f : 0.93f);
                click = ready && o.falling() && !o.sprinting();
                if (click) { f = false; j = false; }
                else if (ready && since >= (plus ? 12 : 15)) {
                    f = true; b = false; j = false; click = o.sprinting();
                }
            }
            case "pressure" -> { click = reach && o.attack_charge() > 0.9f; f = since != 1 && d > 0.9; }
            case "orbit" -> {
                if (d < 4.6) {
                    if (tick >= switchAt || o.health() < lastHealth) {
                        strafeLeft = !strafeLeft; switchAt = tick + (strafeLeft ? 17 : 12);
                    }
                    l = strafeLeft; r = !strafeLeft;
                }
                f = d > 1.9 && since != 1; b = d < 1.25;
            }
            case "counter" -> {
                var rel = o.relative_position(); var v = o.opponent_displacement();
                boolean movingIn = rel.x() * v.x() + rel.z() * v.z() < -0.05 * d;
                boolean recovery = since <= 7;
                b = recovery && d < (movingIn ? 3.8 : 3.35);
                f = !b && since != 1 && d > (recovery ? 4.1 : 2.55);
            }
            default -> throw new IllegalStateException(style);
        }
        if (click) lastSwing = tick;
        lastHealth = o.health();
        return Action.of(f, b, l, r, j, click);
    }

    private Action learned(Observation o) {
        double d = Math.max(o.horizontal_distance(), 1e-6);
        var r = o.relative_position(); var v = o.velocity(); var remote = o.opponent_displacement();
        double closing = (v.x() * r.x() + v.z() * r.z()) / d;
        double charge = o.attack_charge();
        double[] x = new double[weights[0].length];
        double[] base = {1, clamp((d - 3) / 3, -1, 2), (1 - charge) * 2,
                bit(o.rising()), bit(o.falling()), bit(o.sprinting()), clamp(r.y(), -1.5, 1.5),
                bit(o.opponent_on_ground()), clamp(3 * closing, -1.5, 1.5), clamp(3 - o.eye_to_target(), -1, 1)};
        System.arraycopy(base, 0, x, 0, 10);
        if (x.length >= 16) {
            double scale = 3 / d;
            x[10] = clamp((r.x() * remote.x() + r.z() * remote.z()) * scale, -1.5, 1.5);
            x[11] = Math.min(Math.abs((r.x() * remote.z() - r.z() * remote.x()) * scale), 1.5);
            x[12] = Math.min(o.position_age_ns() / 100_000_000.0, 2);
            x[13] = bit(o.opponent_sprinting()); x[14] = clamp(3 * v.y(), -1.5, 1.5);
            x[15] = bit(o.attack_charge() >= 1.0);
        }
        boolean hasMemory = x.length >= 28;
        if (hasMemory) {
            float hp = previous ? previousHealth : o.health();
            double oldDistance = previous ? previousDistance : o.horizontal_distance();
            hurt = o.health() < hp ? 1 : 0.82 * hurt;
            double delta = clamp((oldDistance - o.horizontal_distance()) * 2, -1.5, 1.5);
            approach += 0.25 * (delta - approach);
            opponentAir += 0.20 * (bit(!o.opponent_on_ground()) - opponentAir);
            double[] memory = {hurt, recentSwing, approach, opponentAir, delta,
                    x[1] * charge, x[4] * charge, x[10] * charge, x[1] * approach, x[6] * charge,
                    x[4] * x[9], x[5] * x[4]};
            System.arraycopy(memory, 0, x, 16, 12);
        }
        if (x.length == 36) {
            double descent = bit(o.falling() && !o.sprinting());
            double reach = x[9];
            double[] timing = {bit(o.on_ground()), descent, descent * charge, descent * reach,
                    bit(o.rising()) * charge, bit(o.falling() && o.sprinting()) * charge,
                    bit(o.target_under_crosshair()), x[15] * reach};
            System.arraycopy(timing, 0, x, 28, timing.length);
        }
        double[] y = new double[4];
        for (int row = 0; row < 4; row++) for (int col = 0; col < x.length; col++) y[row] += weights[row][col] * x[col];
        boolean far = o.horizontal_distance() > 8;
        Action action = Action.of(far || y[0] > 0.2, !far && y[0] < -0.2,
                !far && y[1] > 0, false, o.on_ground() && y[2] > 0, o.eye_to_target() < 3 && y[3] > 0);
        if (hasMemory) {
            previous = true; previousHealth = o.health(); previousDistance = o.horizontal_distance();
            recentSwing = action.click() ? 1 : recentSwing * 0.72;
        }
        return action;
    }
}
