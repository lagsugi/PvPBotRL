// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import org.bukkit.configuration.file.FileConfiguration;

record RlSettings(int rttMs, int matchLimit, boolean diagnostics, String equipmentProfile) {
    static final int MATCH_SECONDS = 300;
    static final String FINITE_EQUIPMENT = "finite_durability";
    static final String FIXED_EQUIPMENT = "sword_pvp_fixed_equipment_v1";

    boolean fixedEquipment() { return FIXED_EQUIPMENT.equals(equipmentProfile); }

    static RlSettings read(FileConfiguration config) {
        int rtt = config.getInt("bot-rtt-ms", -1);
        int limit = config.getInt("max-active-matches", -1);
        String profile = config.getString("equipment-profile", FINITE_EQUIPMENT);
        if (rtt < 0 || rtt > BotPing.MAX_RTT_MS) throw new IllegalArgumentException("Invalid bot-rtt-ms.");
        if (limit < 1 || limit > 8) throw new IllegalArgumentException("max-active-matches must be 1–8.");
        if (!FINITE_EQUIPMENT.equals(profile) && !FIXED_EQUIPMENT.equals(profile)) {
            throw new IllegalArgumentException("Unknown equipment-profile.");
        }
        return new RlSettings(rtt, limit, config.getBoolean("diagnostics"), profile);
    }
}
