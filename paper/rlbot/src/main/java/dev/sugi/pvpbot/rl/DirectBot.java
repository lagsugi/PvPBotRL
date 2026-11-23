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
    private final Info info;
    private Player opponent;
    private DirectBot partner;
    private boolean firstSeat;
    private int roundTick;
    private boolean running;
    private Reason closingReason;
    private boolean notified;

    DirectBot(Plugin owner, RlOpponent body, Info info) {
        this.owner = owner;
        this.body = body;
        this.id = body.getPlayer().getUniqueId();
        this.info = info;
    }

    Plugin owner() { return owner; }
    UUID id() { return id; }
    DirectBot partner() { return partner; }
    boolean firstThisTick() { return firstSeat == (roundTick % 2 == 0); }

    @Override public Player player() {
        requireOpen();
        return body.getPlayer();
    }

    @Override public String modelId() {
        requireOpen();
        return info.modelId();
    }

    @Override public Info info() {
        RlRuntime.requireMainThread();
        return info;
    }

    @Override public void start(Player opponent) {
        requireOpen();
        if (partner != null) throw new IllegalStateException("Resume paired bots through startPair.");
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

    void startPair(DirectBot other) {
        requireOpen();
        other.requireOpen();
        if (this == other || owner != other.owner || !owner.isEnabled()
                || info.policyVersion() == null || other.info.policyVersion() == null
                || !alive() || !other.alive() || body.getPlayer().getWorld() != other.body.getPlayer().getWorld()) {
            throw new IllegalArgumentException("Expected two live bots owned by the same plugin in one world.");
        }
        if (partner == other && other.partner == this) {
            running = other.running = true;
            return;
        }
        if (opponent != null || other.opponent != null) {
            throw new IllegalStateException("A different opponent requires fresh bot handles.");
        }
        Player a = body.getPlayer();
        Player b = other.body.getPlayer();
        body.validateEquipment(b);
        other.body.validateEquipment(a);
        try {
            a.showEntity(owner, b);
            b.showEntity(owner, a);
        } catch (RuntimeException | LinkageError error) {
            try { a.hideEntity(owner, b); }
            catch (RuntimeException | LinkageError cleanup) { error.addSuppressed(cleanup); }
            try { b.hideEntity(owner, a); }
            catch (RuntimeException | LinkageError cleanup) { error.addSuppressed(cleanup); }
            throw error;
        }
        opponent = b;
        other.opponent = a;
        partner = other;
        other.partner = this;
        firstSeat = true;
        running = other.running = true;
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
        try { pauseSelf(); }
        finally { if (partner != null) partner.pauseSelf(); }
    }

    private void pauseSelf() {
        if (!running) return;
        running = false;
        body.pause();
    }

    private boolean alive() {
        return closingReason == null && body.spawned() && !body.getPlayer().isDead();
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
        if (!alive() || (opponent != null && (opponent.getWorld() != body.getPlayer().getWorld()
                || (partner == null ? !opponent.isOnline() || opponent.isDead() : !partner.alive())))) {
            closeSafely(Reason.PARTICIPANT_LOST);
            return;
        }
        if (!running) return;
        int tick = roundTick;
        if (roundTick < Integer.MAX_VALUE) roundTick++;
        float remaining = Math.max(0f, 1f - tick / (RlSettings.MATCH_SECONDS * 20f));
        body.arm(opponent, tick, remaining, partner != null);
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
        try {
            if (partner != null) partner.pauseSelf();
        } finally {
            body.remove();
            if (body.removed() && !notified) {
                notified = true;
                Bukkit.getPluginManager().callEvent(new BotClosedEvent(owner, id, closingReason));
            }
        }
    }
}
