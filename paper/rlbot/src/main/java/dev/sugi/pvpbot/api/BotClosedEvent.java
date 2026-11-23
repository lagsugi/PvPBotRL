// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.api;

import java.util.UUID;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
import org.bukkit.plugin.Plugin;

/** Fired on the server thread after a standalone bot and its capacity slot have been released. */
public final class BotClosedEvent extends Event {
    public enum Reason {
        EXPLICIT_CLOSE, PARTICIPANT_LOST, OWNER_DISABLED, RUNTIME_DISABLED, INPUT_FAILURE
    }

    private static final HandlerList HANDLERS = new HandlerList();
    private final Plugin owner;
    private final UUID botId;
    private final Reason reason;

    public BotClosedEvent(Plugin owner, UUID botId, Reason reason) {
        this.owner = owner;
        this.botId = botId;
        this.reason = reason;
    }

    public Plugin getOwner() { return owner; }
    public UUID getBotId() { return botId; }
    public Reason getReason() { return reason; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
