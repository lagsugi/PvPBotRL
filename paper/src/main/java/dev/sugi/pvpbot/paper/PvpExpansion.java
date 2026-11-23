// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import me.clip.placeholderapi.expansion.PlaceholderExpansion;
import org.bukkit.OfflinePlayer;

final class PvpExpansion extends PlaceholderExpansion {
    private final PvpBotPlugin plugin;
    private final Menus menus;

    private PvpExpansion(PvpBotPlugin plugin, Menus menus) {
        this.plugin = plugin;
        this.menus = menus;
    }

    static Runnable enable(PvpBotPlugin plugin, Menus menus) {
        var expansion = new PvpExpansion(plugin, menus);
        if (!expansion.register()) throw new IllegalStateException("The pvpbot expansion ID is already in use.");
        return expansion::unregister;
    }

    @Override public String getIdentifier() { return "pvpbot"; }
    @Override public String getAuthor() { return "PvPBot"; }
    @Override public String getVersion() { return plugin.getPluginMeta().getVersion(); }
    @Override public boolean persist() { return true; }
    @Override public String onRequest(OfflinePlayer player, String key) {
        return player == null ? "" : menus.placeholder(player.getUniqueId(), key);
    }
}
