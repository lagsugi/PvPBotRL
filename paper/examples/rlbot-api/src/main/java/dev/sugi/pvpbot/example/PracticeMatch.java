// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import com.destroystokyo.paper.event.server.ServerTickEndEvent;
import dev.sugi.pvpbot.api.BotClosedEvent;
import dev.sugi.pvpbot.api.BotHandle;
import dev.sugi.pvpbot.api.PvPBotApi;
import dev.sugi.pvpbot.common.PlayerRecovery;
import java.io.IOException;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.UUID;
import java.util.logging.Level;
import net.kyori.adventure.bossbar.BossBar;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.NamedTextColor;
import net.kyori.adventure.title.Title;
import org.bukkit.Bukkit;
import org.bukkit.GameMode;
import org.bukkit.Material;
import org.bukkit.attribute.Attribute;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.entity.EntityDamageByEntityEvent;
import org.bukkit.event.entity.EntityDamageEvent;
import org.bukkit.event.entity.EntityDamageEvent.DamageCause;
import org.bukkit.event.entity.EntityPickupItemEvent;
import org.bukkit.event.entity.EntityRegainHealthEvent;
import org.bukkit.event.entity.FoodLevelChangeEvent;
import org.bukkit.event.entity.PlayerDeathEvent;
import org.bukkit.event.inventory.InventoryClickEvent;
import org.bukkit.event.inventory.InventoryDragEvent;
import org.bukkit.event.inventory.InventoryOpenEvent;
import org.bukkit.event.player.PlayerCommandPreprocessEvent;
import org.bukkit.event.player.PlayerDropItemEvent;
import org.bukkit.event.player.PlayerItemHeldEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.event.player.PlayerRespawnEvent;
import org.bukkit.event.player.PlayerSwapHandItemsEvent;
import org.bukkit.event.player.PlayerTeleportEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.scoreboard.Scoreboard;
import org.bukkit.scoreboard.Team;
import org.bukkit.util.Vector;

final class PracticeMatch implements Listener {
    private static final int TICKS_PER_SECOND = 20;
    private static final int ROUND_TICKS = 300 * TICKS_PER_SECOND;
    private static final int COUNTDOWN_TICKS = 3 * TICKS_PER_SECOND;
    private final ExamplePlugin plugin;
    private final PvPBotApi api;
    private final PracticeArena arena;
    private final PlayerRecovery recovery;
    private final LinkedHashMap<UUID, String> previousModels = new LinkedHashMap<>();
    private Round active;
    private Player internalTeleport;

    PracticeMatch(ExamplePlugin plugin, PvPBotApi api) {
        this.plugin = plugin;
        this.api = api;
        arena = new PracticeArena(plugin);
        recovery = new PlayerRecovery(plugin.getDataFolder().toPath().resolve("recovery"));
    }

    void start(Player human, String model) throws IOException {
        if (active != null) throw new IllegalStateException("The practice arena is busy.");
        if (human.isDead() || human.isInsideVehicle() || human.isSleeping()) {
            throw new IllegalStateException("Stand up and leave any vehicle before starting.");
        }
        if (recovery.pending(human)) throw new IllegalStateException("Run /rlbotexample recover first.");
        var world = arena.prepare();
        human.closeInventory();
        BotHandle handle = null;
        try {
            arena.holdChunks(world);
            recovery.save(human);
            internalTeleport = human;
            try {
                if (!human.teleport(arena.spawn(world, false))) throw new IllegalStateException("Could not enter the arena.");
            } finally {
                internalTeleport = null;
            }
            handle = api.spawn(plugin, arena.spawn(world, true), model);
            var round = new Round(human, handle);
            active = round;
            prepareFighter(human);
            prepareFighter(round.bot);
            round.prepareDisplay(plugin);
            human.sendMessage("Sword duel against " + round.model + ". The round starts in 3 seconds.");
        } catch (IOException | RuntimeException | LinkageError error) {
            try {
                if (active != null) {
                    finish(Outcome.ABORTED);
                } else {
                    try {
                        if (handle != null) handle.close();
                    } finally {
                        arena.releaseChunks(world);
                        recover(human);
                    }
                }
            } catch (RuntimeException | LinkageError cleanup) {
                error.addSuppressed(cleanup);
            }
            throw error;
        }
    }

    void rematch(Player human) throws IOException {
        String previous = previousModels.get(human.getUniqueId());
        if (previous == null) throw new IllegalStateException("You have no recent practice opponent.");
        start(human, previous);
    }

    String status() {
        if (active == null) return "Practice arena ready.";
        String state = active.paused ? "paused" : active.running ? timeLeft(active.elapsed) + " remaining" : "warming up";
        return "Practice arena busy against " + active.model + ", " + state + ".";
    }

    void leave(Player human) {
        Round round = requireRound(human);
        finish(round.running ? Outcome.FORFEIT : Outcome.CANCELLED);
    }

    void pause(Player human, boolean paused) {
        Round round = requireRound(human);
        if (!round.running) throw new IllegalStateException("Wait for the countdown to finish.");
        if (round.paused == paused) return;
        if (paused) round.handle.pause();
        else round.handle.start(human);
        round.paused = paused;
        round.bar.name(Component.text(paused ? "Duel paused" : round.model + " • " + timeLeft(round.elapsed)));
        human.sendMessage(paused ? "Duel paused." : "Duel resumed.");
    }

    private Round requireRound(Player human) {
        if (active == null || !active.human.equals(human)) throw new IllegalStateException("You are not in a practice round.");
        return active;
    }

    @EventHandler
    public void tick(ServerTickEndEvent event) {
        Round round = active;
        if (round == null) return;
        try {
            if (finishDeaths(round)) return;
            if (!round.human.isOnline()) {
                finish(round.running ? Outcome.FORFEIT : Outcome.CANCELLED);
                return;
            }
            if (!arena.contains(round.human.getLocation()) || !arena.contains(round.bot.getLocation())) {
                plugin.getLogger().warning("A practice fighter left the arena bounds.");
                finish(Outcome.ABORTED);
                return;
            }
            if (round.countdown > 0) {
                if (round.countdown % TICKS_PER_SECOND == 0) {
                    int seconds = round.countdown / TICKS_PER_SECOND;
                    round.human.showTitle(Title.title(Component.text(seconds, NamedTextColor.GOLD),
                            Component.text("Sword Duel", NamedTextColor.GRAY)));
                }
                round.countdown--;
            } else if (!round.running) {
                round.handle.start(round.human);
                round.running = true;
                remember(round);
                round.human.showTitle(Title.title(Component.text("Fight!", NamedTextColor.GREEN), Component.empty()));
                round.human.sendMessage("Fight! You have five minutes.");
                round.updateHud();
            } else if (!round.paused) {
                round.elapsed++;
                if (round.elapsed >= ROUND_TICKS) finish(Outcome.TIMEOUT);
                else if (round.elapsed % 5 == 0) round.updateHud();
            }
        } catch (RuntimeException | LinkageError error) {
            plugin.getLogger().log(Level.WARNING, "Practice round failed", error);
            finish(Outcome.ABORTED);
        }
    }

    private void remember(Round round) {
        UUID id = round.human.getUniqueId();
        previousModels.remove(id);
        previousModels.put(id, round.model);
        if (previousModels.size() > 512) previousModels.pollFirstEntry();
    }

    private boolean finishDeaths(Round round) {
        boolean humanDead = round.humanDead || round.human.isDead();
        boolean botDead = round.botDead || round.bot.isDead();
        if (!humanDead && !botDead) return false;
        if (!round.running) {
            plugin.getLogger().warning("A practice fighter died before the countdown finished.");
            finish(Outcome.ABORTED);
        }
        else if (humanDead && botDead) finish(Outcome.DOUBLE_DEATH);
        else finish(humanDead ? Outcome.LOST : Outcome.WON);
        return true;
    }

    void close() { finish(Outcome.ABORTED); }

    private void finish(Outcome outcome) {
        Round round = active;
        if (round == null) return;
        active = null;
        try {
            round.handle.close();
        } catch (RuntimeException | LinkageError error) {
            plugin.getLogger().log(Level.WARNING, "Could not close the practice bot", error);
        } finally {
            try {
                if (round.human.isOnline()) {
                    round.human.hideBossBar(round.bar);
                    round.human.sendActionBar(Component.empty());
                    if (round.board != null && round.human.getScoreboard() == round.board) {
                        round.human.setScoreboard(round.originalBoard);
                    }
                }
            } finally {
                try { recover(round.human); }
                finally { arena.releaseChunks(round.world); }
            }
        }
        if (round.human.isOnline()) {
            round.human.sendMessage(outcome.message + " • " + round.model);
            if (!round.human.isDead()) {
                round.human.showTitle(Title.title(Component.text(outcome.title, outcome.color), Component.text(round.model)));
            }
        }
    }

    void requestRecovery(Player player) {
        if (active != null && active.human.equals(player)) throw new IllegalStateException("Leave your round first.");
        if (!recovery.pending(player)) throw new IllegalStateException("You have no pending recovery.");
        if (player.isDead()) throw new IllegalStateException("Respawn to recover your original state.");
        recover(player);
    }

    private void recover(Player player) {
        if (player.isDead() || !player.isOnline() || !recovery.pending(player)) return;
        internalTeleport = player;
        try {
            if (recovery.restore(player)) player.sendMessage("Your original state was restored.");
        } catch (IOException | RuntimeException error) {
            plugin.getLogger().log(Level.WARNING, "Practice recovery failed for " + player.getUniqueId(), error);
            player.sendMessage("Your recovery was saved. Retry with /rlbotexample recover.");
        } finally {
            internalTeleport = null;
        }
    }

    void scheduleRecovery(Player player) {
        if (recovery.pending(player)) Bukkit.getScheduler().runTask(plugin, () -> recover(player));
    }

    private void prepareFighter(Player player) {
        player.setGameMode(GameMode.SURVIVAL);
        if (player.getGameMode() != GameMode.SURVIVAL) throw new IllegalStateException("The duel requires survival mode.");
        player.setFlying(false);
        player.setAllowFlight(false);
        player.setInvulnerable(false);
        player.setCollidable(false);
        player.setVelocity(new Vector());
        player.setFallDistance(0);
        player.setFireTicks(0);
        player.setFreezeTicks(0);
        player.setRemainingAir(player.getMaximumAir());
        PlayerRecovery.clearEffects(player);
        player.getInventory().clear();
        player.setItemOnCursor(ItemStack.empty());
        var maxHealth = player.getAttribute(Attribute.MAX_HEALTH);
        if (maxHealth == null) throw new IllegalStateException("The fighter has no health attribute.");
        maxHealth.setBaseValue(20);
        if (maxHealth.getValue() != 20) throw new IllegalStateException("The duel requires a maximum of 20 HP.");
        player.setHealth(20);
        player.setAbsorptionAmount(0);
        player.setNoDamageTicks(0);
        player.setFoodLevel(20);
        player.setSaturation(0);
        player.setExhaustion(0);
        var inventory = player.getInventory();
        inventory.setHeldItemSlot(0);
        inventory.setItem(0, item(Material.DIAMOND_SWORD, false));
        inventory.setHelmet(item(Material.DIAMOND_HELMET, true));
        inventory.setChestplate(item(Material.DIAMOND_CHESTPLATE, true));
        inventory.setLeggings(item(Material.DIAMOND_LEGGINGS, true));
        inventory.setBoots(item(Material.DIAMOND_BOOTS, true));
    }

    private ItemStack item(Material material, boolean armor) {
        var item = new ItemStack(material);
        if (armor) item.addEnchantment(Enchantment.PROTECTION, 4);
        if (plugin.fixedEquipment()) item.editMeta(meta -> meta.setUnbreakable(true));
        return item;
    }

    @EventHandler
    public void botClosed(BotClosedEvent event) {
        Round round = active;
        if (round == null || event.getOwner() != plugin || !event.getBotId().equals(round.bot.getUniqueId())) return;
        if (!finishDeaths(round)) {
            plugin.getLogger().warning("The practice bot closed early (" + event.getReason() + ").");
            finish(Outcome.ABORTED);
        }
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void death(PlayerDeathEvent event) {
        Round round = active;
        if (round == null || !round.includes(event.getEntity())) return;
        event.setKeepInventory(false);
        event.getItemsToKeep().clear();
        event.getDrops().clear();
        event.setDroppedExp(0);
    }

    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void confirmDeath(PlayerDeathEvent event) {
        if (active == null) return;
        if (active.human.equals(event.getEntity())) active.humanDead = true;
        if (active.bot.equals(event.getEntity())) active.botDead = true;
    }

    @EventHandler public void join(PlayerJoinEvent event) { scheduleRecovery(event.getPlayer()); }
    @EventHandler public void respawn(PlayerRespawnEvent event) { scheduleRecovery(event.getPlayer()); }

    @EventHandler
    public void quit(PlayerQuitEvent event) {
        if (active != null && active.human.equals(event.getPlayer())) {
            if (!finishDeaths(active)) finish(active.running ? Outcome.FORFEIT : Outcome.CANCELLED);
        }
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void damage(EntityDamageEvent event) {
        if (!arena.owns(event.getEntity().getWorld())) return;
        Round round = active;
        if (round != null && round.running && !round.paused && round.includes(event.getEntity())) {
            if (event.getCause() == DamageCause.FALL) return;
            if (event instanceof EntityDamageByEntityEvent hit && round.includes(hit.getDamager())
                    && !hit.getDamager().equals(hit.getEntity())
                    && (hit.getCause() == DamageCause.ENTITY_ATTACK || hit.getCause() == DamageCause.ENTITY_SWEEP_ATTACK)) return;
        }
        event.setCancelled(true);
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void teleport(PlayerTeleportEvent event) {
        if (!event.getPlayer().equals(internalTeleport)
                && (arena.owns(event.getFrom().getWorld()) || (event.getTo() != null && arena.owns(event.getTo().getWorld())))) {
            event.setCancelled(true);
        }
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void move(PlayerMoveEvent event) {
        if (event instanceof PlayerTeleportEvent || active == null || !active.human.equals(event.getPlayer())) return;
        if (event.hasChangedPosition() && (!active.running || active.paused || !arena.contains(event.getTo()))) {
            event.setCancelled(true);
        }
    }

    @EventHandler(ignoreCancelled = true)
    public void command(PlayerCommandPreprocessEvent event) {
        if (!arena.owns(event.getPlayer().getWorld())) return;
        String command = event.getMessage().split("\\s+", 2)[0].toLowerCase(Locale.ROOT);
        if (!command.equals("/rlbotexample") && !command.equals("/pvpbotrlexample:rlbotexample")) {
            event.setCancelled(true);
            event.getPlayer().sendMessage("Use /rlbotexample leave to exit the arena.");
        }
    }

    @EventHandler(ignoreCancelled = true)
    public void food(FoodLevelChangeEvent event) {
        if (arena.owns(event.getEntity().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void heal(EntityRegainHealthEvent event) {
        if (arena.owns(event.getEntity().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void drop(PlayerDropItemEvent event) {
        if (arena.owns(event.getPlayer().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void pickup(EntityPickupItemEvent event) {
        if (arena.owns(event.getEntity().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void swap(PlayerSwapHandItemsEvent event) {
        if (arena.owns(event.getPlayer().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void slot(PlayerItemHeldEvent event) {
        if (arena.owns(event.getPlayer().getWorld()) && event.getNewSlot() != 0) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void open(InventoryOpenEvent event) {
        if (arena.owns(event.getPlayer().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void click(InventoryClickEvent event) {
        if (arena.owns(event.getWhoClicked().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void drag(InventoryDragEvent event) {
        if (arena.owns(event.getWhoClicked().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void place(BlockPlaceEvent event) {
        if (arena.owns(event.getBlock().getWorld())) event.setCancelled(true);
    }
    @EventHandler(ignoreCancelled = true)
    public void breakBlock(BlockBreakEvent event) {
        if (arena.owns(event.getBlock().getWorld())) event.setCancelled(true);
    }

    private static String timeLeft(int elapsed) {
        int seconds = Math.ceilDiv(ROUND_TICKS - elapsed, TICKS_PER_SECOND);
        return String.format(Locale.ROOT, "%d:%02d", seconds / 60, seconds % 60);
    }

    private enum Outcome {
        WON("Victory", "You defeated the bot.", NamedTextColor.GREEN),
        LOST("Defeat", "The bot won the duel.", NamedTextColor.RED),
        DOUBLE_DEATH("Draw", "Both fighters died.", NamedTextColor.YELLOW),
        TIMEOUT("Time limit", "No winner was awarded.", NamedTextColor.YELLOW),
        FORFEIT("Forfeit", "You left the duel.", NamedTextColor.RED),
        CANCELLED("Cancelled", "You left before the round started.", NamedTextColor.YELLOW),
        ABORTED("Interrupted", "The duel ended early.", NamedTextColor.YELLOW);

        private final String title;
        private final String message;
        private final NamedTextColor color;

        Outcome(String title, String message, NamedTextColor color) {
            this.title = title;
            this.message = message;
            this.color = color;
        }
    }

    private static final class Round {
        final Player human;
        final BotHandle handle;
        final Player bot;
        final org.bukkit.World world;
        final String model;
        final BossBar bar;
        final Scoreboard originalBoard;
        Scoreboard board;
        int countdown = COUNTDOWN_TICKS;
        int elapsed;
        boolean running;
        boolean paused;
        boolean humanDead;
        boolean botDead;

        Round(Player human, BotHandle handle) {
            this.human = human;
            this.handle = handle;
            bot = handle.player();
            world = bot.getWorld();
            model = handle.modelId();
            bar = BossBar.bossBar(Component.text(model + " • Starting"), 1, BossBar.Color.BLUE, BossBar.Overlay.PROGRESS);
            originalBoard = human.getScoreboard();
        }

        boolean includes(org.bukkit.entity.Entity entity) { return human.equals(entity) || bot.equals(entity); }

        void prepareDisplay(ExamplePlugin plugin) {
            board = Bukkit.getScoreboardManager().getNewScoreboard();
            Team fighters = board.registerNewTeam("rlbot_fighters");
            fighters.setOption(Team.Option.COLLISION_RULE, Team.OptionStatus.NEVER);
            fighters.addEntry(human.getName());
            fighters.addEntry(bot.getName());
            human.setScoreboard(board);
            human.showEntity(plugin, bot);
            human.showBossBar(bar);
        }

        void updateHud() {
            bar.name(Component.text(model + " • " + timeLeft(elapsed)));
            bar.progress(1f - (float) elapsed / ROUND_TICKS);
            human.sendActionBar(Component.text(String.format(Locale.ROOT, "You %.1f HP  |  Bot %.1f HP",
                    human.getHealth(), bot.getHealth())));
        }
    }
}
