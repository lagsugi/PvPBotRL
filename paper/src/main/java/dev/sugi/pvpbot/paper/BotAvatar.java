// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import com.mojang.authlib.GameProfile;
import dev.sugi.pvpbot.paper.policy.Action;
import dev.sugi.pvpbot.paper.policy.BotPolicy;
import dev.sugi.pvpbot.paper.policy.Observation;
import dev.sugi.pvpbot.paper.policy.RecurrentInput;
import net.citizensnpcs.api.npc.NPC;
import net.citizensnpcs.api.npc.NPCRegistry;
import net.citizensnpcs.nms.v26_1_R1.entity.EntityHumanNPC;
import net.citizensnpcs.nms.v26_1_R1.entity.HumanController;
import net.citizensnpcs.npc.CitizensNPC;
import net.citizensnpcs.trait.SkinTrait;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.phys.Vec3;
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
    private static final double REACH = 3.0;
    private final NPC npc;
    private final boolean recurrent;
    private final HealthAge ownHealth = new HealthAge();
    private final HealthAge opponentHealth = new HealthAge();
    private Vector lastOpponentPosition;

    BotAvatar(NPCRegistry registry, String style, Location location) {
        recurrent = BotPolicy.isRecurrent(style);
        npc = registry.createNPC(EntityType.PLAYER, style);
        try {
            ((CitizensNPC) npc).setEntityController(new Controller());
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
            if (!npc.spawn(location) || !(npc.getEntity() instanceof Player)) {
                throw new IllegalStateException("Could not spawn the bot NPC.");
            }
            // Citizens initializes a higher NPC walking speed. The adapter sends
            // player inputs, so use the vanilla Player attribute baseline.
            handle().getAttribute(Attributes.MOVEMENT_SPEED).setBaseValue(0.1f);
            player().setCollidable(false);
        } catch (RuntimeException | LinkageError e) {
            npc.destroy();
            throw e;
        }
    }

    Player player() { return (Player) npc.getEntity(); }
    boolean spawned() { return npc.isSpawned() && npc.getEntity() instanceof Player; }
    private DrivenPlayer handle() { return (DrivenPlayer) ((CraftPlayer) player()).getHandle(); }

    record Sample(Observation observation, RecurrentInput recurrent, float targetYaw, float targetPitch) {}

    Sample observe(Player opponent, int elapsed, long oneWayNs, int maxTicks, int floorSurfaceY) {
        long receivedAtNs = elapsed * BotPing.TICK_NS + oneWayNs;
        Player self = player();
        Location at = self.getLocation();
        Vector opponentPosition = opponent.getLocation().toVector();
        Vector displacement = lastOpponentPosition == null ? new Vector()
                : opponentPosition.clone().subtract(lastOpponentPosition);
        lastOpponentPosition = opponentPosition;
        Vector relative = opponentPosition.clone().subtract(at.toVector());
        Vector velocity = self.getVelocity();
        Vector eye = self.getEyeLocation().toVector();
        BoundingBox box = opponent.getBoundingBox();
        boolean grounded = handle().onGround();
        boolean opponentGrounded = ((CraftPlayer) opponent).getHandle().onGround();
        var inventory = self.getInventory();
        var observation = new Observation(receivedAtNs, (float) self.getHealth(), vec(velocity),
                handle().getAttackStrengthScale(0.5f), handle().inputSprinting, self.isSneaking(),
                horizontalSquared(velocity) >= 9e-6, grounded, !grounded && velocity.getY() > 0,
                !grounded && velocity.getY() < 0, at.getYaw(), at.getPitch(), vec(relative),
                vec(opponent.getVelocity()), vec(displacement), opponent.isSprinting(), opponent.isSneaking(),
                horizontalSquared(displacement) >= 9e-6, opponentGrounded,
                !opponentGrounded && displacement.getY() > 0, !opponentGrounded && displacement.getY() < 0,
                relative.length(), Math.sqrt(horizontalSquared(relative)),
                nearest(box, eye, 0).distance(eye), canHit(opponent, self.getEyeLocation()),
                oneWayNs, oneWayNs, oneWayNs, oneWayNs, inventory.getItemInMainHand().getType() == Material.DIAMOND_SWORD,
                new boolean[]{inventory.getHelmet() != null, inventory.getChestplate() != null,
                        inventory.getLeggings() != null, inventory.getBoots() != null});
        Location aim = self.getEyeLocation();
        Vector toward = nearest(box, eye, 1e-4).subtract(eye);
        if (toward.lengthSquared() > 1e-12) aim.setDirection(toward);
        RecurrentInput extra = null;
        if (recurrent) {
            float opponentHp = (float) opponent.getHealth();
            extra = new RecurrentInput(opponentHp, opponentHealth.sample(opponentHp, receivedAtNs),
                    ownHealth.sample(observation.health(), receivedAtNs), handle().jumpCooldown, 0,
                    self.getFallDistance(), at.getY() - floorSurfaceY, (float) (maxTicks - elapsed) / maxTicks);
        }
        return new Sample(observation, extra, aim.getYaw(), aim.getPitch());
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

    boolean apply(Action action, Sample sample, Player opponent) {
        DrivenPlayer bot = handle();
        Location eye = player().getEyeLocation();
        eye.setYaw(sample.targetYaw());
        eye.setPitch(sample.targetPitch());
        bot.setYRot(eye.getYaw());
        bot.setYHeadRot(eye.getYaw());
        bot.setYBodyRot(eye.getYaw());
        bot.setXRot(eye.getPitch());
        boolean hitAttempt = false;
        if (action.click()) {
            if (canHit(opponent, eye)) {
                bot.attack(((CraftPlayer) opponent).getHandle());
                hitAttempt = true;
            }
            bot.swing(net.minecraft.world.InteractionHand.MAIN_HAND, true);
        }
        bot.keys = action.keys();
        return hitAttempt;
    }

    void idle() { if (spawned()) handle().keys = Action.IDLE.keys(); }
    @Override public void close() { npc.destroy(); }

    private static boolean canHit(Player opponent, Location eye) {
        Vector start = eye.toVector();
        BoundingBox box = opponent.getBoundingBox();
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
        @Override protected Entity createEntity(Location location, NPC npc) {
            ServerLevel level = ((CraftWorld) location.getWorld()).getHandle();
            var profile = new GameProfile(npc.getMinecraftUniqueId(), profileName(npc));
            var player = new DrivenPlayer(level, profile, npc);
            player.getBukkitEntity().setSleepingIgnored(true);
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
                if (!activeNames.contains(name)) return name;
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
        private int jumpCooldown;

        DrivenPlayer(ServerLevel level, GameProfile profile, NPC npc) {
            super(level.getServer(), level, profile, ClientInformation.createDefault(), npc);
        }

        @Override public void doTick() {
            applyingInput = true;
            try { super.doTick(); }
            finally { applyingInput = false; }
        }

        @Override public boolean isSprinting() {
            return predictingSprintMovement || super.isSprinting();
        }

        @Override public float getSpeed() {
            if (predictingSprintMovement) {
                return (float) (getAttributeValue(Attributes.MOVEMENT_SPEED) * SPRINT_SPEED_MULTIPLIER);
            }
            return super.getSpeed();
        }

        @Override public void travel(Vec3 input) {
            if (!applyingInput) {
                super.travel(input);
                return;
            }
            applyingInput = false;
            int forward = Boolean.compare(keys.forward(), keys.backward());
            int sideways = Boolean.compare(keys.left(), keys.right());
            setShiftKeyDown(keys.sneak());
            // Send sprint changes only on input transitions. A sprint hit can clear
            // the server flag while the client still predicts sprint movement.
            boolean nextSprint = forward > 0 && !keys.sneak() && !horizontalCollision;
            if (nextSprint != inputSprinting) {
                inputSprinting = nextSprint;
                setSprinting(nextSprint);
            }
            if (!keys.jump()) jumpCooldown = 0;
            else if (jumpCooldown > 0) jumpCooldown--;
            double length = Math.max(1, Math.hypot(sideways, forward));
            double scale = (keys.sneak() ? getAttributeValue(Attributes.SNEAKING_SPEED) : 1.0) * 0.98 / length;
            Vec3 before = position();
            predictingSprintMovement = inputSprinting && !super.isSprinting();
            try {
                if (keys.jump() && jumpCooldown == 0 && onGround()) {
                    jumpFromGround();
                    jumpCooldown = 10;
                }
                super.travel(new Vec3(sideways * scale, 0, forward * scale));
            } finally {
                predictingSprintMovement = false;
            }
            setKnownMovement(position().subtract(before));
        }
    }
}
