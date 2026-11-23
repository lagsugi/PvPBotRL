// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.api;

import org.bukkit.entity.Player;

/** One bot and one episode. Use a fresh handle for each round. All calls are on the main server thread. */
public interface BotHandle extends AutoCloseable {
    /** Model and latency selected for this handle. Scripted policies have no weights hash. */
    record Info(String modelId, String modelSha256, String policyVersion, int rttMs) {}

    /** Returns the Bukkit player so the host can equip the bot. */
    Player player();

    /** Returns the resolved ID, which can be used to spawn another round with the same weights. */
    String modelId();

    /** Returns metadata suitable for match results. */
    Info info();

    /** Starts or resumes combat with the same human opponent. Both fighters need the supported sword kit. */
    void start(Player opponent);

    /** Clears queued inputs and pauses the round, including the other handle if paired. Retains policy state. */
    void pause();

    /** Releases the bot. Its pair partner stops and is released on the next runtime tick. */
    @Override
    void close();
}
