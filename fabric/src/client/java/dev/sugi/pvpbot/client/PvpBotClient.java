// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client;

import dev.sugi.pvpbot.PvpBot;
import net.fabricmc.api.ClientModInitializer;

public final class PvpBotClient implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        PvpBot.LOGGER.info("PvP Bot client initialized.");
    }
}
