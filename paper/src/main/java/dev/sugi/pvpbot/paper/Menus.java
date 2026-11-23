// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import dev.sugi.pvpbot.paper.policy.BotPolicy;
import io.papermc.paper.event.player.AsyncChatEvent;
import java.io.IOException;
import java.time.Instant;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.logging.Level;
import net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer;
import org.bukkit.Bukkit;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;

final class Menus implements Listener {
    static final List<String> PAGES = List.of("main", "fight", "bots", "challenge", "invitations",
            "queue", "select_arena", "live", "details", "spectator", "arenas");
    static final int PAGE_SIZE = 7;

    private record Row(String id, String title, String detail, MatchManager.Invite invite) {
        Row(String id, String title, String detail) { this(id, title, detail, null); }
    }
    private record Page(String name, int index, int total, String token, List<Row> rows) {}
    private record Choice(String mode, String first, String second) {}
    private record Input(String type, Instant expires) {}

    private static final class Session {
        Page page;
        Choice choice;
        String firstBot;
        String detailId;
        String adminArena;
    }

    private final PvpBotPlugin plugin;
    private final MatchManager matches;
    private final StageStore stages;
    private final Map<UUID, Session> sessions = new HashMap<>();
    private final Map<UUID, Input> inputs = new ConcurrentHashMap<>();
    private volatile Map<UUID, Map<String, String>> published = Map.of();
    private DeluxeMenuBridge bridge;
    private PvpCommands commands;

    Menus(PvpBotPlugin plugin, MatchManager matches, StageStore stages) {
        this.plugin = plugin;
        this.matches = matches;
        this.stages = stages;
    }

    void bridge(DeluxeMenuBridge bridge) { this.bridge = bridge; }
    void commands(PvpCommands commands) { this.commands = commands; }

    private boolean available() {
        var plugins = Bukkit.getPluginManager();
        return bridge != null && plugins.isPluginEnabled("DeluxeMenus") && plugins.isPluginEnabled("PlaceholderAPI");
    }

    boolean isOurMenu(Player player) { return available() && bridge.isOurMenu(player); }

    void close() {
        if (available()) {
            for (Player player : Bukkit.getOnlinePlayers()) {
                if (isOurMenu(player)) player.closeInventory();
            }
            bridge.close();
        }
        sessions.clear();
        inputs.clear();
        published = Map.of();
    }

    void forget(Player player) {
        sessions.remove(player.getUniqueId());
        inputs.remove(player.getUniqueId());
    }

    void open(Player player, String name, int index) throws IOException {
        if (!available()) throw new IllegalStateException("Menus require DeluxeMenus and PlaceholderAPI. Use /pvpbot help for commands.");
        if (!PAGES.contains(name)) throw new IllegalArgumentException("Unknown PvPBot menu.");
        if (name.equals("arenas")) PvpBotPlugin.require(player, "pvpbot.admin");
        UUID id = player.getUniqueId();
        var session = sessions.computeIfAbsent(id, ignored -> new Session());
        List<Row> all = rows(player, name);
        int total = Math.max(1, (all.size() + PAGE_SIZE - 1) / PAGE_SIZE);
        int page = Math.clamp(index, 0, total - 1);
        int start = page * PAGE_SIZE;
        session.page = new Page(name, page, total, UUID.randomUUID().toString(),
                List.copyOf(all.subList(start, Math.min(start + PAGE_SIZE, all.size()))));
        publish();
        bridge.open(player, name, session.page.token);
        inputs.remove(id);
    }

    private List<Row> rows(Player viewer, String page) {
        return switch (page) {
            case "fight", "bots" -> BotPolicy.STYLES.stream().map(style -> new Row(style, style, "Select this bot")).toList();
            case "challenge" -> PvpBotPlugin.realPlayers().stream().filter(name -> !name.equalsIgnoreCase(viewer.getName()))
                    .map(Bukkit::getPlayerExact).filter(java.util.Objects::nonNull)
                    .map(player -> new Row(player.getUniqueId().toString(), player.getName(), "Select a player")).toList();
            case "invitations" -> matches.incoming(viewer).stream().map(invite -> {
                Player from = Bukkit.getPlayer(invite.from());
                return from == null ? null : new Row(invite.from().toString(), from.getName(),
                        "Left click to accept. Right click to decline.", invite);
            }).filter(java.util.Objects::nonNull).toList();
            case "live" -> matches.live().stream().map(match -> new Row(match.id,
                    match.fighters[0].displayName() + " vs " + match.fighters[1].displayName(),
                    match.arena + " | " + match.mode())).toList();
            case "select_arena" -> stages.ids().stream().filter(matches::available).map(id -> new Row(id, id, "Available")).toList();
            case "arenas" -> stages.ids().stream().map(id -> new Row(id, id, stages.entry(id).status()
                    + (matches.occupied(id) ? " (in use)" : ""))).toList();
            default -> List.of();
        };
    }

    void command(Player player, String[] args) throws IOException {
        if (args.length < 3) throw new IllegalArgumentException("Open the PvPBot menu with /pvpbot.");
        var session = sessions.get(player.getUniqueId());
        if (session == null || session.page == null || !session.page.token.equals(args[1]) || !isOurMenu(player)) {
            throw new IllegalStateException("This menu has expired. Open /pvpbot again.");
        }
        Page page = session.page;
        switch (args[2]) {
            case "nav" -> {
                if (args.length != 4 || List.of("details", "select_arena").contains(args[3])) throw invalidAction();
                if (args[3].equals("bots") && !page.name.equals("bots")) session.firstBot = null;
                open(player, args[3], 0);
            }
            case "prev" -> open(player, page.name, page.index - 1);
            case "next" -> open(player, page.name, page.index + 1);
            case "refresh" -> open(player, page.name, page.index);
            case "row" -> {
                if (args.length != 5 || !args[3].matches("[0-6]") || !List.of("left", "right").contains(args[4])) throw invalidAction();
                int slot = Integer.parseInt(args[3]);
                if (slot >= page.rows.size()) throw new IllegalArgumentException("There is no entry in that slot.");
                clickRow(player, session, page.rows.get(slot), args[4].equals("right"));
            }
            case "auto" -> {
                requirePage(page, "select_arena");
                launch(player, session, null);
            }
            case "current" -> {
                Duel current = matches.current(player);
                if (current == null) throw new IllegalStateException("You have no current match.");
                details(player, session, current.id);
            }
            case "watch" -> {
                requirePage(page, "details");
                matches.watch(player, matches.find(session.detailId));
                open(player, "spectator", 0);
            }
            case "stop" -> {
                requirePage(page, "details");
                Duel target = matches.find(session.detailId);
                var seat = matches.seat(player);
                if (player.hasPermission("pvpbot.admin")) matches.stop(target.id);
                else if (matches.hosted(player) == target || seat != null && seat.match() == target) matches.stop(player);
                else throw new IllegalStateException("You cannot stop this match.");
            }
            case "leave" -> matches.leave(player);
            case "rematch" -> matches.rematch(player);
            case "help", "recover", "reload" -> commands.execute(player, args[2]);
            case "queue_join", "queue_leave" -> {
                requirePage(page, "queue");
                matches.queue(player, args[2].equals("queue_join"));
            }
            case "free", "a", "b" -> {
                requirePage(page, "spectator");
                matches.view(player, args[2]);
                player.closeInventory();
            }
            case "input" -> {
                if (args.length != 4) throw invalidAction();
                prompt(player, page, args[3]);
            }
            case "stage" -> {
                requirePage(page, "arenas");
                PvpBotPlugin.require(player, "pvpbot.admin");
                if (session.adminArena == null) throw new IllegalStateException("Select an arena first.");
                if (args.length != 4) throw invalidAction();
                switch (args[3]) {
                    case "a", "b" -> commands.execute(player, "stage", "spawn", session.adminArena, args[3]);
                    case "info", "pos1", "pos2", "spectator", "enable", "disable", "validate", "upgrade", "remove" ->
                        commands.execute(player, "stage", args[3], session.adminArena);
                    default -> throw invalidAction();
                }
                if (!stages.ids().contains(session.adminArena)) session.adminArena = null;
                open(player, "arenas", page.index);
            }
            default -> throw invalidAction();
        }
        publish();
    }

    private static void requirePage(Page page, String name) {
        if (!page.name.equals(name)) throw invalidAction();
    }

    private static IllegalArgumentException invalidAction() {
        return new IllegalArgumentException("Invalid menu action.");
    }

    private void clickRow(Player player, Session session, Row row, boolean rightClick) throws IOException {
        switch (session.page.name) {
            case "fight" -> selectArena(player, session, new Choice("fight", row.id, null));
            case "bots" -> {
                PvpBotPlugin.require(player, "pvpbot.host");
                if (session.firstBot == null) {
                    session.firstBot = row.id;
                    open(player, "bots", 0);
                } else {
                    selectArena(player, session, new Choice("bots", session.firstBot, row.id));
                    session.firstBot = null;
                }
            }
            case "challenge" -> selectArena(player, session, new Choice("challenge", row.id, null));
            case "invitations" -> {
                if (!matches.incoming(player).contains(row.invite)) throw new IllegalStateException("That invitation has expired.");
                matches.answer(player, UUID.fromString(row.id), !rightClick);
                if (rightClick) open(player, "invitations", 0);
            }
            case "live" -> {
                matches.find(row.id);
                details(player, session, row.id);
            }
            case "select_arena" -> launch(player, session, row.id);
            case "arenas" -> {
                PvpBotPlugin.require(player, "pvpbot.admin");
                stages.entry(row.id);
                session.adminArena = row.id;
                open(player, "arenas", session.page.index);
            }
            default -> throw invalidAction();
        }
    }

    private void details(Player player, Session session, String id) throws IOException {
        session.detailId = id;
        open(player, "details", 0);
    }

    private void selectArena(Player player, Session session, Choice choice) throws IOException {
        PvpBotPlugin.require(player, choice.mode.equals("bots") ? "pvpbot.host" : "pvpbot.play");
        session.choice = choice;
        open(player, "select_arena", 0);
    }

    private void launch(Player player, Session session, String arena) throws IOException {
        Choice choice = session.choice;
        if (choice == null) throw new IllegalStateException("Select the opponent first.");
        switch (choice.mode) {
            case "fight" -> matches.startBot(player, choice.first, arena);
            case "bots" -> matches.host(player, choice.first, choice.second, arena);
            case "challenge" -> {
                Player target = Bukkit.getPlayer(UUID.fromString(choice.first));
                if (target == null) throw new IllegalStateException("That player is offline.");
                matches.challenge(player, target, arena);
                player.closeInventory();
            }
            default -> throw invalidAction();
        }
        session.choice = null;
    }

    private void prompt(Player player, Page page, String type) {
        String instructions;
        if (type.equals("player")) {
            requirePage(page, "challenge");
            PvpBotPlugin.require(player, "pvpbot.play");
            instructions = "Type the player's exact name in chat.";
        } else {
            requirePage(page, "arenas");
            PvpBotPlugin.require(player, "pvpbot.admin");
            instructions = switch (type) {
                case "create" -> "Type an arena ID, optionally followed by width and depth.";
                case "register" -> "Type a custom arena ID.";
                default -> throw invalidAction();
            };
        }
        player.closeInventory();
        UUID id = player.getUniqueId();
        Input input = new Input(type, Instant.now().plusSeconds(60));
        inputs.put(id, input);
        player.sendMessage(instructions + " Type cancel to go back. You have 60 seconds.");
        Bukkit.getScheduler().runTaskLater(plugin, () -> {
            if (inputs.remove(id, input) && player.isOnline()) player.sendMessage("Menu input timed out.");
        }, 1200);
    }

    @EventHandler(priority = EventPriority.LOWEST, ignoreCancelled = true)
    public void chat(AsyncChatEvent event) {
        Player player = event.getPlayer();
        Input input = inputs.get(player.getUniqueId());
        if (input == null) return;
        // Consume chat only while the input prompt is active.
        event.setCancelled(true);
        if (!inputs.remove(player.getUniqueId(), input)) return;
        String message = PlainTextComponentSerializer.plainText().serialize(event.message()).trim();
        Bukkit.getScheduler().runTask(plugin, () -> completeInput(player, input, message));
    }

    private void completeInput(Player player, Input input, String message) {
        if (!player.isOnline()) return;
        if (!input.expires.isAfter(Instant.now())) {
            player.sendMessage("Menu input timed out.");
            return;
        }
        try {
            if (message.equalsIgnoreCase("cancel")) {
                open(player, input.type.equals("player") ? "challenge" : "arenas", 0);
            } else if (input.type.equals("player")) {
                Player target = Bukkit.getPlayerExact(message);
                if (target == null || target.hasMetadata("NPC") || target.equals(player)) {
                    throw new IllegalArgumentException("Enter another online player's exact name.");
                }
                var session = sessions.computeIfAbsent(player.getUniqueId(), ignored -> new Session());
                selectArena(player, session, new Choice("challenge", target.getUniqueId().toString(), null));
            } else {
                String[] words = message.split("\\s+");
                if (words.length > (input.type.equals("create") ? 3 : 1) || !StageStore.validId(words[0])) {
                    throw new IllegalArgumentException("Enter an arena ID using lowercase letters, digits, hyphens or underscores.");
                }
                var args = new ArrayList<>(List.of("stage", input.type));
                args.addAll(List.of(words));
                commands.execute(player, args.toArray(String[]::new));
                open(player, "arenas", 0);
            }
        } catch (IllegalArgumentException | IllegalStateException e) {
            player.sendMessage(e.getMessage());
        } catch (IOException | RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.WARNING, "PvPBot menu input failed", e);
            player.sendMessage("Could not complete the operation. Check the server log.");
        }
    }

    void publish() {
        if (!Bukkit.getPluginManager().isPluginEnabled("PlaceholderAPI")) return;
        var values = new LinkedHashMap<UUID, Map<String, String>>();
        var live = new HashMap<String, Duel>();
        var stats = new HashMap<String, MatchLog.Result>();
        for (Duel match : matches.live()) {
            live.put(match.id, match);
            stats.put(match.id, match.snapshot());
        }
        for (Player player : Bukkit.getOnlinePlayers()) {
            if (!player.hasMetadata("NPC")) values.put(player.getUniqueId(), snapshot(player, live, stats));
        }
        published = Map.copyOf(values);
    }

    private Map<String, String> snapshot(Player player, Map<String, Duel> live, Map<String, MatchLog.Result> stats) {
        var values = new HashMap<String, String>();
        var view = matches.view(player);
        var match = matches.current(player);
        boolean queued = matches.queued(player);
        values.put("state", matches.seat(player) != null ? "Fighting" : view != null ? "Spectating"
                : matches.hosted(player) != null ? "Hosting" : queued ? "Queued" : "Idle");
        values.put("camera", view == null ? "None" : view.camera().toUpperCase(java.util.Locale.ROOT));
        values.put("queued", Boolean.toString(queued));
        values.put("queue_size", Integer.toString(matches.queueSize()));
        values.put("live_count", Integer.toString(live.size()));
        values.put("invite_count", Integer.toString(matches.incoming(player).size()));
        matchValues(values, "match_", match, match == null ? null : stats.get(match.id));
        Session session = sessions.get(player.getUniqueId());
        String detailId = session == null ? null : session.detailId;
        Duel detail = detailId == null ? null : live.get(detailId);
        MatchLog.Result result = detailId == null ? null : detail != null ? stats.get(detailId) : matches.result(detailId);
        matchValues(values, "detail_", detail, result);
        values.put("first_bot", session == null || session.firstBot == null ? "Select bot A" : "Bot A: " + session.firstBot + " | Select bot B");
        values.put("selected_arena", session == null || session.adminArena == null ? "None" : session.adminArena);
        Page page = session == null ? null : session.page;
        values.put("page", Integer.toString(page == null ? 1 : page.index + 1));
        values.put("pages", Integer.toString(page == null ? 1 : page.total));
        for (int slot = 0; slot < PAGE_SIZE; slot++) {
            Row row = page != null && slot < page.rows.size() ? page.rows.get(slot) : null;
            String prefix = "row_" + slot + "_";
            values.put(prefix + "present", Boolean.toString(row != null));
            values.put(prefix + "id", row == null ? "" : row.id);
            values.put(prefix + "name", row == null ? "" : row.title);
            values.put(prefix + "detail", row == null ? "" : row.detail);
        }
        return Map.copyOf(values);
    }

    private void matchValues(Map<String, String> values, String prefix, Duel match, MatchLog.Result info) {
        values.put(prefix + "id", info == null ? "-" : info.match_id());
        values.put(prefix + "arena", info == null ? "-" : info.arena());
        values.put(prefix + "mode", info == null ? "-" : info.mode());
        values.put(prefix + "status", match == null ? info == null ? "Unavailable" : "Finished" : match.phase.name());
        values.put(prefix + "seconds", info == null ? "0.0" : Duel.number(info.elapsed_ticks() / 20.0));
        values.put(prefix + "spectators", Integer.toString(match == null ? 0 : matches.spectators(match)));
        String outcome = "-";
        if (match == null && info != null) {
            outcome = switch (info.outcome()) {
                case "decided" -> "Winner: " + info.winningFighter().name();
                case "draw" -> "Draw";
                default -> "Aborted";
            };
            outcome += " (" + info.reason().replace('_', ' ') + ")";
        }
        values.put(prefix + "result", outcome);
        fighterValues(values, prefix + "a_", info == null ? null : info.a());
        fighterValues(values, prefix + "b_", info == null ? null : info.b());
    }

    private static void fighterValues(Map<String, String> values, String prefix, MatchLog.Fighter fighter) {
        values.put(prefix + "name", fighter == null ? "-" : fighter.name());
        values.put(prefix + "hp", fighter == null ? "-" : Duel.number(fighter.hp()));
        values.put(prefix + "max_hp", fighter == null ? "-" : Duel.number(fighter.max_hp()));
    }

    String placeholder(UUID player, String key) {
        Map<String, String> values = published.get(player);
        return values == null ? "" : values.get(key);
    }
}
