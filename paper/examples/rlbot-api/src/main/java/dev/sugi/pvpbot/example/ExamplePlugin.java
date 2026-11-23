// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import dev.sugi.pvpbot.api.PvPBotApi;
import java.io.IOException;
import java.util.List;
import java.util.Locale;
import java.util.logging.Level;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.plugin.java.JavaPlugin;

public final class ExamplePlugin extends JavaPlugin {
    private static final String DEFAULT_MODEL = "recurrent-ppo";
    private static final String FINITE_EQUIPMENT = "finite_durability";
    private static final String FIXED_EQUIPMENT = "sword_pvp_fixed_equipment_v1";
    private PvPBotApi api;
    private PracticeMatch match;
    private boolean fixedEquipment;

    @Override public void onEnable() {
        api = getServer().getServicesManager().load(PvPBotApi.class);
        if (api == null) {
            getLogger().severe("PvPBotRL did not register its API service. Check the server startup log.");
            getServer().getPluginManager().disablePlugin(this);
            return;
        }
        saveDefaultConfig();
        String profile = getConfig().getString("equipment-profile", FINITE_EQUIPMENT);
        if (!FINITE_EQUIPMENT.equals(profile) && !FIXED_EQUIPMENT.equals(profile)) {
            getLogger().severe("Unknown practice equipment-profile. Check config.yml.");
            getServer().getPluginManager().disablePlugin(this);
            return;
        }
        fixedEquipment = FIXED_EQUIPMENT.equals(profile);
        match = new PracticeMatch(this, api);
        getServer().getPluginManager().registerEvents(match, this);
        getServer().getOnlinePlayers().forEach(match::scheduleRecovery);
    }

    @Override public void onDisable() {
        if (match != null) match.close();
    }

    boolean fixedEquipment() { return fixedEquipment; }

    @Override public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        String action = args.length == 0 ? "start" : args[0].toLowerCase(Locale.ROOT);
        if (args.length > (action.equals("start") ? 2 : 1)) return false;
        if (action.equals("list")) {
            sender.sendMessage("Available models • " + String.join(", ", api.models()));
            return true;
        }
        if (action.equals("status")) {
            sender.sendMessage(match.status());
            return true;
        }
        if (!(sender instanceof Player human)) {
            sender.sendMessage("Run this command in Minecraft.");
            return true;
        }
        try {
            switch (action) {
                case "start" -> match.start(human, args.length == 2 ? args[1].toLowerCase(Locale.ROOT) : DEFAULT_MODEL);
                case "rematch" -> match.rematch(human);
                case "end", "leave" -> match.leave(human);
                case "recover" -> match.requestRecovery(human);
                case "pause", "resume" -> {
                    if (sender.hasPermission("pvpbotrlexample.admin")) match.pause(human, action.equals("pause"));
                    else sender.sendMessage("Only a practice administrator can pause a duel.");
                }
                default -> { return false; }
            }
        } catch (IllegalArgumentException | IllegalStateException error) {
            human.sendMessage(error.getMessage());
        } catch (IOException | RuntimeException | LinkageError error) {
            getLogger().log(Level.WARNING, "Could not handle a practice command", error);
            human.sendMessage("The practice command failed. Check server diagnostics.");
        }
        return true;
    }

    @Override public List<String> onTabComplete(CommandSender sender, Command command, String alias, String[] args) {
        List<String> choices = switch (args.length) {
            case 1 -> sender.hasPermission("pvpbotrlexample.admin")
                    ? List.of("start", "rematch", "leave", "list", "status", "recover", "pause", "resume")
                    : List.of("start", "rematch", "leave", "list", "status", "recover");
            case 2 -> args[0].equalsIgnoreCase("start") ? api.models() : List.of();
            default -> List.of();
        };
        String prefix = args.length == 0 ? "" : args[args.length - 1].toLowerCase(Locale.ROOT);
        return choices.stream().filter(choice -> choice.startsWith(prefix)).toList();
    }
}
