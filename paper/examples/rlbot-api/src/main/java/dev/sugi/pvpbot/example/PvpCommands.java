// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import java.io.IOException;
import java.util.List;
import java.util.Locale;
import java.util.logging.Level;
import org.bukkit.Bukkit;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.command.TabExecutor;
import org.bukkit.entity.Player;

final class PvpCommands implements TabExecutor {
    private static final String DEFAULT_MODEL = "recurrent-ppo";
    private static final List<String> PUBLIC = List.of("help", "list", "start", "bots", "challenge", "accept",
            "deny", "queue", "matches", "status", "spectate", "view", "leave", "stop", "rematch", "stage", "recover");
    private static final List<String> STAGE_ADMIN = List.of("list", "info", "create", "register", "pos1", "pos2",
            "spawn", "spectator", "validate", "enable", "disable", "remove");

    private final ExamplePlugin plugin;
    private final MatchManager matches;
    private final StageStore stages;
    PvpCommands(ExamplePlugin plugin, MatchManager matches, StageStore stages) {
        this.plugin = plugin;
        this.matches = matches;
        this.stages = stages;
    }

    @Override public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        try {
            execute(sender, args);
        } catch (IllegalArgumentException | IllegalStateException e) {
            sender.sendMessage(e.getMessage() == null ? "Could not complete the operation." : e.getMessage());
        } catch (IOException | RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.SEVERE, "PvPBot command failed", e);
            sender.sendMessage("Could not complete the operation. Check the server log.");
        }
        return true;
    }

    void execute(CommandSender sender, String... args) throws IOException {
        String sub = args.length == 0 ? "start" : lower(args[0]);
        switch (sub) {
            case "help" -> help(sender);
            case "list" -> sender.sendMessage(String.join(", ", matches.models()));
            case "start" -> {
                if (args.length > 3) throw usage("start [bot] [arena]");
                matches.startBot(ExamplePlugin.player(sender), args.length >= 2 ? lower(args[1]) : DEFAULT_MODEL,
                        optionalArena(args, 2));
            }
            case "bots" -> {
                count(args, 3, 4, "bots <botA> <botB> [arena]");
                matches.host(ExamplePlugin.player(sender), lower(args[1]), lower(args[2]), optionalArena(args, 3));
            }
            case "challenge" -> {
                count(args, 2, 3, "challenge <player> [arena]");
                matches.challenge(ExamplePlugin.player(sender), target(args[1]), optionalArena(args, 2));
            }
            case "accept", "deny" -> {
                count(args, 2, 2, sub + " <player>");
                matches.answer(ExamplePlugin.player(sender), target(args[1]).getUniqueId(), sub.equals("accept"));
            }
            case "queue" -> {
                count(args, 2, 2, "queue <join|leave>");
                String action = lower(args[1]);
                if (!List.of("join", "leave").contains(action)) throw usage("queue <join|leave>");
                matches.queue(ExamplePlugin.player(sender), action.equals("join"));
            }
            case "matches" -> {
                if (matches.live().isEmpty()) sender.sendMessage("There are no live matches.");
                for (Duel match : matches.live()) {
                    sender.sendMessage(describe(match));
                }
            }
            case "status" -> status(sender, args);
            case "spectate" -> {
                count(args, 2, 2, "spectate <matchId>");
                matches.watch(ExamplePlugin.player(sender), matches.find(lower(args[1])));
            }
            case "view" -> {
                count(args, 2, 2, "view <free|a|b>");
                matches.view(ExamplePlugin.player(sender), lower(args[1]));
            }
            case "end", "leave" -> matches.leave(ExamplePlugin.player(sender));
            case "rematch" -> matches.rematch(ExamplePlugin.player(sender));
            case "stop" -> {
                count(args, 1, 2, "stop [matchId]");
                if (args.length == 2) {
                    ExamplePlugin.require(sender, "pvpbotrlexample.admin");
                    matches.stop(lower(args[1]));
                } else matches.stop(ExamplePlugin.player(sender));
            }
            case "recover" -> matches.recover(ExamplePlugin.player(sender));
            case "pause", "resume" -> {
                ExamplePlugin.require(sender, "pvpbotrlexample.admin");
                count(args, 1, 2, sub + " [matchId]");
                Duel match = args.length == 2 ? matches.find(lower(args[1])) : matches.current(ExamplePlugin.player(sender));
                if (match == null) throw new IllegalStateException("Specify a live match ID.");
                matches.pause(match, sub.equals("pause"));
            }
            case "stage" -> stage(sender, args);
            case "reload" -> {
                ExamplePlugin.require(sender, "pvpbotrlexample.admin");
                plugin.readSettings();
                sender.sendMessage("Settings reloaded. Existing matches keep their settings.");
            }
            default -> throw usage("help");
        }
    }

    private void help(CommandSender sender) {
        sender.sendMessage("/rlbotexample list | start [bot] [arena] | bots <botA> <botB> [arena]");
        sender.sendMessage("/rlbotexample challenge <player> [arena] | accept <player> | deny <player> | queue <join|leave>");
        sender.sendMessage("/rlbotexample matches | status [matchId] | spectate <matchId> | view <free|a|b>");
        sender.sendMessage("/rlbotexample leave | stop | rematch | recover | stage list | stage info <arena>");
        if (sender.hasPermission("pvpbotrlexample.admin")) {
            sender.sendMessage("/rlbotexample stop <matchId> | pause [matchId] | resume [matchId] | reload");
            sender.sendMessage("/rlbotexample stage create <id> [width] [depth] | stage register <id>");
            sender.sendMessage("/rlbotexample stage <pos1|pos2|spectator> <id> | stage spawn <id> <a|b>");
            sender.sendMessage("/rlbotexample stage <validate|enable|disable|remove> <id>");
        }
    }

    private static String describe(Duel match) {
        return match.id + " | " + match.arena + " | " + match.fighters[0].displayName()
                + " vs " + match.fighters[1].displayName();
    }

    private void status(CommandSender sender, String[] args) {
        count(args, 1, 2, "status [matchId]");
        if (args.length == 2) {
            MatchLog.Result result = matches.result(lower(args[1]));
            if (result != null) {
                sender.sendMessage(result.match_id() + " | " + result.arena() + " | " + result.outcome()
                        + (result.winningFighter() == null ? "" : " | Winner: " + result.winningFighter().name())
                        + " | " + result.reason());
                return;
            }
        }
        Duel match = args.length == 2 ? matches.find(lower(args[1]))
                : sender instanceof Player player ? matches.current(player) : null;
        if (match == null && matches.live().size() == 1) match = matches.live().getFirst();
        if (match == null) {
            sender.sendMessage(matches.live().size() + " live matches | " + matches.queueSize() + " waiting in matchmaking.");
            return;
        }
        sender.sendMessage(describe(match) + " | " + match.phase + " | " + match.elapsed / 20
                + " s elapsed | " + matches.spectators(match) + " spectators");
    }

    private void stage(CommandSender sender, String[] args) throws IOException {
        if (args.length == 1) throw usage("stage <list|info|create|register|pos1|pos2|spawn|spectator|validate|enable|disable|remove>");
        String action = lower(args[1]);
        if (action.equals("list")) {
            count(args, 2, 2, "stage list");
            if (stages.ids().isEmpty()) sender.sendMessage("No arenas are registered.");
            for (String id : stages.ids()) {
                sender.sendMessage(id + " | " + stages.entry(id).status() + (matches.occupied(id) ? " (in use)" : ""));
            }
            return;
        }
        if (!action.equals("info")) ExamplePlugin.require(sender, "pvpbotrlexample.admin");
        if (action.equals("create")) {
            count(args, 3, 5, "stage create <id> [width] [depth]");
            int width = args.length > 3 ? dimension(args[3]) : Stage.WIDTH;
            int depth = args.length > 4 ? dimension(args[4]) : width;
            stages.create(lower(args[2]), ExamplePlugin.player(sender).getLocation(),
                    plugin.settings().distance(), width, depth, sender);
            return;
        }
        if (action.equals("register")) {
            count(args, 3, 3, "stage register <id>");
            stages.register(lower(args[2]));
            sender.sendMessage("Custom arena registered. Set pos1, pos2, spawn a, spawn b, and spectator, then enable it.");
            return;
        }

        if (!STAGE_ADMIN.contains(action)) throw usage("stage list");
        int expected = action.equals("spawn") ? 4 : 3;
        count(args, expected, expected,
                "stage " + action + " <id>" + (action.equals("spawn") ? " <a|b>" : ""));
        String id = lower(args[2]);
        var entry = stages.entry(id);
        if (action.equals("info")) {
            Stage stage = entry.stage();
            sender.sendMessage(id + " | " + entry.kind() + " | " + entry.status() + (matches.occupied(id) ? " (in use)" : ""));
            if (stage != null) {
                sender.sendMessage(stage.worldName() + " | (" + stage.minX() + ", " + stage.floorY() + ", " + stage.minZ() + ") | "
                        + stage.width() + " × " + stage.depth() + " | Wall height " + stage.height());
                sender.sendMessage("Spawn A " + point(stage.a()) + " | Spawn B " + point(stage.b())
                        + " | Spectator " + point(stage.spectator()));
            }
            return;
        }
        if (StageStore.PRACTICE.equals(id)) throw new IllegalStateException("The built-in arena is managed by the plugin.");
        if (matches.occupied(id)) throw new IllegalStateException("Arena " + id + " is in use.");
        switch (action) {
            case "pos1", "pos2", "spectator", "spawn" -> {
                String field = action.equals("spawn") ? lower(args[3]) : action;
                if (action.equals("spawn") && !List.of("a", "b").contains(field)) throw usage("stage spawn <id> <a|b>");
                stages.setPoint(id, field, ExamplePlugin.player(sender).getLocation());
                sender.sendMessage("Set " + field + " for arena " + id + ".");
            }
            case "enable", "validate" -> stages.validate(id, action.equals("enable"), sender);
            case "disable" -> {
                stages.disable(id);
                sender.sendMessage("Arena " + id + " disabled.");
            }
            case "remove" -> stages.remove(id, sender);
            default -> throw usage("stage list");
        }
    }

    private static String point(Stage.StartPoint point) {
        return String.format(Locale.ROOT, "(%.1f, %.1f, %.1f)", point.x(), point.y(), point.z());
    }

    private static int dimension(String text) {
        try { return Integer.parseInt(text); }
        catch (NumberFormatException e) { throw new IllegalArgumentException("Arena dimensions must be integers."); }
    }

    private static Player target(String name) {
        Player player = Bukkit.getPlayerExact(name);
        if (player == null || player.hasMetadata("NPC")) throw new IllegalArgumentException("That player is offline.");
        return player;
    }

    private static String optionalArena(String[] args, int position) {
        return args.length > position ? lower(args[position]) : null;
    }

    private static void count(String[] args, int minimum, int maximum, String usage) {
        if (args.length < minimum || args.length > maximum) throw usage(usage);
    }

    private static IllegalArgumentException usage(String usage) {
        return new IllegalArgumentException("Usage: /rlbotexample " + usage);
    }

    static String lower(String value) { return value.toLowerCase(Locale.ROOT); }

    @Override public List<String> onTabComplete(CommandSender sender, Command command, String alias, String[] args) {
        if (args.length == 0) return List.of();
        String sub = lower(args[0]);
        boolean admin = sender.hasPermission("pvpbotrlexample.admin");
        List<String> options = List.of();
        if (args.length == 1) {
            options = new java.util.ArrayList<>(PUBLIC);
            if (admin) options.addAll(List.of("pause", "resume", "reload"));
        } else if (sub.equals("start") && args.length == 2 || sub.equals("bots") && args.length <= 3) {
            options = matches.models();
        } else if (sub.equals("start") && args.length == 3 || sub.equals("bots") && args.length == 4
                || sub.equals("challenge") && args.length == 3) {
            options = stages.ids().stream().filter(matches::available).toList();
        } else if (List.of("challenge", "accept", "deny").contains(sub) && args.length == 2) {
            options = ExamplePlugin.realPlayers().stream().filter(name -> !name.equalsIgnoreCase(sender.getName())).toList();
        } else if ((List.of("spectate", "status").contains(sub) || admin && List.of("stop", "pause", "resume").contains(sub))
                && args.length == 2) {
            options = matches.live().stream().map(match -> match.id).toList();
        } else if (sub.equals("view") && args.length == 2) {
            options = List.of("free", "a", "b");
        } else if (sub.equals("queue") && args.length == 2) {
            options = List.of("join", "leave");
        } else if (sub.equals("stage")) {
            if (args.length == 2) options = admin ? STAGE_ADMIN : List.of("list", "info");
            else if (args.length == 3 && (admin || lower(args[1]).equals("info"))
                    && !List.of("create", "register", "list").contains(lower(args[1]))) options = stages.ids();
            else if (admin && args.length == 4 && lower(args[1]).equals("spawn")) options = List.of("a", "b");
        }
        String prefix = lower(args[args.length - 1]);
        return options.stream().filter(option -> lower(option).startsWith(prefix)).toList();
    }
}
