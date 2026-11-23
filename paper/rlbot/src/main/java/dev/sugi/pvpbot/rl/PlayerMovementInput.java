// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

/** LocalPlayer.modifyInput and KeyboardInput, preserving their float arithmetic. */
final class PlayerMovementInput {
    record Axes(float sideways, float forward) {}

    static Axes shape(int sideways, int forward, boolean crouching, float sneakSpeed) {
        float x = sideways, z = forward;
        float length = (float) Math.sqrt(x * x + z * z);
        if (length < 1e-4f) return new Axes(0, 0);
        x /= length; z /= length;
        x *= 0.98f; z *= 0.98f;
        if (crouching) { x *= sneakSpeed; z *= sneakSpeed; }
        length = (float) Math.sqrt(x * x + z * z);
        if (length <= 0) return new Axes(0, 0);
        float dx = x * (1.0f / length), dz = z * (1.0f / length);
        float ax = Math.abs(dx), az = Math.abs(dz);
        float tan = az > ax ? ax / az : az / ax;
        float scale = Math.min(length * (float) Math.sqrt(1.0f + tan * tan), 1.0f);
        return new Axes(dx * scale, dz * scale);
    }

    private PlayerMovementInput() {}
}
