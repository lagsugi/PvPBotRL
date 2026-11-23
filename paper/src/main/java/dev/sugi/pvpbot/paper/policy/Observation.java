// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper.policy;

/** Positions: blocks; velocity/displacement: blocks/tick; time/age: nanoseconds. */
public record Observation(
        long time_ns, float health, Vec velocity, float attack_charge,
        boolean sprinting, boolean sneaking, boolean walking, boolean on_ground,
        boolean rising, boolean falling, float yaw, float pitch,
        Vec relative_position, Vec opponent_velocity, Vec opponent_displacement,
        boolean opponent_sprinting, boolean opponent_sneaking, boolean opponent_walking,
        boolean opponent_on_ground, boolean opponent_rising, boolean opponent_falling,
        double distance, double horizontal_distance, double eye_to_target,
        boolean target_under_crosshair, long position_age_ns, long metadata_age_ns,
        long velocity_age_ns, long health_age_ns, boolean sword_present, boolean[] armor_present) {
    public record Vec(double x, double y, double z) {}
}
