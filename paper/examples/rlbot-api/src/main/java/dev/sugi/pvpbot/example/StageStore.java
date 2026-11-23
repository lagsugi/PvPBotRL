// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.example;

import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Base64;
import java.util.List;
import java.util.TreeMap;
import java.util.UUID;
import java.util.logging.Level;
import java.util.zip.GZIPInputStream;
import java.util.zip.GZIPOutputStream;
import net.minecraft.core.BlockPos;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.bukkit.Bukkit;
import org.bukkit.GameRules;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.command.CommandSender;
import org.bukkit.craftbukkit.CraftWorld;
import org.bukkit.plugin.java.JavaPlugin;

/** Persist intent before edits. Replay from the beginning after an interrupted world save. */
final class StageStore {
    static final String PRACTICE = "practice";
    enum Phase { DRAFT, SNAPSHOT, GENERATE, READY, DISABLED, RESTORE }

    record Point(UUID world, Stage.StartPoint position) {
        static Point of(Location at) { return new Point(at.getWorld().getUID(), Stage.StartPoint.from(at)); }
    }

    record Draft(Point pos1, Point pos2, Point a, Point b, Point spectator) {
        Draft set(String field, Point point) {
            return switch (field) {
                case "pos1" -> new Draft(point, pos2, a, b, spectator);
                case "pos2" -> new Draft(pos1, point, a, b, spectator);
                case "a" -> new Draft(pos1, pos2, point, b, spectator);
                case "b" -> new Draft(pos1, pos2, a, point, spectator);
                case "spectator" -> new Draft(pos1, pos2, a, b, point);
                default -> throw new IllegalArgumentException("Unknown arena point.");
            };
        }

        Stage build() {
            if (pos1 == null || pos2 == null || a == null || b == null || spectator == null) {
                throw new IllegalArgumentException("Set both corners, both spawns, and the spectator point.");
            }
            World world = Bukkit.getWorld(pos1.world);
            if (world == null) throw new IllegalStateException("The arena world is not loaded.");
            for (Point point : List.of(pos2, a, b, spectator)) {
                if (!pos1.world.equals(point.world)) throw new IllegalArgumentException("All points must share one world.");
            }
            Location first = pos1.position.at(world);
            Location second = pos2.position.at(world);
            int minX = Math.min(first.getBlockX(), second.getBlockX());
            int minZ = Math.min(first.getBlockZ(), second.getBlockZ());
            int floor = Math.min(first.getBlockY(), second.getBlockY());
            int width = Math.abs(first.getBlockX() - second.getBlockX()) + 1;
            int depth = Math.abs(first.getBlockZ() - second.getBlockZ()) + 1;
            int height = Math.abs(first.getBlockY() - second.getBlockY());
            Stage.dimensions(width, depth);
            if (height < Stage.CLEAR_HEIGHT) throw new IllegalArgumentException("Walls must be at least eight blocks tall.");
            return new Stage(world.getUID(), world.getName(), minX, floor, minZ, width, depth, height,
                    a.position, b.position, spectator.position);
        }
    }

    record Journal(String id, String name, boolean generated, Stage stage, Draft draft, Phase phase) {
        Journal withPhase(Phase next) { return new Journal(id, name, generated, stage, draft, next); }
    }

    private record SavedBlock(String blockData, String blockEntity) {}
    private record Validation(Stage stage, boolean enable, CommandSender sender) {}

    private final JavaPlugin plugin;
    private final Path directory;
    private final TreeMap<String, Entry> entries = new TreeMap<>();
    private String previousBatch = "";

    StageStore(JavaPlugin plugin) throws IOException {
        this.plugin = plugin;
        directory = plugin.getDataFolder().toPath().resolve("arenas");
        Files.createDirectories(directory);
        try (var files = Files.newDirectoryStream(directory, "arena-*.json")) {
            for (Path file : files) {
                Journal journal = StateFiles.JSON.fromJson(Files.readString(file), Journal.class);
                if (journal == null || journal.phase == null || !validId(journal.id) || PRACTICE.equals(journal.id)
                        || !file.equals(journalFile(journal.id))
                        || (journal.stage == null && journal.phase != Phase.DRAFT)) {
                    throw new IOException("Invalid arena journal " + file);
                }
                entries.put(journal.id, new Entry(journal));
            }
        }
    }

    void addPractice(Stage stage) {
        checkOverlap(PRACTICE, stage);
        entries.put(PRACTICE, new Entry(new Journal(PRACTICE, "Practice", false, stage, null, Phase.READY)));
    }

    private Path journalFile(String id) { return directory.resolve("arena-" + id + ".json"); }
    private Path backupFile(String id) { return directory.resolve("arena-" + id + "-backup.json.gz"); }
    static boolean validId(String id) { return id != null && id.matches("[a-z0-9_-]{1,32}"); }
    List<String> ids() { return List.copyOf(entries.keySet()); }

    Entry entry(String id) {
        Entry entry = entries.get(id);
        if (entry == null) throw new IllegalArgumentException("Arena not found: " + id);
        return entry;
    }

    private void add(Journal journal) throws IOException {
        if (!validId(journal.id)) throw new IllegalArgumentException("Use 1–32 lowercase letters, digits, hyphens or underscores.");
        if (entries.containsKey(journal.id) || PRACTICE.equals(journal.id)) {
            throw new IllegalArgumentException("That arena ID is already in use.");
        }
        checkOverlap(journal.id, journal.stage);
        StateFiles.writeJson(journalFile(journal.id), journal);
        entries.put(journal.id, new Entry(journal));
    }

    void create(String id, Location origin, double distance, int width, int depth, CommandSender sender) throws IOException {
        Stage stage = Stage.generated(origin, distance, width, depth);
        add(new Journal(id, id, true, stage, null, Phase.SNAPSHOT));
        entry(id).requester = sender;
        sender.sendMessage("Saving the original blocks and building arena " + id + ".");
    }

    void register(String id) throws IOException {
        add(new Journal(id, id, false, null, new Draft(null, null, null, null, null), Phase.DRAFT));
    }

    private void checkOverlap(String id, Stage stage) {
        if (stage == null) return;
        for (Entry other : entries.values()) {
            if (other.id().equals(id)) continue;
            if (other.stage() != null && stage.overlaps(other.stage())
                    || other.validation != null && stage.overlaps(other.validation.stage)) {
                throw new IllegalArgumentException("Arena overlaps " + other.id() + ".");
            }
        }
    }

    void setPoint(String id, String field, Location at) throws IOException {
        Entry entry = entry(id);
        entry.requireNotValidating();
        Journal old = entry.journal;
        if (old.generated || old.phase == Phase.READY) {
            throw new IllegalArgumentException("Disable a custom arena before editing its points.");
        }
        Draft draft = old.draft.set(field, Point.of(at));
        entry.save(new Journal(id, old.name, false, old.stage, draft, Phase.DRAFT));
    }

    void validate(String id, boolean enable, CommandSender sender) {
        Entry entry = entry(id);
        entry.requireNotValidating();
        Journal old = entry.journal;
        if (entry.failed || entry.working()) {
            throw new IllegalStateException("Finish arena generation before validation.");
        }
        Stage stage = old.generated ? old.stage : old.draft.build();
        checkOverlap(id, stage);
        World world = stage.world();
        if (world == null) throw new IllegalStateException("The arena world is not loaded.");
        stage.validateBounds(world);
        stage.validateSpawns(world);
        if (!pvpEnabled(world)) throw new IllegalStateException("PvP is disabled in the arena world.");
        entry.validation = new Validation(stage, enable, sender);
        entry.cursor = 0;
        sender.sendMessage("Validating arena " + id + ".");
    }

    private static boolean pvpEnabled(World world) {
        return Boolean.TRUE.equals(world.getGameRuleValue(GameRules.PVP));
    }

    void disable(String id) throws IOException {
        Entry entry = entry(id);
        entry.requireNotValidating();
        if (entry.journal.phase != Phase.READY) throw new IllegalStateException("The arena is not enabled.");
        entry.save(entry.journal.withPhase(Phase.DISABLED));
    }

    void remove(String id, CommandSender sender) throws IOException {
        Entry entry = entry(id);
        entry.requireNotValidating();
        if (entry.working() && !entry.failed && entry.journal.phase != Phase.SNAPSHOT) {
            throw new IllegalStateException("Wait for the current arena operation to complete.");
        }
        if (!entry.journal.generated || entry.journal.phase == Phase.SNAPSHOT) {
            delete(entry);
            sender.sendMessage("Removed arena " + id + ".");
            return;
        }
        entry.readBackup();
        entry.save(entry.journal.withPhase(Phase.RESTORE));
        entry.cursor = 0;
        entry.failed = false;
        entry.requester = sender;
        sender.sendMessage("Restoring the original blocks at arena " + id + ".");
    }

    private void delete(Entry entry) throws IOException {
        Files.delete(journalFile(entry.id()));
        entries.remove(entry.id());
        Files.deleteIfExists(backupFile(entry.id()));
    }

    boolean protects(Block block, boolean admin) {
        for (Entry entry : entries.values()) {
            if (entry.validation != null && entry.validation.stage.containsBlock(block)) return true;
            if (entry.stage() == null || !entry.stage().containsBlock(block)) continue;
            if (entry.journal.generated || entry.journal.phase == Phase.READY || !admin) return true;
        }
        return false;
    }

    void tick(int budget) {
        if (entries.isEmpty()) return;
        var ordered = new ArrayList<>(entries.tailMap(previousBatch, false).values());
        ordered.addAll(entries.headMap(previousBatch, true).values());
        for (Entry entry : ordered) {
            if (entry.validation != null && entry.validation.stage.world() != null) {
                previousBatch = entry.id();
                entry.validateBatch(budget);
                return;
            }
            if (!entry.working() || entry.failed || entry.stage().world() == null) continue;
            previousBatch = entry.id();
            entry.tick(budget);
            return;
        }
    }

    final class Entry {
        private Journal journal;
        private List<SavedBlock> backup;
        private int cursor;
        private CommandSender requester;
        private boolean failed;
        private Validation validation;

        Entry(Journal journal) { this.journal = journal; }
        String id() { return journal.id; }
        String name() { return journal.name; }
        Stage stage() { return journal.stage; }
        boolean working() { return journal.phase == Phase.SNAPSHOT || journal.phase == Phase.GENERATE || journal.phase == Phase.RESTORE; }
        private void requireNotValidating() {
            if (validation != null) throw new IllegalStateException("Wait for arena validation to finish.");
        }
        boolean ready() {
            World world = stage() == null ? null : stage().world();
            return !failed && validation == null && journal.phase == Phase.READY && world != null && pvpEnabled(world);
        }
        String status() {
            if (validation != null) return "VALIDATING " + cursor + "/" + validation.stage.volume();
            if (failed) return journal.phase + " (error)";
            if (stage() != null && stage().world() == null) return journal.phase + " (world not loaded)";
            if (journal.phase == Phase.READY && !ready()) return "READY (PvP disabled)";
            return working() ? journal.phase + " " + cursor + "/" + stage().volume() : journal.phase.toString();
        }
        String kind() { return PRACTICE.equals(id()) ? "Built-in" : journal.generated ? "Generated" : "Custom"; }

        private void save(Journal next) throws IOException {
            StateFiles.writeJson(journalFile(id()), next);
            journal = next;
        }

        private void readBackup() throws IOException {
            try (var reader = new InputStreamReader(new GZIPInputStream(Files.newInputStream(backupFile(id()))), StandardCharsets.UTF_8)) {
                SavedBlock[] saved = StateFiles.JSON.fromJson(reader, SavedBlock[].class);
                if (saved == null || saved.length != stage().volume()) throw new IOException("Invalid arena backup size");
                backup = List.of(saved);
            }
        }

        private void validateBatch(int budget) {
            Validation check = validation;
            Stage stage = check.stage;
            World world = stage.world();
            try {
                int end = Math.min(stage.volume(), cursor + budget);
                for (; cursor < end; cursor++) {
                    Block block = stage.block(world, cursor);
                    boolean solid = block.getY() == stage.floorY() || stage.wall(block.getX(), block.getZ());
                    if (solid ? !Stage.fullBlock(block) : !block.getType().isAir()) {
                        throw new IllegalArgumentException("Expected " + (solid ? "a full solid block" : "air")
                                + " at " + block.getX() + ", " + block.getY() + ", " + block.getZ() + ".");
                    }
                }
                if (cursor < stage.volume()) return;
                stage.validateBounds(world);
                stage.validateSpawns(world);
                if (!pvpEnabled(world)) throw new IllegalArgumentException("PvP is disabled in the arena world.");
                if (check.enable) save(new Journal(id(), journal.name, journal.generated, stage, journal.draft, Phase.READY));
                check.sender.sendMessage("Arena " + id() + (check.enable ? " enabled." : " passed validation."));
            } catch (IllegalArgumentException | IllegalStateException e) {
                check.sender.sendMessage("Arena " + id() + " failed validation. " + e.getMessage());
            } catch (IOException | RuntimeException e) {
                plugin.getLogger().log(Level.SEVERE, "Arena validation failed for " + id(), e);
                check.sender.sendMessage("Could not validate arena " + id() + ". Check the server log.");
            }
            validation = null;
            cursor = 0;
        }

        private void tick(int budget) {
            Stage stage = stage();
            World world = stage.world();
            try {
                if (backup == null && journal.phase == Phase.SNAPSHOT) backup = new ArrayList<>(stage.volume());
                if (backup == null && journal.phase == Phase.RESTORE) readBackup();
                int end = Math.min(stage.volume(), cursor + budget);
                for (; cursor < end; cursor++) {
                    Block block = stage.block(world, cursor);
                    switch (journal.phase) {
                        case SNAPSHOT -> backup.add(capture(block));
                        case GENERATE -> block.setType(stage.generatedMaterial(block.getX(), block.getY(), block.getZ()), false);
                        case RESTORE -> restore(block, backup.get(cursor));
                        default -> throw new IllegalStateException("Unexpected arena phase");
                    }
                }
                if (cursor == stage.volume()) complete(world);
            } catch (IOException | RuntimeException e) {
                failed = true;
                plugin.getLogger().log(Level.SEVERE, "Arena operation failed for " + id() + ". Recovery data retained.", e);
                notifyRequester("Arena operation failed for " + id() + ". Check the server log.");
            }
        }

        private void complete(World world) throws IOException {
            switch (journal.phase) {
                case SNAPSHOT -> {
                    StateFiles.replace(backupFile(id()), temporary -> {
                        try (var writer = new OutputStreamWriter(new GZIPOutputStream(Files.newOutputStream(temporary)), StandardCharsets.UTF_8)) {
                            StateFiles.JSON.toJson(backup, writer);
                        }
                    });
                    save(journal.withPhase(Phase.GENERATE));
                }
                case GENERATE -> {
                    ((CraftWorld) world).getHandle().save(null, true, false);
                    save(journal.withPhase(Phase.READY));
                    notifyRequester("Arena " + id() + " is ready.");
                }
                case RESTORE -> {
                    ((CraftWorld) world).getHandle().save(null, true, false);
                    delete(this);
                    notifyRequester("Restored the original blocks at arena " + id() + ".");
                }
                default -> throw new IllegalStateException("Unexpected arena phase");
            }
            backup = null;
            cursor = 0;
        }

        private void notifyRequester(String message) {
            if (requester != null) requester.sendMessage(message);
            requester = null;
        }
    }

    private static SavedBlock capture(Block block) throws IOException {
        var level = ((CraftWorld) block.getWorld()).getHandle();
        var tile = level.getBlockEntity(new BlockPos(block.getX(), block.getY(), block.getZ()));
        String encoded = null;
        if (tile != null) {
            var buffer = new java.io.ByteArrayOutputStream();
            NbtIo.writeCompressed(tile.saveWithFullMetadata(level.registryAccess()), buffer);
            encoded = Base64.getEncoder().encodeToString(buffer.toByteArray());
        }
        return new SavedBlock(block.getBlockData().getAsString(), encoded);
    }

    private static void restore(Block block, SavedBlock original) throws IOException {
        block.setBlockData(Bukkit.createBlockData(original.blockData), false);
        if (original.blockEntity == null) return;
        var level = ((CraftWorld) block.getWorld()).getHandle();
        var pos = new BlockPos(block.getX(), block.getY(), block.getZ());
        var bytes = new java.io.ByteArrayInputStream(Base64.getDecoder().decode(original.blockEntity));
        var tag = NbtIo.readCompressed(bytes, NbtAccounter.unlimitedHeap());
        var entity = BlockEntity.loadStatic(pos, level.getBlockState(pos), tag, level.registryAccess());
        if (entity == null) throw new IOException("Could not restore block entity at " + pos);
        level.setBlockEntity(entity);
        entity.setChanged();
        level.sendBlockUpdated(pos, entity.getBlockState(), entity.getBlockState(), 2);
    }
}
