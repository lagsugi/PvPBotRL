// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import com.extendedclip.deluxemenus.DeluxeMenus;
import com.extendedclip.deluxemenus.menu.Menu;
import com.extendedclip.deluxemenus.menu.MenuHolder;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.Map;
import java.util.logging.Level;
import org.bukkit.Bukkit;
import org.bukkit.configuration.InvalidConfigurationException;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.entity.Player;

final class DeluxeMenuBridge {
    private static final String PREFIX = "pvpbot_";
    private final PvpBotPlugin plugin;
    private final DeluxeMenus deluxe;
    private final Path directory;
    private final Map<String, Menu> installed = new HashMap<>();

    DeluxeMenuBridge(PvpBotPlugin plugin) throws IOException {
        this.plugin = plugin;
        deluxe = (DeluxeMenus) Bukkit.getPluginManager().getPlugin("DeluxeMenus");
        directory = deluxe.getConfiguration().getMenuDirector().toPath();
        Files.createDirectories(directory);
        for (String page : Menus.PAGES) {
            String id = PREFIX + page;
            try {
                export(id);
                menu(id);
            } catch (IOException | RuntimeException e) {
                plugin.getLogger().log(Level.WARNING, "Could not load DeluxeMenus menu " + id, e);
            }
        }
    }

    private void export(String id) throws IOException {
        Path target = directory.resolve(id + ".yml");
        if (Files.exists(target)) return;
        try (var resource = plugin.getResource("deluxemenus/" + id + ".yml")) {
            if (resource == null) throw new IOException("Missing bundled menu " + id);
            Files.copy(resource, target);
        }
    }

    private Menu menu(String id) throws IOException {
        var existing = Menu.getMenuByName(id);
        if (existing.isPresent()) return existing.get();
        export(id);
        var config = new YamlConfiguration();
        Path file = directory.resolve(id + ".yml");
        try {
            config.load(file.toFile());
        } catch (InvalidConfigurationException e) {
            throw new IOException("Invalid DeluxeMenus YAML for " + id, e);
        }
        deluxe.getConfiguration().loadMenu(config, id, false, file.toString());
        Menu loaded = Menu.getMenuByName(id).orElseThrow(() -> new IOException("DeluxeMenus could not load " + id));
        installed.put(id, loaded);
        return loaded;
    }

    void open(Player player, String page, String token) throws IOException {
        menu(PREFIX + page).openMenu(player, Map.of("session", token), null);
    }

    boolean isOurMenu(Player player) {
        return player.getOpenInventory().getTopInventory().getHolder() instanceof MenuHolder holder
                && holder.getMenuName() != null && holder.getMenuName().startsWith(PREFIX);
    }

    void close() {
        installed.forEach((id, menu) -> {
            if (Menu.getMenuByName(id).orElse(null) == menu) Menu.unload(deluxe, id);
        });
        installed.clear();
    }
}
