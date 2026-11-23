// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import com.mojang.authlib.GameProfile;
import dev.sugi.pvpbot.rl.policy.Action;
import dev.sugi.pvpbot.rl.policy.BotPolicy;
import dev.sugi.pvpbot.rl.policy.Observation;
import dev.sugi.pvpbot.rl.policy.RecurrentInput;
import java.util.UUID;
import net.citizensnpcs.api.npc.NPC;
import net.citizensnpcs.api.npc.NPCRegistry;
import net.citizensnpcs.nms.v26_1_R1.entity.EntityHumanNPC;
import net.citizensnpcs.nms.v26_1_R1.entity.HumanController;
import net.citizensnpcs.npc.CitizensNPC;
import net.citizensnpcs.trait.SkinTrait;
import net.citizensnpcs.util.NMS;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.network.protocol.game.ServerboundAttackPacket;
import net.minecraft.network.protocol.game.ServerboundSwingPacket;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.phys.Vec3;
import org.bukkit.Bukkit;
import org.bukkit.FluidCollisionMode;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.craftbukkit.CraftWorld;
import org.bukkit.craftbukkit.entity.CraftPlayer;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.Player;
import org.bukkit.util.BoundingBox;
import org.bukkit.util.Vector;

final class BotAvatar implements AutoCloseable {
    private static final boolean PARITY_TRANSPORT = System.getProperty("pvp.parity.root") != null
            && Boolean.getBoolean("pvp.parity.motionTransport");
    static final String BEHAVIOR_VERSION = !PARITY_TRANSPORT ? "citizens-tracker-opponent-v5"
            : System.getProperty("pvp.parity.motionUplinkMs") == null ? "experimental-motion-transport-v1"
            : "experimental-motion-uplink-v1";
    private static final double REACH = 3.0;
    private final NPC npc;
    private final Player body;
    private final MotionTrace trace;
    private final boolean recurrent;
    private final OpponentTracking opponentTracking;
    private boolean closed;
    private final HealthAge ownHealth = new HealthAge();

    BotAvatar(NPCRegistry registry, UUID entityId, int npcId, String style, boolean recurrent,
              Location location, MotionTrace trace) {
        this.trace = trace;
        this.recurrent = recurrent;
        // The player-info packet limits the NPC profile name to 16 characters.
        String name = BotPolicy.RECURRENT_G2_PLUS.equals(style) ? "Recurrent-G2+" : style;
        npc = registry.createNPC(EntityType.PLAYER, entityId, npcId, name);
        try {
            ((CitizensNPC) npc).setEntityController(new Controller(entityId, trace));
            npc.setProtected(false);
            npc.setUseMinecraftAI(false);
            npc.data().set(NPC.Metadata.COLLIDABLE, false);
            npc.data().set(NPC.Metadata.PICKUP_ITEMS, false);
            npc.data().set(NPC.Metadata.REMOVE_FROM_PLAYERLIST, true);
            npc.data().set(NPC.Metadata.NAMEPLATE_VISIBLE, true);
            // Asynchronous profile lookup can respawn an NPC mid-duel.
            var skin = npc.getOrAddTrait(SkinTrait.class);
            skin.setFetchDefaultSkin(false);
            skin.setShouldUpdateSkins(false);
            if (!npc.spawn(location) || !(npc.getEntity() instanceof Player player)) {
                throw new IllegalStateException("Could not spawn the bot NPC.");
            }
            body = player;
            opponentTracking = new OpponentTracking(handle());
            if (!entityId.equals(body.getUniqueId()) || !entityId.equals(handle().getGameProfile().id())
                    || registry.getByUniqueId(entityId) != npc || registry.getNPC(body) != npc) {
                throw new IllegalStateException("Citizens bot identity does not match " + entityId);
            }
            if (trace != null) trace.record("spawn", "requested=" + entityId + " npc=" + npc.getUniqueId()
                    + " citizensProfile=" + npc.getMinecraftUniqueId() + " entity=" + body.getUniqueId());
            // Citizens initializes a higher NPC walking speed. The adapter sends
            // player inputs, so use the vanilla Player attribute baseline.
            handle().getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.1f);
            player().setCollidable(false);
        } catch (RuntimeException | LinkageError e) {
            try { npc.destroy(); }
            catch (RuntimeException | LinkageError cleanup) { e.addSuppressed(cleanup); }
            throw e;
        }
    }

    Player player() { return body; }
    boolean spawned() { return !closed && npc.isSpawned() && npc.getEntity() == body; }
    private DrivenPlayer handle() { return (DrivenPlayer) ((CraftPlayer) player()).getHandle(); }

    OpponentReplica.Frame receiveOpponent(Player opponent, long delayNs, java.util.function.Consumer<Object> trace) {
        return opponentTracking.observe(((CraftPlayer) opponent).getHandle(), delayNs, trace);
    }

    void stopTrackingOpponent() { opponentTracking.close(); }

    record Sample(Observation observation, RecurrentInput recurrent, float targetYaw, float targetPitch,
                  BoundingBox opponentBounds, Observation.Vec selfPosition, boolean predictedSelf, boolean diagnosticReplica) {}

    Sample observe(int elapsed, long oneWayNs, float remaining, int floorSurfaceY,
                   OpponentReplica.Frame replica, boolean predictSelf) {
        long receivedAtNs = elapsed * BotPing.TICK_NS + oneWayNs;
        Player self = player();
        var prediction = predictSelf ? predictedMotion() : null;
        Location at = prediction == null ? self.getLocation() : location(prediction);
        Vector opponentPosition = vector(replica.position());
        Vector displacement = vector(replica.displacement());
        Vector relative = opponentPosition.clone().subtract(at.toVector());
        Vector velocity = prediction == null ? self.getVelocity() : vector(prediction.velocity());
        Location eyeLocation = prediction == null ? self.getEyeLocation() : predictedEye(prediction);
        Vector eye = eyeLocation.toVector();
        BoundingBox box = playerBox(opponentPosition, replica.pose() == net.minecraft.world.entity.Pose.CROUCHING ? 1.5F : 1.8F);
        boolean grounded = prediction == null ? handle().onGround() : prediction.grounded();
        boolean opponentGrounded = replica.onGround();
        Vector opponentVelocity = vector(replica.velocity());
        var inventory = self.getInventory();
        var observation = new Observation(receivedAtNs, prediction == null ? (float) self.getHealth() : prediction.health(), vec(velocity),
                prediction == null ? handle().getAttackStrengthScale(0.5f) : prediction.charge(),
                prediction == null ? handle().inputSprinting : prediction.sprinting(),
                prediction == null ? self.isSneaking() : prediction.crouching(),
                horizontalSquared(velocity) >= 9e-6, grounded, !grounded && velocity.getY() > 0,
                !grounded && velocity.getY() < 0, at.getYaw(), at.getPitch(), vec(relative),
                vec(opponentVelocity), vec(displacement), replica.sprinting(), replica.pose() == net.minecraft.world.entity.Pose.CROUCHING,
                horizontalSquared(displacement) >= 9e-6, opponentGrounded,
                !opponentGrounded && displacement.getY() > 0, !opponentGrounded && displacement.getY() < 0,
                relative.length(), Math.sqrt(horizontalSquared(relative)),
                nearest(box, eye, 0).distance(eye), canHit(box, eyeLocation),
                replica.positionAge(), replica.metadataAge(), replica.velocityAge(), prediction == null ? oneWayNs : prediction.healthAge(),
                inventory.getItemInMainHand().getType() == Material.DIAMOND_SWORD,
                new boolean[]{inventory.getHelmet() != null, inventory.getChestplate() != null,
                        inventory.getLeggings() != null, inventory.getBoots() != null});
        Location aim = eyeLocation.clone();
        Vector toward = nearest(box, eye, 1e-4).subtract(eye);
        if (toward.lengthSquared() > 1e-12) aim.setDirection(toward);
        RecurrentInput extra = null;
        if (recurrent) {
            extra = new RecurrentInput(replica.health(), replica.healthAge(),
                    prediction == null ? ownHealth.sample(observation.health(), receivedAtNs) : prediction.healthAge(),
                    prediction == null ? handle().jumpCooldown : prediction.jumpDelay(), 0,
                    prediction == null ? self.getFallDistance() : prediction.fallDistance(), at.getY() - floorSurfaceY, remaining);
        }
        return new Sample(observation, extra, aim.getYaw(), aim.getPitch(), box,
                vec(at.toVector()), prediction != null, predictSelf);
    }

    private static final class HealthAge {
        private float lastHealth = Float.NaN;
        private long changedAtNs;

        long sample(float health, long receivedAtNs) {
            if (health != lastHealth) {
                lastHealth = health;
                changedAtNs = receivedAtNs;
            }
            return receivedAtNs - changedAtNs;
        }
    }

    boolean apply(Action action, Sample sample, Player opponent, boolean trackTarget, ParityAttackUplink attackUplink) {
        DrivenPlayer bot = handle();
        var prediction = attackUplink == null ? null : predictedMotion();
        Location eye = prediction == null ? player().getEyeLocation() : predictedEye(prediction);
        aim(eye, sample, trackTarget);
        setInput(action, eye, trackTarget);
        if (!action.click()) return false;
        boolean selected = canHit(sample.opponentBounds(), eye);
        if (attackUplink != null) {
            if (prediction != null) bot.motionTransport.localSwing();
            attackUplink.click(selected ? opponent.getEntityId() : null);
        } else {
            if (selected) bot.attack(((CraftPlayer) opponent).getHandle());
            bot.swing(InteractionHand.MAIN_HAND);
        }
        return selected;
    }

    boolean selectsOpponent(Sample sample, boolean trackTarget) {
        Location eye = player().getEyeLocation();
        aim(eye, sample, trackTarget);
        return canHit(sample.opponentBounds(), eye);
    }

    void applyReceived(Action action, Sample sample, boolean trackTarget, Integer targetId) {
        Location eye = player().getEyeLocation();
        aim(eye, sample, trackTarget);
        setInput(action, eye, trackTarget);
        if (!action.click()) return;
        var connection = handle().connection;
        if (targetId != null) new ServerboundAttackPacket(targetId).handle(connection);
        new ServerboundSwingPacket(InteractionHand.MAIN_HAND).handle(connection);
    }

    private static void aim(Location eye, Sample sample, boolean trackTarget) {
        if (trackTarget) {
            eye.setYaw(sample.targetYaw());
            eye.setPitch(sample.targetPitch());
        }
    }

    private void setInput(Action action, Location eye, boolean trackTarget) {
        DrivenPlayer bot = handle();
        if (trackTarget) {
            bot.setYRot(eye.getYaw());
            bot.setYHeadRot(eye.getYaw());
            bot.setYBodyRot(eye.getYaw());
            bot.setXRot(eye.getPitch());
        }
        bot.keys = action.keys();
    }

    private ParityMotionUplink.SelfMotion predictedMotion() {
        var transport = handle().motionTransport;
        return transport == null ? null : transport.selfMotion();
    }

    private Location location(ParityMotionUplink.SelfMotion motion) {
        return new Location(player().getWorld(), motion.position().x, motion.position().y, motion.position().z,
                motion.yaw(), motion.pitch());
    }

    private Location predictedEye(ParityMotionUplink.SelfMotion motion) {
        return location(motion).add(0, motion.crouching() ? 1.27F : 1.62F, 0);
    }

    void idle() {
        if (!spawned()) return;
        handle().keys = Action.IDLE.keys();
        player().clearActiveItem();
    }

    static java.util.List<Object> probeMotionEvents(Player player) {
        var handle = ((CraftPlayer) player).getHandle();
        return handle instanceof DrivenPlayer driven && driven.motionTransport != null
                ? driven.motionTransport.events() : java.util.List.of();
    }

    static void armMotionUplink(Player player, long delayNs, Long downlinkNs, boolean physicalInput, int initialAttackTicks) {
        var handle = ((CraftPlayer) player).getHandle();
        if (!(handle instanceof DrivenPlayer driven) || driven.motionTransport == null) {
            throw new IllegalStateException("Uplink requires experimental motion transport");
        }
        driven.motionTransport.armUplink(delayNs, downlinkNs, physicalInput, initialAttackTicks);
    }

    static java.util.List<Object> probeUplinkEvents(Player player) {
        var handle = ((CraftPlayer) player).getHandle();
        return handle instanceof DrivenPlayer driven && driven.motionTransport != null
                ? driven.motionTransport.uplinkEvents() : java.util.List.of();
    }

    void probeReset(com.google.gson.JsonObject initial) {
        var bot = handle();
        var p = initial.getAsJsonObject("position");
        var v = initial.getAsJsonObject("velocity");
        bot.setPos(p.get("x").getAsDouble(), p.get("y").getAsDouble(), p.get("z").getAsDouble());
        bot.setDeltaMovement(v.get("x").getAsDouble(), v.get("y").getAsDouble(), v.get("z").getAsDouble());
        bot.setYRot(initial.get("yaw").getAsFloat()); bot.setXRot(initial.get("pitch").getAsFloat());
        bot.setOnGround(initial.get("on_ground").getAsBoolean());
        bot.setSprinting(initial.get("sprinting").getAsBoolean());
        bot.inputSprinting = initial.get("sprinting").getAsBoolean();
        bot.jumpCooldown = initial.get("jump_delay").getAsInt();
        bot.previousSneak = initial.getAsJsonObject("input").get("sneak").getAsBoolean();
        bot.fallDistance = initial.get("fall_distance").getAsDouble();
    }

    static Action.Keys readKeys(com.google.gson.JsonObject action) {
        var k = action.getAsJsonObject("keys");
        java.util.function.Predicate<String> key = name -> k.has(name) && k.get(name).getAsBoolean();
        return new Action.Keys(key.test("forward"), key.test("backward"), key.test("left"),
                key.test("right"), key.test("jump"), key.test("sneak"));
    }

    void probeAction(com.google.gson.JsonObject action, com.google.gson.JsonObject feedback) {
        handle().keys = readKeys(action);
        if (action.has("yaw")) handle().setYRot(action.get("yaw").getAsFloat());
        if (action.has("pitch")) handle().setXRot(action.get("pitch").getAsFloat());
        handle().measuredSprint = feedback == null ? null : feedback.get("sprinting").getAsBoolean();
        if (feedback != null) {
            handle().measuredSpeed = feedback.get("movement_speed").getAsFloat();
            if (!Float.isFinite(handle().measuredSpeed) || handle().measuredSpeed < 0.0F) {
                throw new IllegalArgumentException("Invalid measured movement speed");
            }
        }
    }

    com.google.gson.JsonObject probeState() {
        var bot = handle();
        var out = new com.google.gson.JsonObject();
        var gson = new com.google.gson.Gson();
        out.add("position", gson.toJsonTree(java.util.Map.of("x", bot.getX(), "y", bot.getY(), "z", bot.getZ())));
        var v = bot.getDeltaMovement();
        out.add("velocity", gson.toJsonTree(java.util.Map.of("x", v.x, "y", v.y, "z", v.z)));
        out.addProperty("yaw", bot.getYRot()); out.addProperty("pitch", bot.getXRot());
        out.addProperty("on_ground", bot.onGround()); out.addProperty("sprinting", bot.inputSprinting);
        out.addProperty("pose", bot.getPose() == net.minecraft.world.entity.Pose.CROUCHING ? "crouching" : "standing");
        out.addProperty("fall_distance", bot.fallDistance); out.addProperty("jump_delay", bot.jumpCooldown);
        return out;
    }

    @Override public void close() {
        if (closed) return;
        stopTrackingOpponent();
        if (npc.getOwningRegistry().getByUniqueId(npc.getUniqueId()) == npc) npc.destroy();
        UUID id = body.getUniqueId();
        // Citizens can clear its controller on death before the player entity
        // leaves the world. The close event must follow complete removal.
        if (body.isValid() || Bukkit.getEntity(id) == body) {
            NMS.removeFromWorld(body);
            NMS.remove(body);
            if (trace != null) trace.record("remove-detached-body", id.toString());
        }
        boolean registered = npc.getOwningRegistry().getByUniqueId(npc.getUniqueId()) == npc;
        boolean inWorld = Bukkit.getEntity(id) != null;
        if (trace != null) trace.record("remove", "valid=" + body.isValid()
                + " registered=" + registered + " inWorld=" + inWorld);
        if (body.isValid() || registered || inWorld) {
            throw new IllegalStateException("Could not remove bot " + id);
        }
        closed = true;
    }

    private static BoundingBox playerBox(Vector position, float height) {
        double halfWidth = 0.6F / 2;
        return new BoundingBox(position.getX() - halfWidth, position.getY(), position.getZ() - halfWidth,
                position.getX() + halfWidth, position.getY() + height, position.getZ() + halfWidth);
    }

    private static Vector vector(Vec3 value) { return new Vector(value.x, value.y, value.z); }

    private static boolean canHit(BoundingBox box, Location eye) {
        Vector start = eye.toVector();
        if (box.contains(start)) return true;
        Vector direction = eye.getDirection();
        var target = box.rayTrace(start, direction, REACH);
        if (target == null) return false;
        double distance = target.getHitPosition().distance(start);
        if (distance >= REACH) return false;
        var obstruction = eye.getWorld().rayTraceBlocks(eye, direction, distance, FluidCollisionMode.NEVER, true);
        return obstruction == null;
    }

    private static Vector nearest(BoundingBox box, Vector eye, double inset) {
        return new Vector(Math.clamp(eye.getX(), box.getMinX() + inset, box.getMaxX() - inset),
                Math.clamp(eye.getY(), box.getMinY() + inset, box.getMaxY() - inset),
                Math.clamp(eye.getZ(), box.getMinZ() + inset, box.getMaxZ() - inset));
    }

    private static double horizontalSquared(Vector value) {
        return value.getX() * value.getX() + value.getZ() * value.getZ();
    }

    private static Observation.Vec vec(Vector value) {
        return new Observation.Vec(value.getX(), value.getY(), value.getZ());
    }

    private static final class Controller extends HumanController {
        private final UUID entityId;
        private final MotionTrace trace;

        Controller(UUID entityId, MotionTrace trace) {
            this.entityId = entityId;
            this.trace = trace;
        }

        @Override protected Entity createEntity(Location location, NPC npc) {
            ServerLevel level = ((CraftWorld) location.getWorld()).getHandle();
            // Citizens rewrites v4 NPC UUIDs for its default player profile.
            // The host reserves the actual player UUID before asking us to spawn.
            var profile = new GameProfile(entityId, profileName(npc));
            var player = new DrivenPlayer(level, profile, npc, trace);
            player.getBukkitEntity().setSleepingIgnored(true);
            player.getBukkitEntity().setVisibleByDefault(false);
            return player.getBukkitEntity();
        }

        private static String profileName(NPC npc) {
            // Below-name scores are keyed by the profile name, including for NPCs.
            var activeNames = new java.util.HashSet<String>();
            for (NPC other : npc.getOwningRegistry()) {
                if (other.getEntity() instanceof CraftPlayer player) {
                    activeNames.add(player.getHandle().getScoreboardName());
                }
            }
            for (int offset = 0; offset < 36 * 36; offset++) {
                String suffix = Integer.toString(Math.floorMod(npc.getId() + offset, 36 * 36), 36);
                String name = npc.getName() + "-" + suffix;
                if (!activeNames.contains(name) && org.bukkit.Bukkit.getPlayerExact(name) == null) return name;
            }
            throw new IllegalStateException("Could not allocate a unique bot profile name.");
        }
    }

    /** Citizens already calls travel once in its idle physics path. Replace that
     * zero input in place so gravity, friction and knockback run only once. */
    private static final class DrivenPlayer extends EntityHumanNPC {
        private static final double SPRINT_SPEED_MULTIPLIER = 1.0 + 0.3F;
        private Action.Keys keys = Action.IDLE.keys();
        private boolean applyingInput;
        private boolean inputSprinting;
        private boolean predictingSprintMovement;
        private Boolean measuredSprint;
        private float measuredSpeed;
        private boolean usingMeasuredSpeed;
        private int jumpCooldown;
        private boolean previousSneak;
        private final MotionTrace trace;
        private final ParityMotionTransport motionTransport;

        DrivenPlayer(ServerLevel level, GameProfile profile, NPC npc, MotionTrace trace) {
            super(level.getServer(), level, profile, ClientInformation.createDefault(), npc);
            this.trace = trace;
            motionTransport = PARITY_TRANSPORT ? new ParityMotionTransport(level, this, connection.connection) : null;
            if (motionTransport != null) {
                connection = motionTransport;
                net.citizensnpcs.nms.v26_1_R1.util.NMSImpl.markClientLoaded(connection);
            }
        }

        @Override public boolean hurtServer(ServerLevel level, net.minecraft.world.damagesource.DamageSource source, float amount) {
            return motionTransport == null ? super.hurtServer(level, source, amount)
                    : motionTransport.hurtServer(level, source, amount);
        }

        @Override public void doTick() {
            if (trace != null) trace.record("doTick", "");
            applyingInput = true;
            try { super.doTick(); }
            finally { applyingInput = false; }
        }

        @Override public boolean isSprinting() {
            return predictingSprintMovement || super.isSprinting();
        }

        @Override public float getSpeed() {
            if (usingMeasuredSpeed) return measuredSpeed;
            if (predictingSprintMovement) {
                return (float) (getAttributeValue(Attributes.MOVEMENT_SPEED) * SPRINT_SPEED_MULTIPLIER);
            }
            return super.getSpeed();
        }

        @Override public void travel(Vec3 input) {
            if (trace != null) trace.record("travel", "input=" + keys);
            if (!applyingInput) {
                super.travel(input);
                return;
            }
            applyingInput = false;
            if (motionTransport != null) motionTransport.beforePhysics(keys);
            // Citizens' idle path calls travel without LivingEntity.aiStep's player
            // dead zone. Apply the same pre-movement trimming as LocalPlayer.
            Vec3 velocity = getDeltaMovement();
            boolean stopped = velocity.horizontalDistanceSqr() < 9e-6;
            setDeltaMovement(stopped ? 0 : velocity.x, Math.abs(velocity.y) < 0.003 ? 0 : velocity.y,
                    stopped ? 0 : velocity.z);
            if (motionTransport != null && motionTransport.separateMotion()) {
                Vec3 accepted = position();
                super.travel(Vec3.ZERO);
                // The vanilla connection's tick restores authoritative coordinates
                // after ServerPlayer's movement step and retains the new velocity.
                absSnapTo(accepted.x, accepted.y, accepted.z, getYRot(), getXRot());
                return;
            }
            int forward = Boolean.compare(keys.forward(), keys.backward());
            int sideways = Boolean.compare(keys.left(), keys.right());
            setShiftKeyDown(keys.sneak());
            Boolean receivedSprint = measuredSprint;
            measuredSprint = null;
            if (receivedSprint != null) {
                inputSprinting = receivedSprint;
                setSprinting(receivedSprint);
            }
            // Send sprint changes only on input transitions. A sprint hit can clear
            // the server flag while the client still predicts sprint movement.
            boolean nextSprint = inputSprinting;
            if (!nextSprint && forward > 0 && !previousSneak) nextSprint = true;
            if (forward <= 0 || horizontalCollision) nextSprint = false;
            if (nextSprint != inputSprinting) {
                inputSprinting = nextSprint;
                setSprinting(nextSprint);
            }
            if (!keys.jump()) jumpCooldown = 0;
            else if (jumpCooldown > 0) jumpCooldown--;
            var shaped = PlayerMovementInput.shape(sideways, forward, previousSneak,
                    (float) getAttributeValue(Attributes.SNEAKING_SPEED));
            previousSneak = keys.sneak();
            Vec3 before = position();
            predictingSprintMovement = inputSprinting && !super.isSprinting();
            usingMeasuredSpeed = receivedSprint != null && nextSprint == receivedSprint;
            try {
                if (keys.jump() && jumpCooldown == 0 && onGround()) {
                    jumpFromGround();
                    jumpCooldown = 10;
                }
                super.travel(new Vec3(shaped.sideways(), 0, shaped.forward()));
            } finally {
                predictingSprintMovement = false;
                usingMeasuredSpeed = false;
            }
            setKnownMovement(position().subtract(before));
            if (trace != null) trace.record("physics", position() + " velocity=" + getDeltaMovement());
        }
    }
}
