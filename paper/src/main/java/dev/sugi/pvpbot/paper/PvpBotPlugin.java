// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import dev.sugi.pvpbot.common.PlayerRecovery;
import com.destroystokyo.paper.event.server.ServerTickStartEvent;
import dev.sugi.pvpbot.paper.policy.BotPolicy;
import io.papermc.paper.ServerBuildInfo;
import java.io.IOException;
import java.util.List;
import java.util.logging.Level;
import net.citizensnpcs.api.CitizensAPI;
import net.citizensnpcs.api.npc.NPCRegistry;
import org.bukkit.Bukkit;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.plugin.java.JavaPlugin;

public final class PvpBotPlugin extends JavaPlugin implements Listener {
    private NPCRegistry registry;
    private StageStore stages;
    private MatchManager matches;
    private Settings settings;
    private Menus menus;
    private Runnable unregisterExpansion;

    record Settings(int maxTicks, int maxConcurrent, double distance, int botPingMs,
                    boolean durability, boolean detailed, int blockBatch) {}

    Settings settings() { return settings; }

    @Override public void onEnable() {
        try {
            verifyServer();
            saveDefaultConfig();
            readSettings();
            var data = getDataFolder().toPath();
            stages = new StageStore(this);
            var recovery = new PlayerRecovery(data.resolve("recovery"));
            var logs = data.resolve("matches");
            MatchLog.recover(logs);
            for (String style : BotPolicy.STYLES) new BotPolicy(style);
            registry = CitizensAPI.createInMemoryNPCRegistry("pvpbot-duels");
            matches = new MatchManager(this, stages, recovery, registry, logs);
            menus = new Menus(this, matches, stages);
            var commands = new PvpCommands(this, matches, stages, menus);
            menus.commands(commands);
            var command = getCommand("pvpbot");
            command.setExecutor(commands);
            command.setTabCompleter(commands);
            var plugins = Bukkit.getPluginManager();
            plugins.registerEvents(this, this);
            plugins.registerEvents(new ArenaEvents(this, stages, matches, menus), this);
            plugins.registerEvents(menus, this);
            enableMenus();
            for (Player player : Bukkit.getOnlinePlayers()) matches.restore(player);
        } catch (IOException | RuntimeException | LinkageError e) {
            getLogger().log(Level.SEVERE, "PvPBot could not start", e);
            Bukkit.getPluginManager().disablePlugin(this);
        }
    }

    private static void verifyServer() {
        if (!Bukkit.getMinecraftVersion().equals("26.1.2")
                || ServerBuildInfo.buildInfo().buildNumber().orElse(-1) != 74) {
            throw new IllegalStateException("Paper 26.1.2 build 74 is required.");
        }
        var citizens = Bukkit.getPluginManager().getPlugin("Citizens");
        if (citizens == null || !citizens.isEnabled()
                || !citizens.getPluginMeta().getVersion().equals("2.0.44-SNAPSHOT (build 4269)")) {
            throw new IllegalStateException("Citizens build 4269 is required.");
        }
    }

    private void enableMenus() {
        if (Bukkit.getPluginManager().isPluginEnabled("PlaceholderAPI")) {
            try {
                unregisterExpansion = PvpExpansion.enable(this, menus);
            } catch (RuntimeException | LinkageError e) {
                getLogger().log(Level.WARNING, "Could not register PvPBot's PlaceholderAPI expansion", e);
            }
        }
        if (unregisterExpansion != null && Bukkit.getPluginManager().isPluginEnabled("DeluxeMenus")) {
            try {
                menus.bridge(new DeluxeMenuBridge(this));
            } catch (IOException | RuntimeException | LinkageError e) {
                getLogger().log(Level.WARNING, "Could not install PvPBot's DeluxeMenus files", e);
            }
        }
    }

    @Override public void onDisable() {
        if (menus != null) menus.close();
        if (matches != null) matches.shutdown();
        if (unregisterExpansion != null) unregisterExpansion.run();
        if (registry != null) {
            registry.deregisterAll();
            CitizensAPI.removeNamedNPCRegistry("pvpbot-duels");
        }
    }

    void readSettings() throws IOException {
        var disk = new org.bukkit.configuration.file.YamlConfiguration();
        try {
            disk.load(getDataFolder().toPath().resolve("config.yml").toFile());
        } catch (org.bukkit.configuration.InvalidConfigurationException e) {
            throw new IllegalArgumentException("Invalid config.yml. Check the YAML syntax.", e);
        }
        Object distance = disk.get("match.start-distance", 20.0);
        if (!(distance instanceof Number number) || !Double.isFinite(number.doubleValue())
                || number.doubleValue() < 4 || number.doubleValue() > 60) {
            throw new IllegalArgumentException("match.start-distance must be between 4 and 60.");
        }
        settings = new Settings(integer(disk, "match.max-ticks", 6000, 1, 72_000),
                integer(disk, "match.max-concurrent", 8, 1, 64), number.doubleValue(),
                integer(disk, "match.bot-ping-ms", BotPing.DEFAULT_RTT_MS, 0, BotPing.MAX_RTT_MS),
                bool(disk, "match.durability", true), bool(disk, "logging.detailed", false),
                integer(disk, "stage.blocks-per-tick", 512, 1, 4096));
    }

    private static int integer(org.bukkit.configuration.Configuration config, String key, int fallback, int min, int max) {
        if (config.get(key, fallback) instanceof Integer value && value >= min && value <= max) return value;
        throw new IllegalArgumentException(key + " must be an integer from " + min + " to " + max + ".");
    }

    private static boolean bool(org.bukkit.configuration.Configuration config, String key, boolean fallback) {
        if (config.get(key, fallback) instanceof Boolean value) return value;
        throw new IllegalArgumentException(key + " must be true or false.");
    }

    static void require(CommandSender sender, String permission) {
        if (!sender.hasPermission(permission)) throw new IllegalStateException("You need the " + permission + " permission.");
    }

    static Player player(CommandSender sender) {
        if (sender instanceof Player player && !player.hasMetadata("NPC")) return player;
        throw new IllegalArgumentException("Run this command as a player.");
    }

    static List<String> realPlayers() {
        return Bukkit.getOnlinePlayers().stream().filter(player -> !player.hasMetadata("NPC"))
                .map(Player::getName).sorted(String.CASE_INSENSITIVE_ORDER).toList();
    }

    @EventHandler
    public void tick(ServerTickStartEvent event) {
        matches.tick(event.getTickNumber());
        stages.tick(settings.blockBatch());
        if (event.getTickNumber() % 5 == 0) menus.publish();
    }
}
