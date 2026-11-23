// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import dev.sugi.pvpbot.common.PlayerRecovery;
import dev.sugi.pvpbot.paper.policy.BotPolicy;
import java.io.IOException;
import java.time.Instant;
import java.util.Locale;
import java.util.Objects;
import java.util.UUID;
import org.bukkit.GameMode;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.attribute.Attribute;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.entity.Player;
import org.bukkit.inventory.ItemStack;
import org.bukkit.util.Vector;

final class Duel {
    enum Phase { PREPARING, COUNTDOWN, RUNNING, FINISHING, CLOSED }
    static final int COUNTDOWN = 60;
    static final String[] SIDES = {"A", "B"};

    static final class Fighter {
        final Player human;
        final BotPolicy policy;
        final BotPing<BotAvatar.Sample> ping;
        BotAvatar avatar;
        Player entity;
        Location safe;
        double damage;
        boolean died;

        Fighter(Player human, String bot, int pingMs) throws IOException {
            this.human = human;
            entity = human;
            policy = bot == null ? null : new BotPolicy(bot);
            ping = bot == null ? null : new BotPing<>(pingMs);
        }

        String name() { return human == null ? policy.style() : human.getName(); }
        String displayName() { return avatar == null || entity == null ? name() : entity.getName(); }
        boolean dead() { return died || entity != null && (entity.getHealth() <= 0 || human != null && human.isDead()); }
        double maxHp() {
            var attribute = entity == null ? null : entity.getAttribute(Attribute.MAX_HEALTH);
            return attribute == null ? 20 : attribute.getValue();
        }
        double hp() { return dead() ? 0 : entity == null ? 20 : Math.clamp(entity.getHealth(), 0, maxHp()); }
        String health() { return number(hp()) + "/" + number(maxHp()) + " HP"; }

        MatchLog.Fighter snapshot(int pingMs) {
            String hash = policy == null ? null : policy.hash();
            return new MatchLog.Fighter(displayName(), human == null ? null : human.getUniqueId().toString(),
                    policy == null ? null : policy.style(), hash != null && !hash.startsWith("script-") ? hash : null,
                    policy == null ? null : policy.version(), ping == null ? null : pingMs, hp(), maxHp(), damage);
        }

    }

    final String id = UUID.randomUUID().toString();
    final String started = Instant.now().toString();
    final String arena;
    final Stage stage;
    final World world;
    final UUID host;
    final Fighter[] fighters;
    final PvpBotPlugin.Settings settings;
    Phase phase = Phase.PREPARING;
    int elapsed;
    int countdown = COUNTDOWN;
    MatchLog log;
    BotHealthDisplay healthDisplay;

    Duel(StageStore.Entry entry, UUID host, Fighter a, Fighter b, PvpBotPlugin.Settings settings) {
        arena = entry.id();
        stage = entry.stage();
        world = stage.world();
        this.host = host;
        fighters = new Fighter[]{a, b};
        this.settings = settings;
    }

    String mode() {
        if (fighters[0].human == null) return "Bot vs Bot";
        return fighters[1].human == null ? "Human vs Bot" : "Human vs Human";
    }

    boolean running() { return phase == Phase.RUNNING; }

    MatchLog.Result snapshot() {
        return new MatchLog.Result(2, id, started, null, mode(), host.toString(), arena,
                fighters[0].snapshot(settings.botPingMs()), fighters[1].snapshot(settings.botPingMs()),
                null, null, null, elapsed);
    }

    String health() {
        return "A: " + fighters[0].displayName() + " " + fighters[0].health() + " | B: "
                + fighters[1].displayName() + " " + fighters[1].health();
    }

    void botInputs() throws IOException {
        long nowNs = elapsed * BotPing.TICK_NS;
        var samples = new BotAvatar.Sample[2];
        for (int i = 0; i < 2; i++) {
            Fighter fighter = fighters[i];
            if (fighter.avatar == null) continue;
            fighter.ping.send(nowNs, fighter.avatar.observe(fighters[1 - i].entity, elapsed, fighter.ping.oneWayNs(),
                    settings.maxTicks(), stage.floorY() + 1));
            samples[i] = fighter.ping.poll(nowNs);
        }
        // Alternate application order when both bots can hit on the same tick.
        for (int order = 0; order < 2; order++) {
            int i = (elapsed + order) % 2;
            Fighter fighter = fighters[i];
            var sample = samples[i];
            if (sample == null || fighter.dead() || fighters[1 - i].dead()) continue;
            var action = fighter.policy.act(sample.observation(), sample.recurrent());
            boolean attempted = fighter.avatar.apply(action, sample, fighters[1 - i].entity);
            log.step(SIDES[i], sample, action, attempted, nowNs);
        }
        elapsed++;
    }

    static String number(double hp) { return String.format(Locale.ROOT, "%.1f", hp); }

    static void kit(Player player, boolean durability) {
        if (player.getGameMode() != GameMode.SURVIVAL) player.setGameMode(GameMode.SURVIVAL);
        if (player.getGameMode() != GameMode.SURVIVAL) throw new IllegalStateException("Could not set survival mode.");
        player.closeInventory();
        player.setItemOnCursor(ItemStack.empty());
        player.getInventory().clear();
        player.getInventory().setHeldItemSlot(0);
        player.getInventory().setItem(0, item(Material.DIAMOND_SWORD, false, durability));
        player.getInventory().setArmorContents(new ItemStack[]{
                item(Material.DIAMOND_BOOTS, true, durability),
                item(Material.DIAMOND_LEGGINGS, true, durability),
                item(Material.DIAMOND_CHESTPLATE, true, durability),
                item(Material.DIAMOND_HELMET, true, durability)
        });
        PlayerRecovery.clearEffects(player);
        var health = Objects.requireNonNull(player.getAttribute(Attribute.MAX_HEALTH));
        health.setBaseValue(20);
        player.setHealth(Math.min(20, health.getValue()));
        player.setAbsorptionAmount(0);
        player.setFoodLevel(20);
        player.setSaturation(5);
        player.setExhaustion(0);
        player.setFireTicks(0);
        player.setFreezeTicks(0);
        player.setRemainingAir(player.getMaximumAir());
        player.setFallDistance(0);
        player.setNoDamageTicks(0);
        player.setInvulnerable(false);
        player.setCollidable(false);
        player.setSprinting(false);
        player.setSneaking(false);
        player.setVelocity(new Vector());
        player.setFlying(false);
        player.setAllowFlight(false);
    }

    private static ItemStack item(Material material, boolean armor, boolean durability) {
        var item = new ItemStack(material);
        if (armor) item.addEnchantment(Enchantment.PROTECTION, 4);
        if (!durability) item.editMeta(meta -> meta.setUnbreakable(true));
        return item;
    }
}
