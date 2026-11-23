// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl.policy;

public record Action(Keys keys, Look look, boolean click, boolean attack_held) {
    public record Keys(boolean forward, boolean backward, boolean left, boolean right, boolean jump, boolean sneak) {}
    public record Look(String mode) {}
    private static final Look TRACK_TARGET = new Look("track_target");
    public static Action of(boolean f, boolean b, boolean l, boolean r, boolean j, boolean click) {
        return new Action(new Keys(f, b, l, r, j, false), TRACK_TARGET, click, false);
    }
    public static final Action IDLE = of(false, false, false, false, false, false);
}
