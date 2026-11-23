// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.api;

import java.util.List;
import org.bukkit.Location;
import org.bukkit.plugin.Plugin;

/**
 * Creates sword practice opponents for Paper match plugins. Look up the service
 * through Bukkit's ServicesManager. Call its methods on the main server thread.
 */
public interface PvPBotApi {
    /** Returns the immutable list of selectable model IDs. */
    List<String> models();

    /**
     * Spawns an idle bot on a flat combat floor. Equip it before starting combat.
     * The owner must be enabled. Its bots are closed when it disables.
     *
     * @param modelId a selectable model ID or a resolved model ID from a prior round
     */
    BotHandle spawn(Plugin owner, Location spawn, String modelId);
}
