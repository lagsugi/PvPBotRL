// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import dev.sugi.pvpbot.api.BotClosedEvent.Reason;
import dev.sugi.pvpbot.api.BotHandle;
import dev.sugi.pvpbot.api.PvPBotApi;
import dev.sugi.pvpbot.rl.policy.BotPolicy;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;
import java.util.logging.Level;
import java.util.stream.Stream;
import net.citizensnpcs.api.CitizensAPI;
import net.citizensnpcs.api.npc.NPCRegistry;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.plugin.Plugin;

final class RlRuntime implements PvPBotApi {
    static final String PROBE = "motion-probe";
    private static final String REGISTRY_NAME = "pvpbot-rl";
    private static final String TRAINING_PREFIX = "training:";
    private final RlPlugin plugin;
    private final NPCRegistry npcs = CitizensAPI.createInMemoryNPCRegistry(REGISTRY_NAME);
    private final Map<RlOpponent, DirectBot> bodies = new LinkedHashMap<>();
    private final Set<Plugin> disablingOwners = new HashSet<>();
    private int nextNpcId;
    private int pendingSpawns;
    private boolean closing;

    RlRuntime(RlPlugin plugin) { this.plugin = plugin; }

    static void requireMainThread() {
        if (!Bukkit.isPrimaryThread()) throw new IllegalStateException("PvPBotRL requires the server thread.");
    }

    @Override public List<String> models() {
        requireMainThread();
        requireAvailable();
        return plugin.trainingModels().isEmpty() ? BotPolicy.STYLES
                : Stream.concat(BotPolicy.STYLES.stream(), Stream.of(TrainingModels.RANDOM)).toList();
    }

    @Override public BotHandle spawn(Plugin owner, Location spawn, String modelId) {
        return spawn(owner, spawn, modelId, false);
    }

    @Override public void startPair(BotHandle first, BotHandle second) {
        requireMainThread();
        requireAvailable();
        if (!(first instanceof DirectBot a) || !(second instanceof DirectBot b)
                || !bodies.containsValue(a) || !bodies.containsValue(b)) {
            throw new IllegalArgumentException("Expected open handles from this PvPBotRL runtime.");
        }
        a.startPair(b);
    }

    DirectBot spawnProbe(Location spawn, String modelId) {
        return spawn(plugin, spawn, modelId == null ? PROBE : modelId, true);
    }

    private DirectBot spawn(Plugin owner, Location spawn, String modelId, boolean probe) {
        requireMainThread();
        requireAvailable();
        if (owner == null) throw new IllegalArgumentException("Supply a bot owner.");
        if (!owner.isEnabled() || disablingOwners.contains(owner)) {
            throw new IllegalStateException("The bot owner must be enabled.");
        }
        if (spawn == null || modelId == null) throw new IllegalArgumentException("Supply a bot spawn and model ID.");
        spawn = spawn.clone();
        spawn.checkFinite();
        int floorY = (int) Math.round(spawn.getY());
        if (!RlOpponent.standsOnFloor(spawn, floorY)) {
            throw new IllegalArgumentException("The bot spawn must stand on a solid, flat combat floor.");
        }

        TrainingModels.Entry training = null;
        if (TrainingModels.RANDOM.equals(modelId)) {
            training = plugin.trainingModels().draw(ThreadLocalRandom.current());
            if (training == null) throw new IllegalArgumentException("No training models are installed.");
        } else if (modelId.startsWith(TRAINING_PREFIX)) {
            training = trainingModel(modelId.substring(TRAINING_PREFIX.length()));
        } else if (!BotPolicy.STYLES.contains(modelId) && !(probe && PROBE.equals(modelId))) {
            throw new IllegalArgumentException("Unknown bot model: " + modelId);
        }

        String resolved = training == null ? modelId : TRAINING_PREFIX + training.provenance().sha256();
        RlSettings settings = plugin.settings();
        if (bodies.size() + pendingSpawns >= settings.matchLimit()) {
            throw new IllegalStateException("RL bot capacity unavailable.");
        }
        pendingSpawns++;
        try {
            BotPolicy policy = loadPolicy(modelId, training);
            UUID id = UUID.randomUUID();
            MotionTrace trace = settings.diagnostics() ? trace(id) : null;
            String bodyName = training == null ? modelId : TrainingModels.RANDOM;
            var avatar = newAvatar(id, bodyName, policy != null && policy.isRecurrent(), spawn, trace);
            var body = new RlOpponent(plugin, this, avatar, policy, trace, settings, floorY);
            String hash = policy == null ? null : policy.hash();
            var bot = new DirectBot(owner, body, new BotHandle.Info(resolved,
                    hash != null && !hash.startsWith("script-") ? hash : null,
                    policy == null ? null : policy.version(), settings.rttMs()));
            bodies.put(body, bot);
            if (!owner.isEnabled() || !available()) {
                closeDirect(bot, !available() ? Reason.RUNTIME_DISABLED : Reason.OWNER_DISABLED);
                throw new IllegalStateException("The bot owner or runtime disabled during spawn.");
            }
            return bot;
        } finally {
            pendingSpawns--;
        }
    }

    private TrainingModels.Entry trainingModel(String hash) {
        var entry = plugin.trainingModels().get(hash);
        if (entry == null) throw new IllegalArgumentException("Selected training model is unavailable: " + hash);
        return entry;
    }

    private static BotPolicy loadPolicy(String model, TrainingModels.Entry training) {
        if (training != null) return training.weights().newPolicy(TrainingModels.RANDOM);
        if (PROBE.equals(model)) return null;
        try { return new BotPolicy(model); }
        catch (IOException e) { throw new UncheckedIOException("Could not load model " + model, e); }
    }

    BotAvatar newAvatar(UUID id, String model, boolean recurrent, Location location, MotionTrace trace) {
        if (Bukkit.getEntity(id) != null || npcs.getByUniqueId(id) != null) {
            throw new IllegalStateException("Bot UUID is already in use: " + id);
        }
        return new BotAvatar(npcs, id, nextNpcId++, model, recurrent, location, trace);
    }

    private MotionTrace trace(UUID id) {
        return new MotionTrace(plugin.getDataFolder().toPath().resolve("diagnostics"), id);
    }

    private boolean available() { return plugin.isEnabled() && !closing; }

    private void requireAvailable() {
        if (!available()) throw new IllegalStateException("PvPBotRL is disabled.");
    }

    void forget(RlOpponent body) { bodies.remove(body); }
    int activeBodies() { return bodies.size(); }

    void tickStart() {
        var processed = new HashSet<DirectBot>();
        for (DirectBot bot : List.copyOf(bodies.values())) {
            if (!processed.add(bot)) continue;
            DirectBot partner = bot.partner();
            if (partner == null || !bodies.containsValue(partner) || !processed.add(partner)) {
                bot.tickStart();
            } else if (bot.firstThisTick()) {
                bot.tickStart();
                partner.tickStart();
            } else {
                partner.tickStart();
                bot.tickStart();
            }
        }
    }

    void ownerDisabled(Plugin owner) {
        disablingOwners.add(owner);
        try {
            for (DirectBot bot : List.copyOf(bodies.values())) {
                if (bot.owner() == owner) closeDirect(bot, Reason.OWNER_DISABLED);
            }
        } finally {
            disablingOwners.remove(owner);
        }
    }

    void close() {
        closing = true;
        for (DirectBot bot : List.copyOf(bodies.values())) closeDirect(bot, Reason.RUNTIME_DISABLED);
        CitizensAPI.removeNamedNPCRegistry(REGISTRY_NAME);
    }

    private void closeDirect(DirectBot bot, Reason reason) {
        try { bot.close(reason); }
        catch (RuntimeException | LinkageError e) {
            plugin.getLogger().log(Level.SEVERE, "Could not close RL bot " + bot.id(), e);
        }
    }
}
