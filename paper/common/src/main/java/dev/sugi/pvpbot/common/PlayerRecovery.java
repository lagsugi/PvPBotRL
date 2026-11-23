// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.common;

import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.util.Base64;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import org.bukkit.Bukkit;
import org.bukkit.GameMode;
import org.bukkit.Location;
import org.bukkit.attribute.Attribute;
import org.bukkit.configuration.InvalidConfigurationException;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.entity.Player;
import org.bukkit.inventory.ItemStack;
import org.bukkit.potion.PotionEffect;
import org.bukkit.util.Vector;

/** A recovery file exists from before kit assignment until the restored player is saved. */
public final class PlayerRecovery {
    private final Path directory;

    public PlayerRecovery(Path directory) { this.directory = directory; }

    private Path file(Player player) { return directory.resolve(player.getUniqueId() + ".yml"); }
    public boolean pending(Player player) { return Files.exists(file(player)); }

    public void save(Player player) throws IOException {
        if (pending(player)) throw new IllegalStateException("Player recovery is pending.");
        var state = new YamlConfiguration();
        Location loc = player.getLocation();
        state.set("world", loc.getWorld().getUID().toString());
        state.set("position", List.of(loc.getX(), loc.getY(), loc.getZ(), (double) loc.getYaw(), (double) loc.getPitch()));
        state.set("mode", player.getGameMode().name());
        if (player.getGameMode() == GameMode.SPECTATOR && player.getSpectatorTarget() != null) {
            state.set("spectator-target", player.getSpectatorTarget().getUniqueId().toString());
        }
        state.set("inventory", Base64.getEncoder().encodeToString(ItemStack.serializeItemsAsBytes(player.getInventory().getContents())));
        var cursor = player.getItemOnCursor();
        state.set("cursor", cursor.isEmpty() ? null : Base64.getEncoder().encodeToString(cursor.serializeAsBytes()));
        state.set("held-slot", player.getInventory().getHeldItemSlot());
        state.set("health", player.getHealth());
        state.set("max-health-base", Objects.requireNonNull(player.getAttribute(Attribute.MAX_HEALTH)).getBaseValue());
        state.set("absorption", player.getAbsorptionAmount());
        state.set("food", player.getFoodLevel());
        state.set("saturation", player.getSaturation());
        state.set("exhaustion", player.getExhaustion());
        state.set("level", player.getLevel());
        state.set("exp", player.getExp());
        state.set("total-exp", player.getTotalExperience());
        state.set("allow-flight", player.getAllowFlight());
        state.set("flying", player.isFlying());
        state.set("collidable", player.isCollidable());
        state.set("invulnerable", player.isInvulnerable());
        state.set("fire", player.getFireTicks());
        state.set("air", player.getRemainingAir());
        state.set("freeze", player.getFreezeTicks());
        state.set("effects", List.copyOf(player.getActivePotionEffects()));
        Files.createDirectories(directory);
        Path destination = file(player);
        Path temporary = destination.resolveSibling(destination.getFileName() + ".tmp");
        Files.writeString(temporary, state.saveToString());
        try (FileChannel channel = FileChannel.open(temporary, StandardOpenOption.WRITE)) {
            channel.force(true);
        }
        Files.move(temporary, destination, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
    }

    public boolean restore(Player player) throws IOException {
        if (!pending(player) || player.isDead()) return false;
        var state = new YamlConfiguration();
        try { state.loadFromString(Files.readString(file(player))); }
        catch (InvalidConfigurationException e) { throw new IOException("Invalid player recovery data", e); }
        var world = Bukkit.getWorld(UUID.fromString(Objects.requireNonNull(state.getString("world"))));
        if (world == null) throw new IOException("The player's original world is not loaded.");
        var position = state.getDoubleList("position");
        var destination = new Location(world, position.get(0), position.get(1), position.get(2),
                position.get(3).floatValue(), position.get(4).floatValue());
        var items = ItemStack.deserializeItemsFromBytes(Base64.getDecoder().decode(state.getString("inventory")));
        var cursor = state.getString("cursor");
        var mode = GameMode.valueOf(state.getString("mode"));
        if (!player.teleport(destination)) throw new IOException("Could not restore the player's original location.");
        player.getInventory().setContents(items);
        player.getInventory().setHeldItemSlot(state.getInt("held-slot"));
        player.setItemOnCursor(cursor == null ? ItemStack.empty() : ItemStack.deserializeBytes(Base64.getDecoder().decode(cursor)));
        player.setGameMode(mode);
        if (player.getGameMode() != mode) throw new IOException("Could not restore the player's original game mode.");
        player.setAllowFlight(state.getBoolean("allow-flight"));
        player.setFlying(state.getBoolean("flying"));
        player.setCollidable(state.getBoolean("collidable"));
        player.setInvulnerable(state.getBoolean("invulnerable"));
        player.setVelocity(new Vector());
        player.setFallDistance(0);
        player.setFireTicks(state.getInt("fire"));
        player.setRemainingAir(state.getInt("air"));
        player.setFreezeTicks(state.getInt("freeze"));
        player.setFoodLevel(state.getInt("food"));
        player.setSaturation((float) state.getDouble("saturation"));
        player.setExhaustion((float) state.getDouble("exhaustion"));
        player.setLevel(state.getInt("level"));
        player.setExp((float) state.getDouble("exp"));
        player.setTotalExperience(state.getInt("total-exp"));
        clearEffects(player);
        for (Object effect : Objects.requireNonNull(state.getList("effects"))) player.addPotionEffect((PotionEffect) effect);
        var health = Objects.requireNonNull(player.getAttribute(Attribute.MAX_HEALTH));
        health.setBaseValue(state.getDouble("max-health-base"));
        player.setHealth(Math.min(state.getDouble("health"), health.getValue()));
        player.setAbsorptionAmount(state.getDouble("absorption"));
        player.setNoDamageTicks(0);
        if (mode == GameMode.SPECTATOR) {
            String targetId = state.getString("spectator-target");
            var target = targetId == null ? null : Bukkit.getEntity(UUID.fromString(targetId));
            if (target == null || target.isValid() && target.getWorld().equals(world)) player.setSpectatorTarget(target);
        }
        player.saveData();
        Files.delete(file(player));
        return true;
    }

    public static void clearEffects(Player player) {
        for (var effect : player.getActivePotionEffects()) player.removePotionEffect(effect.getType());
    }
}
