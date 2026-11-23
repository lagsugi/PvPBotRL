// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import io.papermc.paper.math.Position;
import java.util.Random;
import org.bukkit.Bukkit;
import org.bukkit.GameRules;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.WorldCreator;
import org.bukkit.generator.ChunkGenerator;
import org.bukkit.generator.WorldInfo;
import org.bukkit.plugin.Plugin;

final class PracticeArena {
    private static final int WIDTH = 80;
    private static final int HALF_WIDTH = WIDTH / 2;
    private static final int SPAWN_OFFSET = 10;
    private static final int FLOOR_Y = 64;
    private static final int FEET_Y = FLOOR_Y + 1;
    private final NamespacedKey worldKey;
    private final Plugin plugin;

    PracticeArena(Plugin plugin) {
        this.plugin = plugin;
        // Use a separate world when the arena footprint changes. The old chunks retain their geometry.
        worldKey = new NamespacedKey(plugin, "practice_" + WIDTH);
    }

    boolean owns(World world) {
        return world != null && worldKey.equals(world.getKey());
    }

    World prepare() {
        World world = Bukkit.getWorld(worldKey);
        if (world == null) {
            world = new WorldCreator(worldKey)
                    .generator(new ArenaGenerator())
                    .generateStructures(false)
                    .forcedSpawnPosition(Position.block(0, FEET_Y, 0), 0, 0)
                    .createWorld();
        }
        if (world == null || !owns(world)) throw new IllegalStateException("Could not load the practice arena.");
        world.setGameRule(GameRules.NATURAL_HEALTH_REGENERATION, false);
        world.setGameRule(GameRules.PVP, true);
        world.setGameRule(GameRules.SPAWN_MOBS, false);
        world.setGameRule(GameRules.ADVANCE_TIME, false);
        world.setGameRule(GameRules.ADVANCE_WEATHER, false);
        world.setTime(6000);
        world.setStorm(false);
        world.setThundering(false);
        for (Location at : java.util.List.of(spawn(world, false), spawn(world, true))) {
            if (world.getBlockAt(at.getBlockX(), FLOOR_Y, at.getBlockZ()).getType() != Material.SMOOTH_STONE
                    || !at.getBlock().getType().isAir() || !at.clone().add(0, 1, 0).getBlock().getType().isAir()) {
                throw new IllegalStateException("The practice arena's duel spawns are obstructed.");
            }
        }
        return world;
    }

    Location spawn(World world, boolean bot) {
        double z = 0.5 + (bot ? SPAWN_OFFSET : -SPAWN_OFFSET);
        return new Location(world, 0.5, FEET_Y, z, bot ? 180 : 0, 0);
    }

    void holdChunks(World world) {
        int firstChunk = Math.floorDiv(-HALF_WIDTH, 16);
        int lastChunk = Math.floorDiv(HALF_WIDTH - 1, 16);
        for (int x = firstChunk; x <= lastChunk; x++) {
            for (int z = firstChunk; z <= lastChunk; z++) {
                world.addPluginChunkTicket(x, z, plugin);
            }
        }
    }

    void releaseChunks(World world) {
        world.removePluginChunkTickets(plugin);
    }

    boolean contains(Location location) {
        double inner = HALF_WIDTH - 1;
        return owns(location.getWorld()) && location.getY() >= FLOOR_Y
                && location.getX() >= -inner && location.getX() < inner
                && location.getZ() >= -inner && location.getZ() < inner;
    }

    private static final class ArenaGenerator extends ChunkGenerator {
        @Override
        public void generateNoise(WorldInfo world, Random random, int chunkX, int chunkZ, ChunkData chunk) {
            for (int x = 0; x < 16; x++) {
                int blockX = chunkX * 16 + x;
                if (blockX < -HALF_WIDTH || blockX >= HALF_WIDTH) continue;
                for (int z = 0; z < 16; z++) {
                    int blockZ = chunkZ * 16 + z;
                    if (blockZ < -HALF_WIDTH || blockZ >= HALF_WIDTH) continue;
                    chunk.setBlock(x, FLOOR_Y, z, Material.SMOOTH_STONE);
                    if (blockX == -HALF_WIDTH || blockX == HALF_WIDTH - 1
                            || blockZ == -HALF_WIDTH || blockZ == HALF_WIDTH - 1) {
                        chunk.setRegion(x, FEET_Y, z, x + 1, FEET_Y + 5, z + 1, Material.GLASS);
                    }
                }
            }
        }
    }
}
