// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import dev.sugi.pvpbot.common.PlayerRecovery;
import dev.sugi.pvpbot.api.BotHandle;
import java.time.Instant;
import java.util.Locale;
import java.util.Objects;
import java.util.UUID;
import net.kyori.adventure.bossbar.BossBar;
import net.kyori.adventure.text.Component;
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
    enum Phase { PREPARING, COUNTDOWN, RUNNING, PAUSED, FINISHING, CLOSED }
    static final int COUNTDOWN = 60;
    static final int MAX_TICKS = 6_000;
    static final String[] SIDES = {"A", "B"};

    static final class Fighter {
        final Player human;
        final String requestedModel;
        BotHandle bot;
        BotHandle.Info botInfo;
        Player entity;
        Location safe;
        double damage;
        boolean died;
        boolean botClosed;

        Fighter(Player human, String model) {
            this.human = human;
            entity = human;
            requestedModel = model;
        }

        String displayName() { return entity == null ? requestedModel : entity.getName(); }
        boolean dead() { return died || entity != null && (entity.getHealth() <= 0 || human != null && human.isDead()); }
        double maxHp() {
            var attribute = entity == null ? null : entity.getAttribute(Attribute.MAX_HEALTH);
            return attribute == null ? 20 : attribute.getValue();
        }
        double hp() { return dead() ? 0 : entity == null ? 20 : Math.clamp(entity.getHealth(), 0, maxHp()); }
        MatchLog.Fighter snapshot() {
            return new MatchLog.Fighter(displayName(), entity == null ? null : entity.getUniqueId().toString(),
                    botInfo == null ? requestedModel : botInfo.modelId(),
                    botInfo == null ? null : botInfo.modelSha256(),
                    botInfo == null ? null : botInfo.policyVersion(),
                    botInfo == null ? null : botInfo.rttMs(), hp(), maxHp(), damage);
        }

    }

    final String id = UUID.randomUUID().toString();
    final String started = Instant.now().toString();
    final String arena;
    final Stage stage;
    final World world;
    final UUID host;
    final Fighter[] fighters;
    final BossBar timer = BossBar.bossBar(Component.text("Starting"), 1f, BossBar.Color.BLUE, BossBar.Overlay.PROGRESS);
    Phase phase = Phase.PREPARING;
    int elapsed;
    int countdown = COUNTDOWN;
    MatchLog log;
    MatchHealthDisplay healthDisplay;

    Duel(StageStore.Entry entry, UUID host, Fighter a, Fighter b) {
        arena = entry.id();
        stage = entry.stage();
        world = stage.world();
        this.host = host;
        fighters = new Fighter[]{a, b};
    }

    String mode() {
        if (fighters[0].human == null) return "Bot vs Bot";
        return fighters[1].human == null ? "Human vs Bot" : "Human vs Human";
    }

    boolean running() { return phase == Phase.RUNNING; }
    boolean started() { return running() || phase == Phase.PAUSED; }

    MatchLog.Result snapshot() {
        return new MatchLog.Result(2, id, started, null, mode(), host.toString(), arena,
                fighters[0].snapshot(), fighters[1].snapshot(),
                null, null, null, elapsed);
    }

    void updateTimer() {
        int ticks = phase == Phase.COUNTDOWN ? countdown : MAX_TICKS - elapsed;
        int seconds = (ticks + 19) / 20;
        String label = switch (phase) {
            case COUNTDOWN -> "Starting in " + seconds;
            case PAUSED -> "Paused";
            default -> String.format(Locale.ROOT, "%d:%02d remaining", seconds / 60, seconds % 60);
        };
        timer.name(Component.text(label));
        timer.progress((float) ticks / (phase == Phase.COUNTDOWN ? COUNTDOWN : MAX_TICKS));
    }

    static void kit(Player player) {
        if (player.getGameMode() != GameMode.SURVIVAL) player.setGameMode(GameMode.SURVIVAL);
        if (player.getGameMode() != GameMode.SURVIVAL) throw new IllegalStateException("Could not set survival mode.");
        player.closeInventory();
        player.setItemOnCursor(ItemStack.empty());
        player.getInventory().clear();
        player.getInventory().setHeldItemSlot(0);
        player.getInventory().setItem(0, item(Material.DIAMOND_SWORD, false));
        player.getInventory().setArmorContents(new ItemStack[]{
                item(Material.DIAMOND_BOOTS, true),
                item(Material.DIAMOND_LEGGINGS, true),
                item(Material.DIAMOND_CHESTPLATE, true),
                item(Material.DIAMOND_HELMET, true)
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

    private static ItemStack item(Material material, boolean armor) {
        var item = new ItemStack(material);
        if (armor) item.addEnchantment(Enchantment.PROTECTION, 4);
        item.editMeta(meta -> meta.setUnbreakable(true));
        return item;
    }
}
