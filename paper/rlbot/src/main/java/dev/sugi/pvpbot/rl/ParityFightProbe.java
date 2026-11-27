// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import dev.sugi.pvpbot.rl.policy.Action;
import dev.sugi.pvpbot.rl.policy.BotPolicy;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.UUID;
import java.util.function.BiConsumer;
import net.minecraft.network.protocol.game.ServerboundAttackPacket;
import net.minecraft.network.protocol.game.ServerboundSwingPacket;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.entity.LivingEntity;
import org.bukkit.Bukkit;
import org.bukkit.GameMode;
import org.bukkit.GameRules;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.attribute.Attribute;
import org.bukkit.craftbukkit.entity.CraftPlayer;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.entity.EntityDamageByEntityEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.FoodLevelChangeEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.event.player.PlayerVelocityEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.Damageable;

/** Explicitly enabled isolated-server combat capture; normal matches never enter this path. */
final class ParityFightProbe implements Listener {
    private static final int FIRST_SAMPLE = 150;
    private static final VarHandle ATTACK_TICKS = attackTicks();
    private final RlPlugin plugin;
    private final RlRuntime runtime;
    private final Path root;
    private String previous = "";
    private Path directory;
    private UUID trialId;
    private DirectBot botHandle;
    private Player human;
    private Player opponent;
    private boolean bot;
    private String scenario;
    private String policyModel;
    private double distance;
    private float pitch;
    private boolean oneDurability;
    private boolean opponentClick;
    private boolean clicked;
    private boolean reverse;
    private double fallHeight;
    private Long motionUplinkMs;
    private Long motionDownlinkNs;
    private int age;
    private final List<Object> events = new ArrayList<>();
    private final Map<String, ParityPlayerConnection> playerConnections = new LinkedHashMap<>();
    private final List<Object> moveEvents = new ArrayList<>();
    private final List<Object> packetHandlers = new ArrayList<>();
    private final List<Object> tickStarts = new ArrayList<>();
    private final List<Object> botObservations = new ArrayList<>();
    private final List<Object> policyFrames = new ArrayList<>();
    private final List<Object> knockbackRandom = new ArrayList<>();
    private OpponentReplica remoteReplica;
    private Object shadowTracker;
    private final List<Object> shadowReplicaEvents = new ArrayList<>();
    private final List<Object> productionReplicaEvents = new ArrayList<>();
    private ParityAttackUplink attackUplink;
    private JsonArray opponentActions;
    private int captureTicks = 80;
    private int scriptedStartDelay;
    private final boolean traceHandlers = Boolean.getBoolean("pvp.parity.packetHandlers");
    private final boolean traceReplica = Boolean.getBoolean("pvp.parity.remoteReplica");
    private final boolean useReplica = Boolean.getBoolean("pvp.parity.replicaObservation");
    private final Long attackUplinkMs = Long.getLong("pvp.parity.attackUplinkMs");

    ParityFightProbe(RlPlugin plugin, RlRuntime runtime, Path root) {
        this.plugin = plugin; this.runtime = runtime; this.root = root.toAbsolutePath().normalize();
        if (useReplica && (!traceReplica || !traceHandlers)) {
            throw new IllegalStateException("Replica observations require the native tracker probe");
        }
        if (attackUplinkMs != null && !useReplica) throw new IllegalStateException("Attack uplink requires replica observations");
        if (traceHandlers) {
            try {
                var agent = Class.forName("dev.sugi.pvpbot.parity.PaperPacketAgent", true, ClassLoader.getSystemClassLoader());
                agent.getMethod("setSink", BiConsumer.class).invoke(null, (BiConsumer<Object, Object>) this::afterHandler);
                agent.getMethod("setRandomSink", BiConsumer.class).invoke(null, (BiConsumer<Object, Double>) this::onKnockbackRandom);
                if (traceReplica) agent.getMethod("setTrackerSink", BiConsumer.class).invoke(null,
                        (BiConsumer<Object, Object>) this::onTrackedPacket);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException("Parity packet handler agent is missing", e);
            }
        }
    }

    boolean usesReplica(Player body) {
        return useReplica && isTrialBot(body);
    }

    OpponentReplica.Frame replicaFrame() {
        return remoteReplica == null ? null : remoteReplica.frame();
    }

    ParityAttackUplink attackUplink() { return attackUplink; }

    private void onTrackedPacket(Object tracker, Object packet) {
        if (remoteReplica != null && tracker == shadowTracker) remoteReplica.receive((net.minecraft.network.protocol.Packet<?>) packet);
    }

    java.util.function.Consumer<Object> productionTrace(Player body) {
        return isTrialBot(body) ? productionReplicaEvents::add : null;
    }

    private boolean isTrialBot(Player body) { return trialId != null && bot && body.equals(opponent); }

    private void onKnockbackRandom(Object entity, double value) {
        if (trialId == null || !Bukkit.isPrimaryThread() || age < FIRST_SAMPLE || age >= FIRST_SAMPLE + captureTicks ||
                !(entity instanceof net.minecraft.server.level.ServerPlayer affected)) return;
        Player player = affected.getBukkitEntity();
        if (!player.equals(human) && !player.equals(opponent)) return;
        knockbackRandom.add(Map.of("sequence", knockbackRandom.size(), "actor", player.equals(human) ? "human" : "opponent",
                "server_tick", Bukkit.getCurrentTick(), "monotonic_ns", System.nanoTime(), "value", value));
    }

    void tickStart() {
        if (trialId == null || age < FIRST_SAMPLE || age >= FIRST_SAMPLE + captureTicks) return;
        tickStarts.add(Map.of("server_tick", Bukkit.getCurrentTick(), "monotonic_ns", System.nanoTime()));
        if (attackUplink != null) attackUplink.applyDue();
        if (remoteReplica != null) remoteReplica.tick();
    }

    private void afterHandler(Object listener, Object packet) {
        if (trialId == null || !Bukkit.isPrimaryThread() || !(listener instanceof ServerGamePacketListenerImpl game)) return;
        Player player = game.player.getBukkitEntity();
        String actor = player.equals(human) ? "human" : !bot && player.equals(opponent) ? "opponent" : "";
        var connection = playerConnections.get(actor);
        if (connection == null) return;
        int receipt = connection.takeReceipt(packet);
        if (age < FIRST_SAMPLE || age >= FIRST_SAMPLE + captureTicks) return;
        var event = new LinkedHashMap<String, Object>();
        event.put("actor", actor);
        event.put("receipt_sequence", receipt);
        event.put("monotonic_ns", System.nanoTime());
        event.put("server_tick", Bukkit.getCurrentTick());
        if (packet instanceof ServerboundMovePlayerPacket || packet instanceof ServerboundAttackPacket ||
                packet instanceof ServerboundSwingPacket) {
            event.put("state", state(player));
            event.put("connection_state", ParityPlayerConnection.movementState(game));
            if (packet instanceof ServerboundAttackPacket) event.put("target_state", state(player.equals(human) ? opponent : human));
        }
        packetHandlers.add(event);
    }

    void tick() {
        try {
            Path request = root.resolve("fight-request.json");
            if (trialId == null && Files.exists(request)) {
                String text = Files.readString(request);
                if (!text.equals(previous)) {
                    previous = text;
                    var spec = JsonParser.parseString(text).getAsJsonObject();
                    String name = spec.get("id").getAsString();
                    if (!name.matches("[a-zA-Z0-9_-]+")) throw new IllegalArgumentException("Invalid id");
                    directory = root.resolve(name); Files.createDirectory(directory);
                    Files.writeString(directory.resolve("request.json"), text);
                    bot = spec.get("bot").getAsBoolean();
                    scenario = spec.has("scenario") ? spec.get("scenario").getAsString() : "charged";
                    captureTicks = spec.has("capture_ticks") ? spec.get("capture_ticks").getAsInt() : 80;
                    if (captureTicks < 60 || captureTicks > 6040) throw new IllegalArgumentException("Invalid capture length");
                    opponentActions = spec.getAsJsonArray("opponent_actions");
                    if (opponentActions != null && opponentActions.size() != captureTicks - 40) {
                        throw new IllegalArgumentException("Opponent action count differs from requested capture");
                    }
                    scriptedStartDelay = spec.has("script_start_delay_ticks") ? spec.get("script_start_delay_ticks").getAsInt() : 0;
                    if (scriptedStartDelay < 0 || scriptedStartDelay > 20 || (scriptedStartDelay > 0 && opponentActions == null)) {
                        throw new IllegalArgumentException("Invalid scripted start delay");
                    }
                    policyModel = spec.has("policy_model") ? spec.get("policy_model").getAsString() : null;
                    if (policyModel != null && !BotPolicy.isRecurrent(policyModel)) throw new IllegalArgumentException("Invalid policy model");
                    motionUplinkMs = Long.getLong("pvp.parity.motionUplinkMs");
                    motionDownlinkNs = null;
                    if (spec.has("match_motion_delays") && spec.get("match_motion_delays").getAsBoolean()) {
                        if (motionUplinkMs == null) throw new IllegalStateException("Expected the isolated motion-uplink adapter");
                        motionUplinkMs = spec.get("client_uplink_ms").getAsLong();
                        motionDownlinkNs = Math.multiplyExact(spec.get("client_downlink_ms").getAsLong(), 1_000_000L);
                    }
                    distance = spec.has("distance") ? spec.get("distance").getAsDouble() : 2.5;
                    pitch = spec.has("pitch") ? spec.get("pitch").getAsFloat() : 0;
                    oneDurability = spec.has("one_durability") && spec.get("one_durability").getAsBoolean();
                    opponentClick = spec.has("opponent_click") && spec.get("opponent_click").getAsBoolean();
                    clicked = false;
                    fallHeight = spec.has("fall_height") ? spec.get("fall_height").getAsDouble() : 0;
                    double minimumDistance = scenario.equals("overlap") ? 0 : 1;
                    if (!Double.isFinite(distance) || distance < minimumDistance || distance > 8 || !Float.isFinite(pitch) || Math.abs(pitch) > 90
                            || !Double.isFinite(fallHeight) || fallHeight < 0 || fallHeight > 10) {
                        throw new IllegalArgumentException("Invalid combat positioning");
                    }
                    String striker = spec.has("striker") ? spec.get("striker").getAsString() : "ParityA";
                    if (!List.of("ParityA", "ParityB").contains(striker)) throw new IllegalArgumentException("Invalid striker");
                    human = java.util.Objects.requireNonNull(Bukkit.getPlayerExact(striker));
                    reverse = striker.equals("ParityB");
                    events.clear(); moveEvents.clear(); packetHandlers.clear(); tickStarts.clear();
                    shadowReplicaEvents.clear(); productionReplicaEvents.clear();
                    botObservations.clear(); policyFrames.clear(); knockbackRandom.clear(); age = 0;
                    opponent = bot ? null : java.util.Objects.requireNonNull(Bukkit.getPlayerExact(
                            reverse ? "ParityA" : "ParityB"));
                    trialId = UUID.randomUUID();
                }
            }
            if (trialId == null) return;
            age++;
            if (age == 110) {
                var at = new Location(Bukkit.getWorlds().getFirst(), 0.5, 65, -9.5, reverse ? -180 : 0, pitch);
                Location target = at.clone().add(0, 0, reverse ? -distance : distance);
                target.setYaw(reverse ? 0 : -180); target.setPitch(0);
                at.getWorld().setGameRule(GameRules.NATURAL_HEALTH_REGENERATION, false);
                if (bot) {
                    botHandle = runtime.spawnProbe(target, policyModel);
                    opponent = botHandle.player();
                }
                playerConnections.put("human", new ParityPlayerConnection(human));
                if (!bot) playerConnections.put("opponent", new ParityPlayerConnection(opponent));
                prepareFighter(human, at, bot || !oneDurability);
                prepareFighter(opponent, target, bot || !oneDurability);
                if (bot) botHandle.start(human);
                if (oneDurability) {
                    // The historical break probe forces its test state after public kit validation.
                    if (bot) {
                        equipFighter(human, false);
                        equipFighter(opponent, false);
                    }
                    setOneDurability();
                }
            }
            if (age == FIRST_SAMPLE) {
                var humanHandle = ((CraftPlayer) human).getHandle();
                var opponentHandle = ((CraftPlayer) opponent).getHandle();
                ATTACK_TICKS.set(humanHandle, 20);
                ATTACK_TICKS.set(opponentHandle, 20);
                if (bot && traceReplica) {
                    var tracker = java.util.Objects.requireNonNull(humanHandle.level().getChunkSource().chunkMap.entityMap.get(humanHandle.getId()),
                            "No native tracker for the ordinary client");
                    shadowTracker = tracker;
                    remoteReplica = new OpponentReplica(humanHandle, Long.getLong("pvp.parity.replicaDownlinkMs", 0L) * 1_000_000L,
                            shadowReplicaEvents::add);
                    tracker.serverEntity.sendPairingData(opponentHandle, remoteReplica::receive);
                }
                if (bot && attackUplinkMs != null) {
                    attackUplink = new ParityAttackUplink(opponentHandle, Math.multiplyExact(attackUplinkMs, 1_000_000L));
                }
                if (bot && motionUplinkMs != null) {
                    if (!scenario.equals("charged") && opponentActions == null) {
                        throw new IllegalStateException("Motion uplink requires the charged or mixed scripted trial");
                    }
                    BotAvatar.armMotionUplink(opponent, Math.multiplyExact(motionUplinkMs, 1_000_000L), motionDownlinkNs,
                            opponentActions != null, (int) ATTACK_TICKS.get(opponentHandle));
                }
                events.add(Map.of("event", "initial", "human", state(human), "opponent", state(opponent),
                        "connection_states", Map.of("human", ParityPlayerConnection.movementState(humanHandle.connection),
                                "opponent", ParityPlayerConnection.movementState(opponentHandle.connection))));
                Path ready = directory.resolve("ready.tmp");
                int readyTick = Bukkit.getCurrentTick();
                long readyEpoch = System.currentTimeMillis();
                Files.writeString(ready, StateFiles.JSON.toJson(Map.of("ready", true, "server_tick", readyTick,
                        "epoch_ms", readyEpoch, "script_start_server_tick", readyTick + scriptedStartDelay + 1,
                        "script_start_epoch_ms", readyEpoch + 50L * scriptedStartDelay)));
                Files.move(ready, directory.resolve("ready.json"), java.nio.file.StandardCopyOption.ATOMIC_MOVE);
            }
            if (age >= FIRST_SAMPLE && age <= FIRST_SAMPLE + captureTicks) {
                if (opponentActions != null && (!human.isOnline() || human.isDead()
                        || (bot ? !opponent.isValid() : !opponent.isOnline()) || opponent.isDead())) {
                    throw new IllegalStateException("The scripted fight ended before its capture window closed");
                }
                var sample = new LinkedHashMap<String, Object>();
                sample.put("event", "state");
                sample.put("tick", age - FIRST_SAMPLE);
                sample.put("server_tick", Bukkit.getCurrentTick());
                sample.put("monotonic_ns", System.nanoTime());
                sample.put("epoch_ms", System.currentTimeMillis());
                sample.put("human", state(human));
                sample.put("opponent", state(opponent));
                if (traceHandlers && fallHeight > 0) {
                    sample.put("connection_states", Map.of("human", movementState(human), "opponent", movementState(opponent)));
                }
                events.add(sample);
            }
            if (age == FIRST_SAMPLE + 1 && fallHeight > 0) {
                opponent.teleport(opponent.getLocation().add(0, fallHeight, 0));
                events.add(Map.of("event", "fall_teleport", "server_tick", Bukkit.getCurrentTick(),
                        "monotonic_ns", System.nanoTime(), "actor", "opponent", "height", fallHeight));
            }
            if (age == FIRST_SAMPLE + captureTicks) {
                Path temporary = directory.resolve("server.tmp");
                Files.writeString(temporary, StateFiles.JSON.toJson(Map.ofEntries(
                        Map.entry("schema", 1), Map.entry("scope", scenario.equals("charged") ? "paper-single-charged-hit" : "paper-scripted-combat"),
                        Map.entry("capture_ticks", captureTicks),
                        Map.entry("bot", bot), Map.entry("unbreakable", !oneDurability),
                        Map.entry("match", trialId), Map.entry("events", events), Map.entry("status", "complete"),
                        Map.entry("adapter_version", BotAvatar.BEHAVIOR_VERSION), Map.entry("motion_transport", BotAvatar.probeMotionEvents(opponent)),
                        Map.entry("player_connections", finishConnections()), Map.entry("player_move_events", moveEvents),
                        Map.entry("packet_handlers", packetHandlers), Map.entry("tick_starts", tickStarts),
                        Map.entry("bot_observations", botObservations), Map.entry("policy_frames", policyFrames),
                        Map.entry("knockback_random", knockbackRandom),
                        Map.entry("remote_replica", shadowReplicaEvents), Map.entry("production_replica", productionReplicaEvents),
                        Map.entry("attack_uplink", attackUplink == null ? List.of() : attackUplink.events()),
                        Map.entry("motion_uplink", BotAvatar.probeUplinkEvents(opponent)))));
                close();
                Files.move(temporary, directory.resolve("server.json"), java.nio.file.StandardCopyOption.ATOMIC_MOVE);
            }
        } catch (Exception e) {
            try { close(); }
            catch (RuntimeException cleanup) { e.addSuppressed(cleanup); }
            plugin.getLogger().log(java.util.logging.Level.SEVERE, "Parity fight failed", e);
            try { if (directory != null) Files.writeString(directory.resolve("failed.json"), e.toString()); }
            catch (Exception suppressed) { e.addSuppressed(suppressed); }
        }
    }

    void close() {
        trialId = null;
        remoteReplica = null; shadowTracker = null; attackUplink = null;
        try { finishConnections(); }
        finally {
            if (botHandle != null) {
                var ending = botHandle;
                botHandle = null;
                ending.close();
            }
        }
    }

    private static void prepareFighter(Player player, Location at, boolean unbreakable) {
        player.setGameMode(GameMode.SURVIVAL);
        if (!player.teleport(at)) throw new IllegalStateException("Could not position a parity fighter");
        player.setCollidable(false);
        player.setFoodLevel(20);
        player.setSaturation(0);
        player.setExhaustion(0);
        player.setHealth(attribute(player, Attribute.MAX_HEALTH));
        equipFighter(player, unbreakable);
    }

    private static void equipFighter(Player player, boolean unbreakable) {
        var inventory = player.getInventory();
        inventory.clear();
        inventory.setItem(0, item(Material.DIAMOND_SWORD, false, unbreakable));
        inventory.setHeldItemSlot(0);
        inventory.setHelmet(item(Material.DIAMOND_HELMET, true, unbreakable));
        inventory.setChestplate(item(Material.DIAMOND_CHESTPLATE, true, unbreakable));
        inventory.setLeggings(item(Material.DIAMOND_LEGGINGS, true, unbreakable));
        inventory.setBoots(item(Material.DIAMOND_BOOTS, true, unbreakable));
    }

    private static ItemStack item(Material material, boolean armor, boolean unbreakable) {
        var item = new ItemStack(material);
        if (armor) item.addEnchantment(Enchantment.PROTECTION, 4);
        if (unbreakable) item.editMeta(meta -> meta.setUnbreakable(true));
        return item;
    }

    private static Object movementState(Player player) {
        return ParityPlayerConnection.movementState(((CraftPlayer) player).getHandle().connection);
    }

    private void setOneDurability() {
        var strikerInventory = human.getInventory();
        strikerInventory.setItemInMainHand(oneDurability(strikerInventory.getItemInMainHand()));
        var victimInventory = opponent.getInventory();
        ItemStack[] armor = victimInventory.getArmorContents();
        for (int i = 0; i < armor.length; i++) armor[i] = oneDurability(armor[i]);
        victimInventory.setArmorContents(armor);
    }

    private static ItemStack oneDurability(ItemStack stack) {
        if (stack == null || !(stack.getItemMeta() instanceof Damageable meta) || meta.isUnbreakable()) {
            throw new IllegalStateException("Expected breakable kit equipment");
        }
        meta.setDamage(stack.getType().getMaxDurability() - 1);
        stack.setItemMeta(meta);
        return stack;
    }

    private Map<String, Object> finishConnections() {
        var captures = new LinkedHashMap<String, Object>();
        playerConnections.forEach((actor, connection) -> captures.put(actor, connection.finish()));
        playerConnections.clear();
        return captures;
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = false)
    public void move(PlayerMoveEvent e) {
        if (trialId == null || age < 110 || age >= FIRST_SAMPLE + captureTicks || e.getClass() != PlayerMoveEvent.class) return;
        String actor;
        if (e.getPlayer().equals(human)) actor = "human";
        else if (!bot && e.getPlayer().equals(opponent)) actor = "opponent";
        else return;
        moveEvents.add(Map.of("actor", actor, "phase", "bukkit-player-move-monitor", "cancelled", e.isCancelled(),
                "from", position(e.getFrom()), "to", position(e.getTo()), "server_tick", Bukkit.getCurrentTick(),
                "monotonic_ns", System.nanoTime(), "epoch_ms", System.currentTimeMillis()));
    }

    Action botAction(Player body) {
        if (!isTrialBot(body) || policyModel != null) return null;
        if (opponentActions != null) {
            int step = age - FIRST_SAMPLE - scriptedStartDelay;
            if (step < 0 || step >= opponentActions.size()) return Action.IDLE;
            JsonObject action = opponentActions.get(step).getAsJsonObject();
            return new Action(BotAvatar.readKeys(action), Action.IDLE.look(), action.get("click").getAsBoolean(), false);
        }
        if (opponentClick && !clicked && age >= 160) {
            clicked = true;
            events.add(Map.of("event", "bot_click", "server_tick", Bukkit.getCurrentTick(), "monotonic_ns", System.nanoTime()));
            return Action.of(false, false, false, false, false, true);
        }
        return Action.IDLE;
    }

    boolean tracePolicy(Player body) {
        return policyModel != null && isTrialBot(body);
    }

    void policyFrame(BotAvatar.Sample sample, BotPolicy policy, BotPolicy.Inference inference) {
        policyFrames.add(Map.of("step", policyFrames.size(), "server_tick", Bukkit.getCurrentTick(),
                "monotonic_ns", System.nanoTime(), "model", policy.style(), "model_sha256", policy.hash(),
                "observation", sample.observation(), "recurrent", sample.recurrent(), "inference", inference));
    }

    void observation(Player body, BotAvatar.Sample sample, Action action, boolean attempted) {
        if (!isTrialBot(body) || age < FIRST_SAMPLE || age >= FIRST_SAMPLE + captureTicks) return;
        var event = new LinkedHashMap<String, Object>();
        event.put("phase", action == null ? "bot_sample" : "bot_action_applied");
        event.put("server_tick", Bukkit.getCurrentTick());
        event.put("monotonic_ns", System.nanoTime());
        event.put("epoch_ms", System.currentTimeMillis());
        event.put("observation", sample.observation());
        event.put("opponent_source", sample.diagnosticReplica() ? "native_tracker_replica" : "tracker_packet_replica");
        event.put("self_motion_source", sample.predictedSelf() ? "client_prediction" : "server_body");
        event.put("self_position", sample.selfPosition());
        event.put("target_rotation", List.of(sample.targetYaw(), sample.targetPitch()));
        if (sample.recurrent() != null) event.put("recurrent_input", sample.recurrent());
        if (action != null) {
            event.put("action", action);
            event.put("attack_attempted", attempted);
        }
        botObservations.add(event);
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = false)
    public void fall(EntityDamageEvent e) {
        if (trialId == null || e.getCause() != EntityDamageEvent.DamageCause.FALL || !e.getEntity().equals(opponent)) return;
        events.add(Map.of("event", "fall_damage", "tick", age - 150, "server_tick", Bukkit.getCurrentTick(),
                "cancelled", e.isCancelled(), "raw", e.getDamage(), "final_damage", e.getFinalDamage(),
                "monotonic_ns", System.nanoTime(), "epoch_ms", System.currentTimeMillis()));
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void food(FoodLevelChangeEvent e) {
        if (trialId == null || age < 110 || !(e.getEntity() instanceof Player player)) return;
        if ((player.equals(human) || player.equals(opponent)) && e.getFoodLevel() < player.getFoodLevel()) {
            e.setCancelled(true);
        }
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = false)
    public void damage(EntityDamageByEntityEvent e) {
        if (trialId == null || !(e.getEntity() instanceof Player victim) || !(e.getDamager() instanceof Player attacker)
                || !List.of(human, opponent == null ? human : opponent).contains(victim)) return;
        events.add(Map.of("event", "damage", "tick", age - 150, "server_tick", Bukkit.getCurrentTick(),
                "attacker", attacker.equals(human) ? "human" : "opponent", "cancelled", e.isCancelled(),
                "raw", e.getDamage(), "final_damage", e.getFinalDamage(), "victim", state(victim),
                "monotonic_ns", System.nanoTime(), "epoch_ms", System.currentTimeMillis()));
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = false)
    public void velocity(PlayerVelocityEvent e) {
        if (trialId == null || !(e.getPlayer().equals(human) || e.getPlayer().equals(opponent))) return;
        var v = e.getVelocity();
        var buffer = io.netty.buffer.Unpooled.buffer();
        try {
            net.minecraft.network.LpVec3.write(buffer, new net.minecraft.world.phys.Vec3(v.getX(), v.getY(), v.getZ()));
            String encoded = io.netty.buffer.ByteBufUtil.hexDump(buffer);
            var decoded = net.minecraft.network.LpVec3.read(buffer);
            events.add(Map.of("event", "velocity_event", "tick", age - 150, "server_tick", Bukkit.getCurrentTick(),
                    "epoch_ms", System.currentTimeMillis(), "monotonic_ns", System.nanoTime(),
                    "actor", e.getPlayer().equals(human) ? "human" : "opponent", "cancelled", e.isCancelled(),
                    "velocity", List.of(v.getX(), v.getY(), v.getZ()), "lpvec3_hex", encoded,
                    "decoded_velocity", List.of(decoded.x, decoded.y, decoded.z)));
        } finally { buffer.release(); }
    }

    private static Object state(Player p) {
        var nms = ((CraftPlayer) p).getHandle();
        var at = p.getLocation(); var v = p.getVelocity();
        var inventory = p.getInventory();
        var state = new LinkedHashMap<String, Object>();
        state.put("health", p.getHealth());
        state.put("charge", nms.getAttackStrengthScale(0.5f));
        state.put("attack_ticks", (int) ATTACK_TICKS.get(nms));
        state.put("last_hurt", nms.lastHurt);
        var known = nms.getKnownMovement();
        state.put("known_movement", List.of(known.x, known.y, known.z));
        state.put("position", position(at));
        state.put("velocity", List.of(v.getX(), v.getY(), v.getZ()));
        state.put("on_ground", nms.onGround());
        state.put("sprinting", p.isSprinting());
        state.put("pose", nms.getPose().name().toLowerCase(java.util.Locale.ROOT));
        state.put("fall_distance", p.getFallDistance());
        state.put("hurt_ticks", nms.hurtTime);
        state.put("invulnerable_ticks", nms.invulnerableTime);
        state.put("rotation", List.of(at.getYaw(), at.getPitch()));
        state.put("equipment", Map.of("main_hand", item(inventory.getItemInMainHand()),
                "off_hand", item(inventory.getItemInOffHand()), "head", item(inventory.getHelmet()),
                "chest", item(inventory.getChestplate()), "legs", item(inventory.getLeggings()),
                "feet", item(inventory.getBoots())));
        state.put("attributes", Map.of("armor", attribute(p, Attribute.ARMOR),
                "armor_toughness", attribute(p, Attribute.ARMOR_TOUGHNESS),
                "attack_damage", attribute(p, Attribute.ATTACK_DAMAGE),
                "attack_speed", attribute(p, Attribute.ATTACK_SPEED),
                "knockback_resistance", attribute(p, Attribute.KNOCKBACK_RESISTANCE),
                "movement_speed", attribute(p, Attribute.MOVEMENT_SPEED),
                "entity_interaction_range", attribute(p, Attribute.ENTITY_INTERACTION_RANGE)));
        state.put("food", Map.of("level", p.getFoodLevel(), "saturation", p.getSaturation(), "exhaustion", p.getExhaustion()));
        state.put("collidable", p.isCollidable());
        return state;
    }

    private static List<Double> position(Location at) { return List.of(at.getX(), at.getY(), at.getZ()); }

    private static VarHandle attackTicks() {
        try {
            return MethodHandles.privateLookupIn(LivingEntity.class, MethodHandles.lookup())
                    .findVarHandle(LivingEntity.class, "attackStrengthTicker", int.class);
        } catch (ReflectiveOperationException e) { throw new ExceptionInInitializerError(e); }
    }

    private static double attribute(Player p, Attribute attribute) {
        return java.util.Objects.requireNonNull(p.getAttribute(attribute)).getValue();
    }

    private static Object item(ItemStack item) {
        if (item == null || item.isEmpty()) return Map.of("material", "minecraft:air", "count", 0, "damage", 0,
                "enchantments", Map.of());
        var enchantments = new TreeMap<String, Integer>();
        item.getEnchantments().forEach((enchantment, level) -> enchantments.put(enchantment.key().asString(), level));
        int damage = item.getItemMeta() instanceof Damageable durability ? durability.getDamage() : 0;
        return Map.of("material", item.getType().key().asString(), "count", item.getAmount(),
                "damage", damage, "enchantments", enchantments);
    }
}
