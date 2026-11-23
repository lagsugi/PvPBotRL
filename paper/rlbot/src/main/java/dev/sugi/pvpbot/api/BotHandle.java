// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.api;

import org.bukkit.entity.Player;

/** One bot and one episode. Use a fresh handle for each round. All calls are on the main server thread. */
public interface BotHandle extends AutoCloseable {
    /** Returns the Bukkit player so the host can equip the bot. */
    Player player();

    /** Returns the resolved ID, which can be used to spawn another round with the same weights. */
    String modelId();

    /** Starts combat or resumes it with the same opponent. Both fighters must have the supported sword kit. */
    void start(Player opponent);

    /** Stops combat and clears queued inputs while retaining the round's policy state. */
    void pause();

    /** Releases the bot. Subsequent calls are harmless. */
    @Override
    void close();
}
