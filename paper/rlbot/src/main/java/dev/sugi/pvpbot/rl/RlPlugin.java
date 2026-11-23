// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import com.destroystokyo.paper.event.server.ServerTickEndEvent;
import com.destroystokyo.paper.event.server.ServerTickStartEvent;
import dev.sugi.pvpbot.api.PvPBotApi;
import dev.sugi.pvpbot.rl.policy.Action;
import dev.sugi.pvpbot.rl.policy.BotPolicy;
import java.nio.file.Path;
import java.util.List;
import java.util.Locale;
import java.util.logging.Level;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.server.PluginDisableEvent;
import org.bukkit.plugin.ServicePriority;
import org.bukkit.plugin.java.JavaPlugin;

public final class RlPlugin extends JavaPlugin implements Listener {
    private RlSettings settings;
    private RlRuntime runtime;
    private TrainingModels trainingModels;
    private ParityMotionProbe motionProbe;
    private ParityFightProbe fightProbe;

    @Override public void onEnable() {
        try {
            var citizens = getServer().getPluginManager().getPlugin("Citizens");
            if (citizens == null || !citizens.isEnabled()
                    || !"2.0.44-SNAPSHOT (build 4270)".equals(citizens.getPluginMeta().getVersion())) {
                throw new IllegalStateException("Citizens 2.0.44 build 4270 is required.");
            }
            saveDefaultConfig();
            settings = RlSettings.read(getConfig());
            trainingModels = TrainingModels.load(getDataFolder().toPath().resolve("training-models"), getLogger());
            runtime = new RlRuntime(this);
            String parityRoot = System.getProperty("pvp.parity.root");
            if (parityRoot != null) {
                if (!"127.0.0.1".equals(getServer().getIp()) || getServer().getOnlineMode()) {
                    throw new IllegalStateException("Parity probes require an offline-mode loopback test server.");
                }
                Path root = Path.of(parityRoot);
                motionProbe = new ParityMotionProbe(root, this, runtime);
                fightProbe = new ParityFightProbe(this, runtime, root);
                getServer().getPluginManager().registerEvents(fightProbe, this);
            }
            getServer().getPluginManager().registerEvents(this, this);
            getServer().getServicesManager().register(PvPBotApi.class, runtime, this, ServicePriority.Normal);
        } catch (RuntimeException | LinkageError e) {
            getLogger().log(Level.SEVERE, "PvPBotRL could not initialize", e);
            getServer().getPluginManager().disablePlugin(this);
        }
    }

    @EventHandler public void onTickStart(ServerTickStartEvent event) {
        if (fightProbe != null) fightProbe.tickStart();
        runtime.tickStart();
        if (motionProbe != null) motionProbe.tickStart();
    }

    @EventHandler public void onTickEnd(ServerTickEndEvent event) {
        if (motionProbe != null) motionProbe.tickEnd();
        if (fightProbe != null) fightProbe.tick();
    }

    @EventHandler(priority = EventPriority.MONITOR)
    public void onPluginDisable(PluginDisableEvent event) {
        if (event.getPlugin() != this) runtime.ownerDisabled(event.getPlugin());
    }

    @Override public void onDisable() {
        try {
            if (runtime != null) getServer().getServicesManager().unregister(PvPBotApi.class, runtime);
            if (motionProbe != null) {
                try { motionProbe.close(); }
                catch (Exception e) { getLogger().log(Level.WARNING, "Could not close the motion probe", e); }
            }
            if (fightProbe != null) fightProbe.close();
        } finally {
            if (runtime != null) runtime.close();
        }
    }

    RlSettings settings() { return settings; }
    TrainingModels trainingModels() { return trainingModels; }

    Action parityAction(Player body) { return fightProbe == null ? null : fightProbe.botAction(body); }
    boolean parityUsesReplica(Player body) { return fightProbe != null && fightProbe.usesReplica(body); }
    OpponentReplica.Frame parityReplicaFrame() { return fightProbe.replicaFrame(); }
    ParityAttackUplink parityAttackUplink() { return fightProbe.attackUplink(); }
    java.util.function.Consumer<Object> parityOpponentTrace(Player body) {
        return fightProbe == null ? null : fightProbe.productionTrace(body);
    }

    void parityObservation(Player body, BotAvatar.Sample sample, Action action, boolean attempted) {
        if (fightProbe != null) fightProbe.observation(body, sample, action, attempted);
    }

    Action parityPolicyAction(Player body, BotPolicy policy, BotAvatar.Sample sample) {
        if (fightProbe != null && fightProbe.tracePolicy(body)) {
            return policy.act(sample.observation(), sample.recurrent(), traced -> fightProbe.policyFrame(sample, policy, traced));
        }
        return policy.act(sample.observation(), sample.recurrent());
    }

    @Override public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        if (!sender.hasPermission("pvpbotrl.play") && !sender.hasPermission("pvpbotrl.admin")) {
            sender.sendMessage("You do not have permission to use PvPBotRL.");
            return true;
        }
        if (args.length > 1) return false;
        String action = args.length == 0 ? "status" : args[0].toLowerCase(Locale.ROOT);
        switch (action) {
            case "list" -> sender.sendMessage("Models: " + String.join(", ", runtime.models()));
            case "status" -> sender.sendMessage("PvPBotRL API ready, active bots " + runtime.activeBodies()
                    + ", RTT " + settings.rttMs() + " ms, diagnostics " + settings.diagnostics());
            case "reload" -> {
                if (!sender.hasPermission("pvpbotrl.admin")) {
                    sender.sendMessage("You do not have permission to administer PvPBotRL.");
                    return true;
                }
                try {
                    reloadConfig();
                    settings = RlSettings.read(getConfig());
                    sender.sendMessage("PvPBotRL settings reloaded.");
                } catch (IllegalArgumentException e) {
                    sender.sendMessage("Settings rejected: " + e.getMessage());
                }
            }
            default -> { return false; }
        }
        return true;
    }

    @Override public List<String> onTabComplete(CommandSender sender, Command command, String alias, String[] args) {
        boolean admin = sender.hasPermission("pvpbotrl.admin");
        if (args.length != 1 || (!sender.hasPermission("pvpbotrl.play") && !admin)) return List.of();
        List<String> choices = admin ? List.of("list", "status", "reload") : List.of("list", "status");
        String prefix = args[0].toLowerCase(Locale.ROOT);
        return choices.stream().filter(choice -> choice.startsWith(prefix)).toList();
    }

}
