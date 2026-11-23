// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import io.papermc.paper.math.Position;
import java.util.Random;
import org.bukkit.Bukkit;
import org.bukkit.GameRules;
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
    private static final int WALL_HEIGHT = 5;
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

    Stage prepare() {
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
        return new Stage(world.getUID(), world.getName(), -HALF_WIDTH, FLOOR_Y, -HALF_WIDTH,
                WIDTH, WIDTH, WALL_HEIGHT, spawn(false), spawn(true),
                new Stage.StartPoint(0.5, FEET_Y + WALL_HEIGHT + 1, 0.5, 0, 45));
    }

    private static Stage.StartPoint spawn(boolean bot) {
        double z = 0.5 + (bot ? SPAWN_OFFSET : -SPAWN_OFFSET);
        return new Stage.StartPoint(0.5, FEET_Y, z, bot ? 180 : 0, 0);
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
                        chunk.setRegion(x, FEET_Y, z, x + 1, FEET_Y + WALL_HEIGHT, z + 1, Material.GLASS);
                    }
                }
            }
        }
    }
}
