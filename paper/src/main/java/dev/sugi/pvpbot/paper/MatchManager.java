// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import dev.sugi.pvpbot.common.PlayerRecovery;
import dev.sugi.pvpbot.paper.policy.BotPolicy;
import java.io.IOException;
import java.nio.file.Path;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.logging.Level;
import net.citizensnpcs.api.npc.NPCRegistry;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.title.Title;
import org.bukkit.Bukkit;
import org.bukkit.GameMode;
import org.bukkit.Location;
import org.bukkit.Sound;
import org.bukkit.SoundCategory;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.util.Vector;

final class MatchManager {
    private static final Title.Times RESULT_TITLE_TIMES =
            Title.Times.times(Duration.ofMillis(150), Duration.ofSeconds(2), Duration.ofMillis(350));
    record Seat(Duel match, int side) {}
    record View(Duel match, String camera) {}
    record Invite(UUID from, UUID to, String arena, Instant expires) {}
    private record Rematch(String a, String b, UUID player) {}
    private record ChunkKey(World world, int x, int z) {}

    private final PvpBotPlugin plugin;
    private final StageStore stages;
    private final PlayerRecovery recovery;
    private final NPCRegistry registry;
    private final Path logs;
    private final Map<String, Duel> matches = new LinkedHashMap<>();
    private final Map<String, Duel> arenas = new HashMap<>();
    private final Map<UUID, Seat> seats = new HashMap<>();
    private final Map<UUID, Duel> hosts = new HashMap<>();
    private final Map<UUID, View> viewers = new HashMap<>();
    private final Map<UUID, Invite> invites = new LinkedHashMap<>();
    private final LinkedHashSet<UUID> queue = new LinkedHashSet<>();
    private final Map<UUID, Rematch> rematches = new LinkedHashMap<>(16, 0.75f, true);
    private final Map<String, MatchLog.Result> recent = new LinkedHashMap<>();
    private final Map<UUID, MatchLog.Result> respawnResults = new HashMap<>();
    private final Map<ChunkKey, Integer> tickets = new HashMap<>();
    private final Map<Duel, List<ChunkKey>> heldChunks = new HashMap<>();
    private final Set<UUID> changing = new HashSet<>();

    MatchManager(PvpBotPlugin plugin, StageStore stages, PlayerRecovery recovery, NPCRegistry registry, Path logs) {
        this.plugin = plugin;
        this.stages = stages;
        this.recovery = recovery;
        this.registry = registry;
        this.logs = logs;
    }

    Seat seat(Entity entity) { return seats.get(entity.getUniqueId()); }
    Duel hosted(Player player) { return hosts.get(player.getUniqueId()); }
    View view(Player player) { return viewers.get(player.getUniqueId()); }
    boolean changing(Player player) { return changing.contains(player.getUniqueId()); }
    boolean occupied(String arena) { return arenas.containsKey(arena); }
    boolean uses(World world) { return matches.values().stream().anyMatch(match -> match.world.equals(world)); }
    boolean available(String arena) { return stages.entry(arena).ready() && !occupied(arena); }
    boolean managed(Player player) { return seat(player) != null || view(player) != null; }
    boolean queued(Player player) { return queue.contains(player.getUniqueId()); }
    List<Duel> live() { return List.copyOf(matches.values()); }
    int queueSize() { return queue.size(); }
    MatchLog.Result result(String id) { return recent.get(id); }

    List<Invite> incoming(Player player) {
        return invites.values().stream().filter(invite -> invite.to.equals(player.getUniqueId())
                && invite.expires.isAfter(Instant.now())).toList();
    }

    Duel current(Player player) {
        Seat seat = seat(player);
        if (seat != null) return seat.match;
        View view = view(player);
        return view != null ? view.match : hosted(player);
    }

    Duel find(String id) {
        Duel match = matches.get(id);
        if (match == null) throw new IllegalArgumentException("This match is no longer live.");
        return match;
    }

    int spectators(Duel match) {
        return (int) viewers.values().stream().filter(view -> view.match == match).count();
    }

    private void ensureIdle(Player player) {
        if (player == null || !player.isOnline() || player.isDead() || player.isInsideVehicle() || player.isSleeping()) {
            throw new IllegalStateException("A player is unavailable.");
        }
        if (seat(player) != null || hosted(player) != null || view(player) != null || recovery.pending(player)) {
            throw new IllegalStateException(player.getName() + " is busy or has pending recovery.");
        }
    }

    private boolean pendingInvite(UUID id) {
        return invites.containsKey(id) || invites.values().stream().anyMatch(invite -> invite.to.equals(id));
    }

    private void requireNoRequests(Player player) {
        if (queued(player) || pendingInvite(player.getUniqueId())) {
            throw new IllegalStateException(player.getName() + " has a pending invitation or is in the queue.");
        }
    }

    void startBot(Player player, String style, String arena) throws IOException {
        PvpBotPlugin.require(player, "pvpbot.play");
        expireInvites();
        ensureIdle(player);
        requireNoRequests(player);
        requireStyle(style);
        start(player, player, null, null, style, select(arena));
        remember(player, new Rematch(null, style, null));
    }

    void host(Player host, String a, String b, String arena) throws IOException {
        PvpBotPlugin.require(host, "pvpbot.host");
        PvpBotPlugin.require(host, "pvpbot.spectate");
        expireInvites();
        ensureIdle(host);
        requireNoRequests(host);
        requireStyle(a);
        requireStyle(b);
        start(host, null, a, null, b, select(arena));
        remember(host, new Rematch(a, b, null));
    }

    private static void requireStyle(String style) {
        if (!BotPolicy.STYLES.contains(style)) throw new IllegalArgumentException("Unknown bot. See /pvpbot list.");
    }

    void challenge(Player from, Player to, String arena) {
        PvpBotPlugin.require(from, "pvpbot.play");
        PvpBotPlugin.require(to, "pvpbot.play");
        if (from.equals(to)) throw new IllegalArgumentException("Choose another player.");
        expireInvites();
        ensureIdle(from);
        ensureIdle(to);
        requireNoRequests(from);
        requireNoRequests(to);
        if (arena != null) stages.entry(arena);
        invites.put(from.getUniqueId(), new Invite(from.getUniqueId(), to.getUniqueId(), arena, Instant.now().plusSeconds(60)));
        from.sendMessage("Invitation sent to " + to.getName() + ".");
        to.sendMessage(from.getName() + " challenged you. /pvpbot accept " + from.getName()
                + " or /pvpbot deny " + from.getName());
    }

    void answer(Player to, UUID fromId, boolean accept) throws IOException {
        PvpBotPlugin.require(to, "pvpbot.play");
        expireInvites();
        Invite invite = invites.get(fromId);
        Player from = Bukkit.getPlayer(fromId);
        if (invite == null || !invite.to.equals(to.getUniqueId()) || from == null) {
            throw new IllegalArgumentException("That invitation is no longer available.");
        }
        if (!accept) {
            invites.remove(fromId);
            to.sendMessage("Invitation declined.");
            from.sendMessage(to.getName() + " declined your invitation.");
            return;
        }
        PvpBotPlugin.require(from, "pvpbot.play");
        ensureIdle(from);
        ensureIdle(to);
        if (queued(from) || queued(to)) throw new IllegalStateException("A player is queued.");
        startHumans(from, to, select(invite.arena));
    }

    void rematch(Player player) throws IOException {
        Rematch last = rematches.get(player.getUniqueId());
        if (last == null) throw new IllegalStateException("You have no recent opponent.");
        if (last.player != null) {
            Player other = Bukkit.getPlayer(last.player);
            if (other == null) throw new IllegalStateException("Your opponent is offline.");
            challenge(player, other, null);
        } else if (last.a != null) {
            host(player, last.a, last.b, null);
        } else {
            startBot(player, last.b, null);
        }
    }

    void queue(Player player, boolean join) {
        UUID id = player.getUniqueId();
        if (!join) {
            player.sendMessage(queue.remove(id) ? "You left matchmaking." : "You are not in the queue.");
            return;
        }
        PvpBotPlugin.require(player, "pvpbot.play");
        ensureIdle(player);
        expireInvites();
        requireNoRequests(player);
        queue.add(id);
        player.sendMessage("You joined matchmaking. Queue position: " + queue.size() + ".");
    }

    private void expireInvites() {
        Instant now = Instant.now();
        invites.values().removeIf(invite -> !invite.expires.isAfter(now));
    }

    private void clearRequests(UUID id) {
        queue.remove(id);
        invites.values().removeIf(invite -> invite.from.equals(id) || invite.to.equals(id));
    }

    private StageStore.Entry select(String requested) {
        if (matches.size() >= plugin.settings().maxConcurrent()) throw new IllegalStateException("The match limit has been reached.");
        if (requested != null) {
            StageStore.Entry entry = stages.entry(requested);
            if (!available(requested)) throw new IllegalStateException("Arena " + requested + " is unavailable.");
            validateStart(entry);
            return entry;
        }
        for (String id : stages.ids()) {
            if (!available(id)) continue;
            StageStore.Entry entry = stages.entry(id);
            try {
                validateStart(entry);
                return entry;
            } catch (IllegalArgumentException | IllegalStateException invalid) {
                // Check the next enabled arena if this one has become obstructed.
            }
        }
        throw new IllegalStateException("No playable arenas are available. See /pvpbot stage list.");
    }

    private static void validateStart(StageStore.Entry entry) {
        Stage stage = entry.stage();
        World world = stage.world();
        stage.validateBounds(world);
        stage.validateSpawns(world);
    }

    private void startHumans(Player a, Player b, StageStore.Entry entry) throws IOException {
        start(a, a, null, b, null, entry);
        remember(a, new Rematch(null, null, b.getUniqueId()));
        remember(b, new Rematch(null, null, a.getUniqueId()));
    }

    private void remember(Player player, Rematch previous) {
        rematches.put(player.getUniqueId(), previous);
        if (rematches.size() > 512) rematches.remove(rematches.keySet().iterator().next());
    }

    private void start(Player host, Player humanA, String botA, Player humanB, String botB,
                       StageStore.Entry entry) throws IOException {
        var settings = plugin.settings();
        var a = new Duel.Fighter(humanA, botA, settings.botPingMs());
        var b = new Duel.Fighter(humanB, botB, settings.botPingMs());
        var match = new Duel(entry, host.getUniqueId(), a, b, settings);
        matches.put(match.id, match);
        arenas.put(match.arena, match);
        hosts.put(match.host, match);
        clearRequests(match.host);
        for (int side = 0; side < 2; side++) {
            Player human = match.fighters[side].human;
            if (human == null) continue;
            seats.put(human.getUniqueId(), new Seat(match, side));
            clearRequests(human.getUniqueId());
        }
        try {
            match.log = new MatchLog(logs, match.snapshot());
            if (settings.detailed()) match.log.openTrace();
            keepChunks(match);
            for (int side = 0; side < 2; side++) prepare(match, side);
            if (a.avatar != null || b.avatar != null) {
                match.healthDisplay = new BotHealthDisplay(match);
                for (Duel.Fighter fighter : match.fighters) {
                    if (fighter.human != null) match.healthDisplay.show(fighter.human);
                }
            }
            if (humanA == null) watch(host, match);
            match.phase = Duel.Phase.COUNTDOWN;
            broadcast(match, match.mode() + " at " + match.arena + " | Match ID: " + match.id);
        } catch (IOException | RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.WARNING, "Match setup failed for " + match.id, e);
            finish(match, "aborted", null, "setup_failed", null);
            throw e;
        }
    }

    private void prepare(Duel match, int side) throws IOException {
        Duel.Fighter fighter = match.fighters[side];
        Location spawn = match.stage.spawn(match.world, side);
        if (fighter.human == null) {
            fighter.avatar = new BotAvatar(registry, fighter.name(), spawn);
            fighter.entity = fighter.avatar.player();
            seats.put(fighter.entity.getUniqueId(), new Seat(match, side));
            changing(fighter.entity, () -> Duel.kit(fighter.entity, match.settings.durability()));
        } else {
            fighter.human.closeInventory();
            recovery.save(fighter.human);
            changing(fighter.human, () -> {
                teleport(fighter.human, spawn);
                Duel.kit(fighter.human, match.settings.durability());
            });
        }
        fighter.safe = spawn;
    }

    private void keepChunks(Duel match) {
        Stage stage = match.stage;
        List<ChunkKey> held = new ArrayList<>();
        heldChunks.put(match, held);
        for (int x = stage.minX() >> 4; x <= (stage.minX() + stage.width() - 1) >> 4; x++) {
            for (int z = stage.minZ() >> 4; z <= (stage.minZ() + stage.depth() - 1) >> 4; z++) {
                var key = new ChunkKey(match.world, x, z);
                if (!tickets.containsKey(key)) match.world.addPluginChunkTicket(x, z, plugin);
                tickets.merge(key, 1, Integer::sum);
                held.add(key);
            }
        }
    }

    private void releaseChunks(Duel match) {
        List<ChunkKey> held = heldChunks.remove(match);
        if (held == null) return;
        for (ChunkKey key : held) {
            if (tickets.merge(key, -1, Integer::sum) == 0) {
                tickets.remove(key);
                clean(match, () -> key.world.removePluginChunkTicket(key.x, key.z, plugin));
            }
        }
    }

    void tick(int tick) {
        for (Duel match : live()) {
            try {
                tickMatch(match, tick);
            } catch (IOException | RuntimeException | LinkageError e) {
                plugin.getLogger().log(Level.SEVERE, "Match tick failed for " + match.id, e);
                finish(match, "aborted", null, "error", null);
            }
        }
        if (tick % 20 == 0) {
            expireInvites();
            try {
                matchQueue();
            } catch (RuntimeException | LinkageError e) {
                plugin.getLogger().log(Level.WARNING, "Could not process the matchmaking queue", e);
            }
        }
    }

    private void matchQueue() {
        queue.removeIf(id -> {
            try {
                Player player = Bukkit.getPlayer(id);
                ensureIdle(player);
                return !player.hasPermission("pvpbot.play");
            } catch (IllegalStateException e) {
                return true;
            }
        });
        if (queue.size() < 2) return;
        var waiting = queue.iterator();
        Player a = Bukkit.getPlayer(waiting.next());
        Player b = Bukkit.getPlayer(waiting.next());
        StageStore.Entry entry;
        try {
            entry = select(null);
        } catch (IllegalStateException unavailable) {
            return;
        }
        try {
            startHumans(a, b, entry);
        } catch (IOException | RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.SEVERE, "Queued match failed at " + entry.id(), e);
            a.sendMessage("Could not start the queued match.");
            b.sendMessage("Could not start the queued match.");
        }
    }

    private void tickMatch(Duel match, int tick) throws IOException {
        if (match.phase != Duel.Phase.COUNTDOWN && !match.running()) return;
        if (finishDeaths(match)) return;
        for (int side = 0; side < 2; side++) {
            Duel.Fighter fighter = match.fighters[side];
            if (fighter.human != null && !fighter.human.isOnline()) {
                forfeit(match, side, "disconnect", fighter.human.getUniqueId());
                return;
            }
            if (fighter.avatar != null && !fighter.avatar.spawned()) {
                finish(match, "aborted", null, "npc_removed", null);
                return;
            }
            Location at = fighter.entity.getLocation();
            if (!match.stage.contains(at, false)) {
                changing(fighter.entity, () -> teleport(fighter.entity, fighter.safe));
                fighter.entity.setVelocity(new Vector());
                fighter.entity.setFallDistance(0);
            } else {
                fighter.safe = at;
            }
        }
        if (match.countdown > 0) {
            if (match.countdown % 20 == 0) broadcast(match, "Starting in " + match.countdown / 20 + "...");
            match.countdown--;
            for (Duel.Fighter fighter : match.fighters) {
                if (fighter.avatar != null) fighter.avatar.idle();
            }
        } else if (match.elapsed >= match.settings.maxTicks()) {
            finish(match, "draw", null, "timeout", null);
            return;
        } else {
            if (!match.running()) {
                match.phase = Duel.Phase.RUNNING;
                broadcast(match, "Fight!");
            }
            match.botInputs();
        }
        if (match.healthDisplay != null) match.healthDisplay.update();
        if (tick % 5 == 0) {
            for (Player viewer : audience(match)) {
                View current = view(viewer);
                if (current == null) continue;
                try {
                    if (!viewer.hasPermission("pvpbot.spectate")) stopWatching(viewer, true);
                    else {
                        if (!current.camera.equals("free") && viewer.getSpectatorTarget() == null) detached(viewer);
                        viewer.sendActionBar(Component.text(match.health()));
                    }
                } catch (RuntimeException | LinkageError e) {
                    plugin.getLogger().log(Level.WARNING, "Could not update spectator " + viewer.getUniqueId(), e);
                    stopWatching(viewer, true);
                }
            }
        }
    }

    private boolean finishDeaths(Duel match) {
        boolean a = match.fighters[0].dead();
        boolean b = match.fighters[1].dead();
        if (!a && !b) return false;
        if (!match.running()) finish(match, "aborted", null, "death_before_start", null);
        else if (a && b) finish(match, "draw", null, "double_death", null);
        else finish(match, "decided", Duel.SIDES[a ? 1 : 0], "death", null);
        return true;
    }

    void watch(Player player, Duel match) throws IOException {
        PvpBotPlugin.require(player, "pvpbot.spectate");
        if (match.phase == Duel.Phase.FINISHING || match.phase == Duel.Phase.CLOSED) {
            throw new IllegalStateException("This match is no longer live.");
        }
        expireInvites();
        requireNoRequests(player);
        if (seat(player) != null || player.isDead() || player.isSleeping() || player.isInsideVehicle()) {
            throw new IllegalStateException("You cannot spectate in your current state.");
        }
        View previous = view(player);
        if (previous == null) {
            if (recovery.pending(player)) throw new IllegalStateException("Your recovery is pending.");
            player.closeInventory();
            recovery.save(player);
        }
        try {
            if (previous != null && previous.match.healthDisplay != null) previous.match.healthDisplay.hide(player);
            viewers.put(player.getUniqueId(), new View(match, "free"));
            changing(player, () -> {
                player.setGameMode(GameMode.SPECTATOR);
                if (player.getGameMode() != GameMode.SPECTATOR) throw new IllegalStateException("Could not set spectator mode.");
                player.setSpectatorTarget(null);
                teleport(player, match.stage.spectator().at(match.world));
            });
            if (match.healthDisplay != null) match.healthDisplay.show(player);
            player.sendMessage("Watching " + match.fighters[0].displayName() + " vs " + match.fighters[1].displayName()
                    + ". /pvpbot view <free|a|b> or /pvpbot leave");
        } catch (RuntimeException | LinkageError e) {
            stopWatching(player, true);
            throw e;
        }
    }

    void view(Player player, String camera) {
        PvpBotPlugin.require(player, "pvpbot.spectate");
        View old = view(player);
        if (old == null) throw new IllegalStateException("You are not spectating a PvPBot match.");
        Duel match = old.match;
        int side = switch (camera) {
            case "free" -> -1;
            case "a" -> 0;
            case "b" -> 1;
            default -> throw new IllegalArgumentException("Use free, a, or b.");
        };
        Player target = side == -1 ? null : match.fighters[side].entity;
        if (side != -1 && (target == null || !target.isValid() || match.fighters[side].dead())) {
            throw new IllegalStateException("That fighter is no longer available.");
        }
        changing(player, () -> {
            player.setSpectatorTarget(target);
            if (target == null) teleport(player, match.stage.spectator().at(match.world));
            else if (player.getSpectatorTarget() != target) throw new IllegalStateException("Could not attach to that fighter.");
        });
        viewers.put(player.getUniqueId(), new View(match, camera));
    }

    void detached(Player player) {
        View view = view(player);
        if (view != null && !changing(player)) viewers.put(player.getUniqueId(), new View(view.match, "free"));
    }

    private void stopWatching(Player player, boolean restoreNow) {
        View old = viewers.remove(player.getUniqueId());
        if (old == null) return;
        if (old.match.healthDisplay != null) clean(old.match, () -> old.match.healthDisplay.hide(player));
        if (restoreNow) {
            clean(old.match, () -> {
                if (player.getGameMode() == GameMode.SPECTATOR) player.setSpectatorTarget(null);
                player.sendActionBar(Component.empty());
            });
            restore(player);
        }
    }

    void leave(Player player) {
        Seat seat = seat(player);
        if (seat != null) {
            forfeit(seat.match, seat.side, "left", null);
        } else if (view(player) != null) {
            stopWatching(player, true);
        } else {
            throw new IllegalStateException("You are not fighting or spectating.");
        }
    }

    void stop(Player player) {
        Duel hostMatch = hosted(player);
        Seat seat = seat(player);
        if (hostMatch != null && (seat == null || player.hasPermission("pvpbot.admin"))) {
            finish(hostMatch, "aborted", null, "manual_stop", null);
        } else if (seat != null) {
            forfeit(seat.match, seat.side, "left", null);
        } else {
            throw new IllegalStateException("You have no match to stop.");
        }
    }

    void stop(String id) { finish(find(id), "aborted", null, "admin_stop", null); }

    private void forfeit(Duel match, int side, String reason, UUID disconnected) {
        if (match.running()) finish(match, "decided", Duel.SIDES[1 - side], reason, disconnected);
        else finish(match, "aborted", null, reason, disconnected);
    }

    void quit(Player player) {
        UUID id = player.getUniqueId();
        respawnResults.remove(id);
        clearRequests(id);
        Seat seat = seat(player);
        if (seat != null && seat.match.fighters[seat.side].human != null) {
            forfeit(seat.match, seat.side, "disconnect", id);
        }
        stopWatching(player, false);
    }

    void shutdown() {
        for (Duel match : live()) finish(match, "aborted", null, "server_stop", null);
    }

    void showRespawnResult(Player player) {
        MatchLog.Result result = respawnResults.remove(player.getUniqueId());
        if (result != null && player.isOnline() && !managed(player)) announceResult(player, result);
    }

    private void finish(Duel match, String outcome, String winner, String reason, UUID disconnected) {
        if (match.phase == Duel.Phase.FINISHING || match.phase == Duel.Phase.CLOSED) return;
        match.phase = Duel.Phase.FINISHING;
        List<Player> audience = audience(match);
        MatchLog.Result finalResult = null;
        try {
            var snapshot = match.snapshot();
            var result = snapshot.ended(outcome, winner, reason, match.elapsed, snapshot.a(), snapshot.b());
            finalResult = result;
            recent.put(match.id, finalResult);
            if (recent.size() > 64) recent.remove(recent.keySet().iterator().next());
            if (match.log != null) clean(match, () -> match.log.result(result));
            String message = switch (outcome) {
                case "draw" -> "Match drawn";
                case "decided" -> "Winner: " + result.winningFighter().name();
                default -> "Match aborted";
            };
            for (Player player : audience) {
                if (!player.getUniqueId().equals(disconnected)) {
                    player.sendMessage(message + " (" + reason.replace('_', ' ') + "). " + match.health());
                }
            }
        } catch (RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.SEVERE, "Could not capture result for " + match.id, e);
        } finally {
            for (Player player : audience) {
                View view = view(player);
                if (view != null && view.match == match) stopWatching(player, !player.getUniqueId().equals(disconnected));
            }
            viewers.values().removeIf(view -> view.match == match);
            if (match.healthDisplay != null) clean(match, match.healthDisplay::close);
            for (Duel.Fighter fighter : match.fighters) {
                if (fighter.entity != null) seats.remove(fighter.entity.getUniqueId());
                if (fighter.avatar != null) clean(match, fighter.avatar::close);
                if (fighter.human != null && !fighter.human.getUniqueId().equals(disconnected)) restore(fighter.human);
            }
            if (match.log != null) clean(match, match.log::close);
            releaseChunks(match);
            hosts.remove(match.host, match);
            matches.remove(match.id, match);
            arenas.remove(match.arena, match);
            match.phase = Duel.Phase.CLOSED;
        }
        if (finalResult == null || reason.equals("server_stop")) return;
        MatchLog.Result result = finalResult;
        for (Player player : audience) {
            if (!player.isOnline() || player.getUniqueId().equals(disconnected)) continue;
            if (player.isDead()) respawnResults.put(player.getUniqueId(), result);
            else clean(match, () -> announceResult(player, result));
        }
    }

    private static void announceResult(Player player, MatchLog.Result result) {
        var winner = result.winningFighter();
        String text;
        String subtitle;
        NamedTextColor color;
        Sound sound = Sound.BLOCK_NOTE_BLOCK_CHIME;
        float pitch = 1.0f;
        if (winner != null) {
            boolean participated = player.getUniqueId().toString().equals(result.a().player_uuid())
                    || player.getUniqueId().toString().equals(result.b().player_uuid());
            boolean won = player.getUniqueId().toString().equals(winner.player_uuid());
            text = winner.name() + " wins";
            subtitle = (participated ? (won ? "Victory · " : "Defeat · ") : "")
                    + Duel.number(winner.hp()) + " HP remaining";
            color = participated && !won ? NamedTextColor.RED : NamedTextColor.GOLD;
            if (participated && !won) {
                sound = Sound.BLOCK_NOTE_BLOCK_BASS;
                pitch = 0.8f;
            } else {
                pitch = 1.3f;
            }
        } else {
            boolean draw = result.outcome().equals("draw");
            text = draw ? "DRAW" : "MATCH ABORTED";
            subtitle = result.reason().replace('_', ' ');
            color = draw ? NamedTextColor.YELLOW : NamedTextColor.GRAY;
            if (!draw) sound = Sound.BLOCK_NOTE_BLOCK_BASS;
        }
        player.showTitle(Title.title(Component.text(text, color),
                Component.text(subtitle, NamedTextColor.WHITE), RESULT_TITLE_TIMES));
        player.playSound(player.getLocation(), sound, SoundCategory.PLAYERS, 0.65f, pitch);
    }

    private List<Player> audience(Duel match) {
        var audience = new LinkedHashSet<Player>();
        for (Duel.Fighter fighter : match.fighters) {
            if (fighter.human != null) audience.add(fighter.human);
        }
        for (var entry : viewers.entrySet()) {
            if (entry.getValue().match != match) continue;
            Player player = Bukkit.getPlayer(entry.getKey());
            if (player != null) audience.add(player);
        }
        return List.copyOf(audience);
    }

    private void broadcast(Duel match, String message) {
        for (Player player : audience(match)) player.sendMessage(message);
    }

    void restore(Player player) {
        if (!player.isOnline()) return;
        try {
            if (recovery.restore(player)) player.sendMessage("Your original state was restored.");
        } catch (IOException | RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.SEVERE, "Could not restore player " + player.getUniqueId(), e);
            player.sendMessage("Could not restore your original state. The recovery file was kept.");
        }
    }

    void recover(Player player) {
        if (managed(player)) throw new IllegalStateException("Leave the match before recovering your state.");
        if (!recovery.pending(player)) throw new IllegalStateException("No recovery is pending.");
        if (player.isDead()) throw new IllegalStateException("Respawn to recover your original state.");
        restore(player);
    }

    private void changing(Player player, Runnable operation) {
        UUID id = player.getUniqueId();
        boolean first = changing.add(id);
        try { operation.run(); }
        finally { if (first) changing.remove(id); }
    }

    private static void teleport(Player player, Location at) {
        if (!player.teleport(at)) throw new IllegalStateException("Could not teleport to the arena.");
    }

    private void clean(Duel match, Cleanup operation) {
        try { operation.run(); }
        catch (IOException | RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.SEVERE, "Match cleanup failed for " + match.id, e);
        }
    }

    @FunctionalInterface
    private interface Cleanup { void run() throws IOException; }
}
