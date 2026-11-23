// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl.policy;

/** Additional values captured with the ordinary observation, before the ping delay. */
public record RecurrentInput(float opponent_health, long opponent_health_age_ns, long health_age_ns,
                             int jump_delay, int miss_time, double fall_distance,
                             double height_above_floor, float remaining_fraction) {}
