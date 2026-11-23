// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import java.util.UUID;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;

record Stage(UUID worldId, String worldName, int minX, int floorY, int minZ,
             int width, int depth, int height, StartPoint a, StartPoint b, StartPoint spectator) {
    static final int WIDTH = 64;
    static final int CLEAR_HEIGHT = 8;

    static Stage generated(Location origin, double distance, int width, int depth) {
        dimensions(width, depth);
        World world = origin.getWorld();
        int x = origin.getBlockX() - width / 2;
        int z = origin.getBlockZ() - depth / 2;
        int floor = origin.getBlockY() - 1;
        double centerX = x + width / 2.0;
        double centerZ = z + depth / 2.0;
        float yaw = origin.getYaw();
        var stage = new Stage(world.getUID(), world.getName(), x, floor, z, width, depth, CLEAR_HEIGHT,
                startPoint(centerX, floor, centerZ, yaw, distance, false),
                startPoint(centerX, floor, centerZ, yaw, distance, true),
                new StartPoint(centerX, floor + CLEAR_HEIGHT - 2, centerZ, yaw, 35));
        stage.validateBounds(world);
        stage.validatePoints();
        return stage;
    }

    static void dimensions(int width, int depth) {
        if (width < 16 || width > 128 || depth < 16 || depth > 128) {
            throw new IllegalArgumentException("Arena width and depth must be between 16 and 128 blocks.");
        }
    }

    World world() { return Bukkit.getWorld(worldId); }

    Location spawn(World world, int side) { return (side == 0 ? a : b).at(world); }

    record StartPoint(double x, double y, double z, float yaw, float pitch) {
        StartPoint(double x, double y, double z, float yaw) { this(x, y, z, yaw, 0); }
        static StartPoint from(Location location) {
            return new StartPoint(location.getX(), location.getY(), location.getZ(),
                    location.getYaw(), location.getPitch());
        }
        Location at(World world) { return new Location(world, x, y, z, yaw, pitch); }
    }

    static StartPoint startPoint(double x, int floorY, double z, float yaw, double distance, boolean second) {
        double radians = Math.toRadians(yaw);
        double offset = (second ? 0.5 : -0.5) * distance;
        return new StartPoint(x - Math.sin(radians) * offset, floorY + 1,
                z + Math.cos(radians) * offset, second ? yaw + 180 : yaw);
    }

    int volume() { return Math.multiplyExact(width * depth, height + 1); }

    Block block(World world, int index) {
        int layer = index / (width * depth);
        int column = index % (width * depth);
        return world.getBlockAt(minX + column % width, floorY + layer, minZ + column / width);
    }

    boolean wall(int x, int z) {
        return x == minX || x == minX + width - 1 || z == minZ || z == minZ + depth - 1;
    }

    Material generatedMaterial(int x, int y, int z) {
        return y == floorY || wall(x, z) ? Material.STONE : Material.AIR;
    }

    private boolean horizontallyInside(double x, double z) {
        return x >= minX && x < minX + width && z >= minZ && z < minZ + depth;
    }

    boolean contains(Location at, boolean observing) {
        if (at.getWorld() == null || !worldId.equals(at.getWorld().getUID())) return false;
        if (observing) {
            return horizontallyInside(at.getX(), at.getZ()) && at.getY() >= floorY + 1
                    && at.getY() < floorY + height + 17;
        }
        return at.getX() >= minX + 1 && at.getX() < minX + width - 1
                && at.getZ() >= minZ + 1 && at.getZ() < minZ + depth - 1
                && at.getY() >= floorY + 0.5 && at.getY() < floorY + height + 1;
    }

    boolean containsBlock(Block block) {
        return worldId.equals(block.getWorld().getUID()) && horizontallyInside(block.getX(), block.getZ())
                && block.getY() >= floorY && block.getY() <= floorY + height;
    }

    boolean overlaps(Stage other) {
        return worldId.equals(other.worldId) && minX < other.minX + other.width && other.minX < minX + width
                && minZ < other.minZ + other.depth && other.minZ < minZ + depth
                && floorY <= other.floorY + other.height && other.floorY <= floorY + height;
    }

    void validateBounds(World world) {
        if (floorY < world.getMinHeight() || floorY + height >= world.getMaxHeight()) {
            throw new IllegalArgumentException("Arena exceeds the world's height limit.");
        }
        var border = world.getWorldBorder();
        for (double x : new double[]{minX, minX + width}) {
            for (double z : new double[]{minZ, minZ + depth}) {
                if (!border.isInside(new Location(world, x, floorY, z))) {
                    throw new IllegalArgumentException("Arena exceeds the world border.");
                }
            }
        }
    }

    private boolean validSpawn(StartPoint point) {
        return point != null && point.x >= minX + 2 && point.x <= minX + width - 2
                && point.z >= minZ + 2 && point.z <= minZ + depth - 2 && point.y == floorY + 1;
    }

    void validatePoints() {
        if (!validSpawn(a) || !validSpawn(b) || Math.hypot(a.x - b.x, a.z - b.z) < 2) {
            throw new IllegalArgumentException("Set two distinct floor-level spawns at least one block away from the walls.");
        }
        if (spectator == null || !horizontallyInside(spectator.x, spectator.z)
                || spectator.y < floorY + 1 || spectator.y >= floorY + height + 17) {
            throw new IllegalArgumentException("Set a spectator point inside or up to 16 blocks above the arena.");
        }
    }

    void validateSpawns(World world) {
        validatePoints();
        for (StartPoint point : new StartPoint[]{a, b, spectator}) {
            Location at = point.at(world);
            if (at.getY() + 1 >= world.getMaxHeight() || !world.getWorldBorder().isInside(at)) {
                throw new IllegalStateException("An arena spawn is outside the playable world.");
            }
            Block feet = at.getBlock();
            if (!feet.getType().isAir() || !feet.getRelative(0, 1, 0).getType().isAir()
                    || (point != spectator && !fullBlock(feet.getRelative(0, -1, 0)))) {
                throw new IllegalStateException("An arena spawn is obstructed or missing its floor.");
            }
        }
    }

    static boolean fullBlock(Block block) {
        var level = ((org.bukkit.craftbukkit.CraftWorld) block.getWorld()).getHandle();
        var pos = new net.minecraft.core.BlockPos(block.getX(), block.getY(), block.getZ());
        var shape = level.getBlockState(pos).getCollisionShape(level, pos);
        return net.minecraft.world.level.block.Block.isShapeFullBlock(shape);
    }
}
