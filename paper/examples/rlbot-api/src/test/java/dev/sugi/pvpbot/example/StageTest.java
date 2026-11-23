package dev.sugi.pvpbot.example;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.UUID;
import org.bukkit.Material;
import org.junit.jupiter.api.Test;

class StageTest {
    private final UUID world = UUID.randomUUID();

    private Stage arena(int x, int y, int z) {
        return new Stage(world, "world", x, y, z, 32, 48, 8,
                new Stage.StartPoint(x + 16, y + 1, z + 14, 0),
                new Stage.StartPoint(x + 16, y + 1, z + 34, 180),
                new Stage.StartPoint(x + 16, y + 6, z + 24, 0));
    }

    @Test void rectangularArenaHasSolidCornersAndAnEmptyInterior() {
        var stage = arena(-32, 20, 0);
        assertEquals(32 * 48 * 9, stage.volume());
        assertEquals(Material.STONE, stage.generatedMaterial(-32, 28, 47));
        assertEquals(Material.STONE, stage.generatedMaterial(-1, 28, 0));
        assertEquals(Material.STONE, stage.generatedMaterial(-31, 20, 46));
        assertEquals(Material.AIR, stage.generatedMaterial(-31, 28, 46));
    }

    @Test void overlapIncludesSharedWallAndFloorBlocks() {
        var stage = arena(-32, 20, 0);
        assertTrue(stage.overlaps(arena(-1, 20, 0)));
        assertTrue(stage.overlaps(arena(-32, 28, 0)));
        assertFalse(stage.overlaps(arena(0, 20, 0)));
        assertFalse(stage.overlaps(arena(-32, 29, 0)));
    }

    @Test void startingPointsMustFitInsideWalls() {
        var stage = arena(0, 20, 0);
        stage.validatePoints();
        var wallAdjacent = new Stage(world, "world", 0, 20, 0, 32, 48, 8,
                new Stage.StartPoint(1.5, 21, 14, 0), stage.b(), stage.spectator());
        assertThrows(IllegalArgumentException.class, wallAdjacent::validatePoints);
    }

    @Test void nonCardinalYawPlacesSpawnsOnOppositeSidesAtTheSameHeight() {
        var human = Stage.startPoint(-4.25, 70, 8.75, 37, 20, false);
        var bot = Stage.startPoint(-4.25, 70, 8.75, 37, 20, true);
        assertEquals(-4.25, (human.x() + bot.x()) / 2, 1e-9);
        assertEquals(8.75, (human.z() + bot.z()) / 2, 1e-9);
        assertEquals(71, human.y());
        assertEquals(human.y(), bot.y());
        assertEquals(20, Math.hypot(human.x() - bot.x(), human.z() - bot.z()), 1e-9);
        assertEquals(180, bot.yaw() - human.yaw());
    }

    @Test void minecraftYawZeroPointsTowardPositiveZ() {
        var human = Stage.startPoint(3.5, 60, 10.5, 0, 20, false);
        var bot = Stage.startPoint(3.5, 60, 10.5, 0, 20, true);
        assertEquals(0.5, human.z());
        assertEquals(20.5, bot.z());
    }
}
