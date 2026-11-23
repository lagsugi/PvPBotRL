// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import dev.sugi.pvpbot.rl.policy.Action;
import dev.sugi.pvpbot.rl.policy.BotPolicy;
import java.io.IOException;
import java.util.logging.Level;
import org.bukkit.Bukkit;
import org.bukkit.GameRules;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.entity.Player;
import org.bukkit.inventory.ItemStack;

final class RlOpponent {
    private final RlPlugin plugin;
    private final RlRuntime runtime;
    private final BotAvatar avatar;
    private final BotPolicy policy;
    private final MotionTrace trace;
    private final RlSettings settings;
    private final int floorY;
    private final BotPing<BotAvatar.Sample> ping;
    private final BotPing<PendingAction> uplink;
    private boolean removing;
    private boolean closing;
    private boolean removed;
    private long pauseCount;
    private LiveFrame nextFrame;
    private Throwable inputFailure;

    private record LiveFrame(Player enemy, int roundTick, float remaining) {}
    private record PendingAction(int opponentId, BotAvatar.Sample sample, Action action, boolean aim, Integer targetId) {}

    RlOpponent(RlPlugin plugin, RlRuntime runtime, BotAvatar avatar, BotPolicy policy,
               MotionTrace trace, RlSettings settings, int floorY) {
        this.plugin = plugin;
        this.runtime = runtime;
        this.avatar = avatar;
        this.policy = policy;
        this.trace = trace;
        this.settings = settings;
        this.floorY = floorY;
        ping = new BotPing<>(settings.rttMs());
        uplink = BotPing.oneWay(settings.rttMs());
    }

    Player getPlayer() { return avatar.player(); }
    boolean spawned() { return avatar.spawned(); }
    boolean removed() { return removed; }
    Throwable inputFailure() { return inputFailure; }

    void arm(Player enemy, int roundTick, float remaining) {
        nextFrame = new LiveFrame(enemy, roundTick, remaining);
    }

    static boolean standsOnFloor(Location spawn, int floorY) {
        return spawn.getWorld() != null && floorY > spawn.getWorld().getMinHeight()
                && floorY < spawn.getWorld().getMaxHeight() && Math.abs(spawn.getY() - floorY) <= 0.01
                && spawn.getWorld().getBlockAt(spawn.getBlockX(), floorY - 1, spawn.getBlockZ()).getType().isOccluding();
    }

    void validateEquipment(Player human) {
        boolean fixed = settings.fixedEquipment();
        requireSwordKit(getPlayer(), fixed);
        requireSwordKit(human, fixed);
        if (fixed && !Boolean.FALSE.equals(human.getWorld().getGameRuleValue(GameRules.NATURAL_HEALTH_REGENERATION))) {
            throw new IllegalArgumentException("The fixed-equipment arena requires natural health regeneration off.");
        }
    }

    void tickStart() {
        LiveFrame frame = nextFrame;
        nextFrame = null;
        if (removing || inputFailure != null) return;
        try {
            if (frame == null || !avatar.spawned() || !frame.enemy().isOnline() || frame.enemy().isDead()
                    || frame.enemy().getWorld() != getPlayer().getWorld()) {
                pause();
                return;
            }
            applyFrame(frame);
        } catch (RuntimeException | LinkageError e) {
            inputFailure = e;
            try { pause(); }
            catch (RuntimeException | LinkageError cleanup) { e.addSuppressed(cleanup); }
        }
    }

    private void applyFrame(LiveFrame frame) {
        if (!plugin.parityUsesReplica(getPlayer())) {
            applyDelayedFrame(frame);
            return;
        }
        Player enemy = frame.enemy();
        int tick = frame.roundTick();
        long now = tick * BotPing.TICK_NS;
        var replica = plugin.parityReplicaFrame();
        if (replica == null) return;
        var attackUplink = plugin.parityAttackUplink();
        long observationOffset = attackUplink == null ? ping.oneWayNs() : 0;
        var sample = avatar.observe(tick, observationOffset, frame.remaining(), floorY, replica, true);
        plugin.parityObservation(getPlayer(), sample, null, false);
        if (trace != null) trace.record("sample", "roundTick=" + tick + " timeNs=" + sample.observation().time_ns());
        BotAvatar.Sample ready = sample;
        if (attackUplink == null) {
            ping.send(now, sample);
            ready = ping.poll(now);
        }
        if (ready == null) return;
        Action action = decide(ready);
        Action diagnostic = plugin.parityAction(getPlayer());
        if (diagnostic != null) action = diagnostic;
        if (trace != null) trace.record("apply", "sampleRoundNs=" + (ready.observation().time_ns() - observationOffset)
                + " appliedRoundNs=" + now + " keys=" + action.keys());
        boolean attempted = avatar.apply(action, ready, enemy, diagnostic == null, attackUplink);
        recordAction(ready, action, attempted);
    }

    private void applyDelayedFrame(LiveFrame frame) {
        long now = (long) Bukkit.getCurrentTick() * BotPing.TICK_NS;
        long pauses = pauseCount;
        applyPending(frame, now);
        if (pauseCount != pauses || removing || !avatar.spawned() || frame.enemy().isDead() || !frame.enemy().isOnline()) return;

        var received = avatar.receiveOpponent(frame.enemy(), uplink.oneWayNs(), plugin.parityOpponentTrace(getPlayer()));
        if (received == null) {
            avatar.idle();
            return;
        }

        var sample = avatar.observe(frame.roundTick(), 0, frame.remaining(), floorY, received, false);
        plugin.parityObservation(getPlayer(), sample, null, false);
        Action action = decide(sample);
        Action diagnostic = plugin.parityAction(getPlayer());
        if (diagnostic != null) action = diagnostic;
        boolean aim = diagnostic == null;
        int opponentId = frame.enemy().getEntityId();
        Integer target = action.click() && avatar.selectsOpponent(sample, aim) ? opponentId : null;
        uplink.send(now, new PendingAction(opponentId, sample, action, aim, target));
        applyPending(frame, now);
    }

    private Action decide(BotAvatar.Sample sample) {
        return policy == null ? Action.IDLE : plugin.parityPolicyAction(getPlayer(), policy, sample);
    }

    private void applyPending(LiveFrame frame, long now) {
        PendingAction pending;
        while (!removing && !frame.enemy().isDead() && (pending = uplink.poll(now)) != null) {
            if (pending.opponentId() != frame.enemy().getEntityId()) continue;
            if (trace != null) trace.record("apply", "sampleRoundNs=" + pending.sample().observation().time_ns()
                    + " appliedRoundNs=" + frame.roundTick() * BotPing.TICK_NS + " keys=" + pending.action().keys());
            avatar.applyReceived(pending.action(), pending.sample(), pending.aim(), pending.targetId());
            recordAction(pending.sample(), pending.action(), pending.targetId() != null);
        }
    }

    private void recordAction(BotAvatar.Sample sample, Action action, boolean attempted) {
        plugin.parityObservation(getPlayer(), sample, action, attempted);
        if (trace != null && action.click()) trace.record("attack", "hitAttempt=" + attempted);
    }

    private static void requireSwordKit(Player player, boolean fixedEquipment) {
        var inv = player.getInventory();
        ItemStack[] storage = inv.getStorageContents();
        if (!isItem(storage[0], Material.DIAMOND_SWORD, false, fixedEquipment) || inv.getHeldItemSlot() != 0
                || !isItem(inv.getHelmet(), Material.DIAMOND_HELMET, true, fixedEquipment)
                || !isItem(inv.getChestplate(), Material.DIAMOND_CHESTPLATE, true, fixedEquipment)
                || !isItem(inv.getLeggings(), Material.DIAMOND_LEGGINGS, true, fixedEquipment)
                || !isItem(inv.getBoots(), Material.DIAMOND_BOOTS, true, fixedEquipment)
                || !inv.getItemInOffHand().getType().isAir()) {
            throw new IllegalArgumentException("Invalid RL equipment on " + player.getName());
        }
        for (int i = 1; i < storage.length; i++) {
            if (storage[i] != null && !storage[i].getType().isAir()) {
                throw new IllegalArgumentException("Unexpected RL kit inventory item on " + player.getName());
            }
        }
    }

    private static boolean isItem(ItemStack item, Material material, boolean armor, boolean fixedEquipment) {
        if (item == null || item.getType() != material || item.getAmount() != 1) return false;
        if ((item.hasItemMeta() && item.getItemMeta().isUnbreakable()) != fixedEquipment) return false;
        var enchants = item.getEnchantments();
        return armor ? enchants.size() == 1 && enchants.getOrDefault(Enchantment.PROTECTION, 0) == 4
                : enchants.isEmpty();
    }

    void pause() {
        pauseCount++;
        nextFrame = null;
        ping.clear();
        uplink.clear();
        avatar.stopTrackingOpponent();
        avatar.idle();
    }

    void show(Player viewer) {
        if (!removing && viewer.isOnline()) {
            viewer.showEntity(plugin, getPlayer());
            if (trace != null) trace.record("show", "visible=" + viewer.canSee(getPlayer()));
        }
    }

    void remove() {
        if (removed || closing) return;
        removing = true;
        closing = true;
        try {
            pause();
            avatar.close();
            removed = true;
            runtime.forget(this);
            if (trace != null) {
                try { trace.save(); }
                catch (IOException e) { plugin.getLogger().log(Level.WARNING, "Could not save motion trace", e); }
            }
        } finally {
            closing = false;
        }
    }
}
