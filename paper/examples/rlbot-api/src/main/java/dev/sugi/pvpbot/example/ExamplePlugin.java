// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import com.destroystokyo.paper.event.server.ServerTickEndEvent;
import dev.sugi.pvpbot.api.BotClosedEvent;
import dev.sugi.pvpbot.api.PvPBotApi;
import dev.sugi.pvpbot.common.PlayerRecovery;
import java.io.IOException;
import java.util.List;
import java.util.Objects;
import java.util.logging.Level;
import org.bukkit.Bukkit;
import org.bukkit.command.CommandSender;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.plugin.java.JavaPlugin;

public final class ExamplePlugin extends JavaPlugin implements Listener {
    private StageStore stages;
    private MatchManager matches;
    private Settings settings;

    record Settings(int maxConcurrent, double distance, int blockBatch) {}

    Settings settings() { return settings; }

    @Override public void onEnable() {
        try {
            var api = Objects.requireNonNull(getServer().getServicesManager().load(PvPBotApi.class),
                    "PvPBotRL did not register its API service.");
            saveDefaultConfig();
            readSettings();
            stages = new StageStore(this);
            stages.addPractice(new PracticeArena(this).prepare());
            var data = getDataFolder().toPath();
            var logs = data.resolve("matches");
            MatchLog.recover(logs);
            matches = new MatchManager(this, stages, new PlayerRecovery(data.resolve("recovery")), api, logs);
            var command = Objects.requireNonNull(getCommand("rlbotexample"));
            var commands = new PvpCommands(this, matches, stages);
            command.setExecutor(commands);
            command.setTabCompleter(commands);
            var plugins = getServer().getPluginManager();
            plugins.registerEvents(this, this);
            plugins.registerEvents(new ArenaEvents(this, stages, matches), this);
            for (Player player : getServer().getOnlinePlayers()) matches.restore(player);
        } catch (IOException | RuntimeException | LinkageError error) {
            getLogger().log(Level.SEVERE, "PvPBotRLExample could not start", error);
            getServer().getPluginManager().disablePlugin(this);
        }
    }

    @Override public void onDisable() {
        if (matches != null) matches.shutdown();
    }

    @EventHandler public void tick(ServerTickEndEvent event) {
        matches.tick(event.getTickNumber());
        stages.tick(settings.blockBatch());
    }

    @EventHandler public void botClosed(BotClosedEvent event) { matches.botClosed(event); }

    void readSettings() throws IOException {
        var disk = new YamlConfiguration();
        try {
            disk.load(getDataFolder().toPath().resolve("config.yml").toFile());
        } catch (org.bukkit.configuration.InvalidConfigurationException error) {
            throw new IllegalArgumentException("Invalid config.yml. Check the YAML syntax.", error);
        }
        Object distance = disk.get("match.start-distance", 20.0);
        if (!(distance instanceof Number number) || !Double.isFinite(number.doubleValue())
                || number.doubleValue() < 4 || number.doubleValue() > 60) {
            throw new IllegalArgumentException("match.start-distance must be between 4 and 60.");
        }
        String profile = disk.getString("equipment-profile", "sword_pvp_fixed_equipment_v1");
        if (!"sword_pvp_fixed_equipment_v1".equals(profile) && !"finite_durability".equals(profile)) {
            throw new IllegalArgumentException("Unknown equipment-profile. Check config.yml.");
        }
        settings = new Settings(integer(disk, "match.max-concurrent", 8, 1, 64), number.doubleValue(),
                integer(disk, "stage.blocks-per-tick", 512, 1, 4096));
    }

    private static int integer(YamlConfiguration disk, String key, int fallback, int min, int max) {
        if (disk.get(key, fallback) instanceof Integer value && value >= min && value <= max) return value;
        throw new IllegalArgumentException(key + " must be an integer from " + min + " to " + max + ".");
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
}
