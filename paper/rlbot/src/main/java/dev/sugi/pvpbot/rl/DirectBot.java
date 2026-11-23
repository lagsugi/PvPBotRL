// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import dev.sugi.pvpbot.api.BotClosedEvent;
import dev.sugi.pvpbot.api.BotClosedEvent.Reason;
import dev.sugi.pvpbot.api.BotHandle;
import java.util.UUID;
import java.util.logging.Level;
import net.citizensnpcs.api.CitizensAPI;
import org.bukkit.Bukkit;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;

final class DirectBot implements BotHandle {
    private final Plugin owner;
    private final RlOpponent body;
    private final UUID id;
    private final String modelId;
    private Player opponent;
    private int roundTick;
    private boolean running;
    private Reason closingReason;
    private boolean notified;

    DirectBot(Plugin owner, RlOpponent body, String modelId) {
        this.owner = owner;
        this.body = body;
        this.id = body.getPlayer().getUniqueId();
        this.modelId = modelId;
    }

    Plugin owner() { return owner; }
    UUID id() { return id; }

    @Override public Player player() {
        requireOpen();
        return body.getPlayer();
    }

    @Override public String modelId() {
        requireOpen();
        return modelId;
    }

    @Override public void start(Player opponent) {
        requireOpen();
        if (opponent == null || opponent.getUniqueId().equals(id) || !opponent.isOnline() || opponent.isDead()
                || Bukkit.getPlayer(opponent.getUniqueId()) == null || opponent.getWorld() != body.getPlayer().getWorld()
                || isNpc(opponent)) {
            throw new IllegalArgumentException("Expected an online human opponent in the bot's world.");
        }
        if (this.opponent != null && this.opponent != opponent) {
            throw new IllegalArgumentException("A different opponent requires a new bot handle.");
        }
        if (running) return;
        boolean firstStart = this.opponent == null;
        if (firstStart) body.validateEquipment(opponent);
        this.opponent = opponent;
        running = true;
        if (firstStart) body.show(opponent);
    }

    private static boolean isNpc(Player player) {
        if (CitizensAPI.getNPCRegistry().isNPC(player)) return true;
        for (var registry : CitizensAPI.getNPCRegistries()) {
            if (registry.isNPC(player)) return true;
        }
        return false;
    }

    @Override public void pause() {
        requireOpen();
        if (!running) return;
        running = false;
        body.pause();
    }

    void tickStart() {
        if (closingReason != null) {
            if (Bukkit.getCurrentTick() % 20 == 0) closeSafely(closingReason);
            return;
        }
        if (!owner.isEnabled()) {
            closeSafely(Reason.OWNER_DISABLED);
            return;
        }
        if (!body.spawned() || body.getPlayer().isDead() || (opponent != null
                && (!opponent.isOnline() || opponent.isDead() || opponent.getWorld() != body.getPlayer().getWorld()))) {
            closeSafely(Reason.PARTICIPANT_LOST);
            return;
        }
        if (!running) return;
        int tick = roundTick;
        if (roundTick < Integer.MAX_VALUE) roundTick++;
        float remaining = Math.max(0f, 1f - tick / (RlSettings.MATCH_SECONDS * 20f));
        body.arm(opponent, tick, remaining);
        body.tickStart();
        if (body.inputFailure() != null && closingReason == null) {
            owner.getLogger().log(Level.WARNING, "RL bot input failed for " + id, body.inputFailure());
            closeSafely(Reason.INPUT_FAILURE);
        }
    }

    private void closeSafely(Reason reason) {
        try { close(reason); }
        catch (RuntimeException | LinkageError e) {
            owner.getLogger().log(Level.SEVERE, "Could not close RL bot " + id, e);
        }
    }

    private void requireOpen() {
        RlRuntime.requireMainThread();
        if (closingReason != null) throw new IllegalStateException("The bot handle is closed.");
    }

    @Override public void close() { close(Reason.EXPLICIT_CLOSE); }

    void close(Reason reason) {
        RlRuntime.requireMainThread();
        if (notified) return;
        if (closingReason == null) closingReason = reason;
        running = false;
        body.remove();
        if (body.removed() && !notified) {
            notified = true;
            Bukkit.getPluginManager().callEvent(new BotClosedEvent(owner, id, closingReason));
        }
    }
}
