// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import com.destroystokyo.paper.event.player.PlayerStartSpectatingEntityEvent;
import com.destroystokyo.paper.event.player.PlayerStopSpectatingEntityEvent;
import com.destroystokyo.paper.event.player.PlayerAdvancementCriterionGrantEvent;
import java.util.List;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.entity.Projectile;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.block.BlockBurnEvent;
import org.bukkit.event.block.BlockExplodeEvent;
import org.bukkit.event.block.BlockFadeEvent;
import org.bukkit.event.block.BlockFormEvent;
import org.bukkit.event.block.BlockFromToEvent;
import org.bukkit.event.block.BlockGrowEvent;
import org.bukkit.event.block.BlockIgniteEvent;
import org.bukkit.event.block.BlockPistonExtendEvent;
import org.bukkit.event.block.BlockPistonRetractEvent;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.block.BlockSpreadEvent;
import org.bukkit.event.entity.EntityChangeBlockEvent;
import org.bukkit.event.entity.EntityDamageByEntityEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.EntityExplodeEvent;
import org.bukkit.event.entity.EntityMountEvent;
import org.bukkit.event.entity.EntityPickupItemEvent;
import org.bukkit.event.entity.EntityPotionEffectEvent;
import org.bukkit.event.entity.EntityRegainHealthEvent;
import org.bukkit.event.entity.FoodLevelChangeEvent;
import org.bukkit.event.entity.PlayerDeathEvent;
import org.bukkit.event.inventory.InventoryClickEvent;
import org.bukkit.event.inventory.InventoryDragEvent;
import org.bukkit.event.player.PlayerBucketEmptyEvent;
import org.bukkit.event.player.PlayerBucketFillEvent;
import org.bukkit.event.player.PlayerDropItemEvent;
import org.bukkit.event.player.PlayerGameModeChangeEvent;
import org.bukkit.event.player.PlayerInteractEntityEvent;
import org.bukkit.event.player.PlayerInteractEvent;
import org.bukkit.event.player.PlayerItemHeldEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.event.player.PlayerRespawnEvent;
import org.bukkit.event.player.PlayerSwapHandItemsEvent;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.event.world.WorldUnloadEvent;

final class ArenaEvents implements Listener {
    private final PvpBotPlugin plugin;
    private final StageStore stages;
    private final MatchManager matches;
    private final Menus menus;

    ArenaEvents(PvpBotPlugin plugin, StageStore stages, MatchManager matches, Menus menus) {
        this.plugin = plugin;
        this.stages = stages;
        this.matches = matches;
        this.menus = menus;
    }

    private boolean managed(Entity entity) {
        return entity instanceof Player player && matches.managed(player);
    }

    private static Entity source(EntityDamageByEntityEvent event) {
        Entity direct = event.getDamager();
        if (direct instanceof Projectile projectile && projectile.getShooter() instanceof Entity shooter) return shooter;
        return direct;
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void damage(EntityDamageEvent event) {
        Entity entity = event.getEntity();
        MatchManager.Seat victim = matches.seat(entity);
        if (event instanceof EntityDamageByEntityEvent attack) {
            Entity source = source(attack);
            MatchManager.Seat attacker = matches.seat(source);
            if (!managed(entity) && !managed(source)) return;
            if (victim == null || attacker == null || victim.match() != attacker.match()
                    || victim.side() == attacker.side() || !victim.match().running()
                    || victim.match().fighters[victim.side()].died || victim.match().fighters[attacker.side()].died) {
                event.setCancelled(true);
            }
        } else if (managed(entity) && (victim == null || !victim.match().running()
                || event.getCause() != EntityDamageEvent.DamageCause.FALL)) {
            event.setCancelled(true);
        }
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void countDamage(EntityDamageByEntityEvent event) {
        var victim = matches.seat(event.getEntity());
        var attacker = matches.seat(source(event));
        if (victim != null && attacker != null && victim.match() == attacker.match() && attacker.side() != victim.side()) {
            var match = victim.match();
            match.fighters[attacker.side()].damage += Math.clamp(event.getFinalDamage(), 0, match.fighters[victim.side()].hp());
        }
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void death(PlayerDeathEvent event) {
        var seat = matches.seat(event.getEntity());
        if (seat == null) return;
        event.getDrops().clear();
        event.setDroppedExp(0);
        event.setKeepInventory(true);
        event.setKeepLevel(true);
        event.deathMessage(null);
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void recordDeath(PlayerDeathEvent event) {
        var seat = matches.seat(event.getEntity());
        // The next tick resolves both sides after native death handling has completed.
        if (seat != null) seat.match().fighters[seat.side()].died = true;
    }

    @EventHandler public void join(PlayerJoinEvent event) { recoverAfterSpawn(event.getPlayer()); }
    @EventHandler public void respawn(PlayerRespawnEvent event) { recoverAfterSpawn(event.getPlayer()); }

    @EventHandler
    public void quit(PlayerQuitEvent event) {
        matches.quit(event.getPlayer());
        menus.forget(event.getPlayer());
    }

    private void recoverAfterSpawn(Player player) {
        if (player.hasMetadata("NPC")) return;
        Bukkit.getScheduler().runTask(plugin, () -> {
            if (!matches.managed(player)) matches.restore(player);
            matches.showRespawnResult(player);
        });
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void move(PlayerMoveEvent event) {
        if (event instanceof PlayerTeleportEvent || !event.hasChangedPosition()) return;
        Player player = event.getPlayer();
        var view = matches.view(player);
        var seat = matches.seat(player);
        Location to = event.getTo();
        if (view != null && !view.match().stage.contains(to, true)) {
            event.setTo(view.match().stage.spectator().at(view.match().world));
        } else if (seat != null) {
            Location fallback = null;
            if (!seat.match().running()) fallback = event.getFrom().clone();
            else if (!seat.match().stage.contains(to, false)) fallback = seat.match().fighters[seat.side()].safe.clone();
            if (fallback == null) return;
            fallback.setYaw(to.getYaw());
            fallback.setPitch(to.getPitch());
            event.setTo(fallback);
            player.setVelocity(new org.bukkit.util.Vector());
            player.setFallDistance(0);
        }
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void teleport(PlayerTeleportEvent event) {
        if (managed(event.getPlayer()) && !matches.changing(event.getPlayer())) event.setCancelled(true);
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void startCamera(PlayerStartSpectatingEntityEvent event) {
        if (matches.view(event.getPlayer()) != null && !matches.changing(event.getPlayer())) event.setCancelled(true);
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void stopCamera(PlayerStopSpectatingEntityEvent event) { matches.detached(event.getPlayer()); }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void mode(PlayerGameModeChangeEvent event) {
        if (managed(event.getPlayer()) && !matches.changing(event.getPlayer())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void worldUnload(WorldUnloadEvent event) {
        if (matches.uses(event.getWorld())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void potion(EntityPotionEffectEvent event) {
        if (event.getEntity() instanceof Player player && managed(player) && !matches.changing(player)) {
            event.setCancelled(true);
        }
    }

    @EventHandler(ignoreCancelled = true)
    public void advancement(PlayerAdvancementCriterionGrantEvent event) {
        if (matches.seat(event.getPlayer()) != null) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void mount(EntityMountEvent event) {
        if (managed(event.getEntity()) || managed(event.getMount())) event.setCancelled(true);
    }

    private boolean locked(Block block) { return stages.protects(block, false); }
    private boolean locked(Block block, Player player) { return stages.protects(block, player.hasPermission("pvpbot.admin")); }

    @EventHandler(ignoreCancelled = true)
    public void breakBlock(BlockBreakEvent event) {
        if (managed(event.getPlayer()) || locked(event.getBlock(), event.getPlayer())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void place(BlockPlaceEvent event) {
        if (managed(event.getPlayer()) || locked(event.getBlock(), event.getPlayer())
                || event instanceof org.bukkit.event.block.BlockMultiPlaceEvent multi
                && multi.getReplacedBlockStates().stream().anyMatch(state -> locked(state.getBlock(), event.getPlayer()))) {
            event.setCancelled(true);
        }
    }

    @EventHandler(ignoreCancelled = true)
    public void explode(EntityExplodeEvent event) { event.blockList().removeIf(this::locked); }

    @EventHandler(ignoreCancelled = true)
    public void explodeBlock(BlockExplodeEvent event) { event.blockList().removeIf(this::locked); }

    @EventHandler(ignoreCancelled = true)
    public void flow(BlockFromToEvent event) {
        if (locked(event.getBlock()) || locked(event.getToBlock())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void bucketFill(PlayerBucketFillEvent event) {
        if (managed(event.getPlayer()) || locked(event.getBlock(), event.getPlayer())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void bucketEmpty(PlayerBucketEmptyEvent event) {
        if (managed(event.getPlayer()) || locked(event.getBlock(), event.getPlayer())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void pistonExtend(BlockPistonExtendEvent event) {
        if (pistonLocked(event.getBlock(), event.getDirection(), event.getBlocks(), false)) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void pistonRetract(BlockPistonRetractEvent event) {
        if (pistonLocked(event.getBlock(), event.getDirection(), event.getBlocks(), true)) event.setCancelled(true);
    }

    private boolean pistonLocked(Block piston, BlockFace direction, List<Block> moved, boolean retract) {
        BlockFace move = retract ? direction.getOppositeFace() : direction;
        return locked(piston) || locked(piston.getRelative(direction))
                || moved.stream().anyMatch(block -> locked(block) || locked(block.getRelative(move)));
    }

    @EventHandler(ignoreCancelled = true)
    public void entityBlock(EntityChangeBlockEvent event) {
        if (locked(event.getBlock()) || managed(event.getEntity())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void burn(BlockBurnEvent event) { if (locked(event.getBlock())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void ignite(BlockIgniteEvent event) { if (locked(event.getBlock())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void fade(BlockFadeEvent event) { if (locked(event.getBlock())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void form(BlockFormEvent event) { if (locked(event.getBlock())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void grow(BlockGrowEvent event) { if (locked(event.getBlock())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void spread(BlockSpreadEvent event) { if (locked(event.getBlock())) event.setCancelled(true); }

    @EventHandler(ignoreCancelled = true)
    public void drop(PlayerDropItemEvent event) { if (managed(event.getPlayer())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void pickup(EntityPickupItemEvent event) { if (managed(event.getEntity())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void slot(PlayerItemHeldEvent event) { if (matches.seat(event.getPlayer()) != null) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void swap(PlayerSwapHandItemsEvent event) { if (managed(event.getPlayer())) event.setCancelled(true); }

    @EventHandler(ignoreCancelled = true)
    public void inventory(InventoryClickEvent event) {
        if (event.getWhoClicked() instanceof Player player && managed(player) && !menus.isOurMenu(player)) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void drag(InventoryDragEvent event) {
        if (event.getWhoClicked() instanceof Player player && managed(player)) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void interact(PlayerInteractEvent event) {
        if (event.getAction().isRightClick() && managed(event.getPlayer())
                || event.getClickedBlock() != null && locked(event.getClickedBlock(), event.getPlayer())) {
            event.setCancelled(true);
        }
    }

    @EventHandler(ignoreCancelled = true)
    public void interactEntity(PlayerInteractEntityEvent event) {
        if (managed(event.getPlayer()) || managed(event.getRightClicked())) event.setCancelled(true);
    }

    @EventHandler(ignoreCancelled = true)
    public void food(FoodLevelChangeEvent event) { if (managed(event.getEntity())) event.setCancelled(true); }
    @EventHandler(ignoreCancelled = true)
    public void heal(EntityRegainHealthEvent event) { if (managed(event.getEntity())) event.setCancelled(true); }
}
