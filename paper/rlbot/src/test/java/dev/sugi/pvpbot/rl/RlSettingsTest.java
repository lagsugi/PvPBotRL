package dev.sugi.pvpbot.rl;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.bukkit.configuration.file.YamlConfiguration;
import org.junit.jupiter.api.Test;

class RlSettingsTest {
    @Test void readsStandaloneDefaults() {
        var settings = RlSettings.read(config());
        assertEquals(20, settings.rttMs());
        assertEquals(8, settings.matchLimit());
        assertFalse(settings.diagnostics());
        assertFalse(settings.fixedEquipment());
    }

    @Test void readsFixedEquipmentAndDiagnostics() {
        var config = config();
        config.set("equipment-profile", RlSettings.FIXED_EQUIPMENT);
        config.set("diagnostics", true);
        var settings = RlSettings.read(config);
        assertTrue(settings.fixedEquipment());
        assertTrue(settings.diagnostics());
    }

    @Test void rejectsInvalidSettings() {
        var config = config();
        config.set("bot-rtt-ms", BotPing.MAX_RTT_MS + 1);
        assertThrows(IllegalArgumentException.class, () -> RlSettings.read(config));
        config.set("bot-rtt-ms", 20);
        config.set("max-active-matches", 0);
        assertThrows(IllegalArgumentException.class, () -> RlSettings.read(config));
        config.set("max-active-matches", 8);
        config.set("equipment-profile", "unknown");
        assertThrows(IllegalArgumentException.class, () -> RlSettings.read(config));
    }

    @Test void reloadLeavesPreviousSettingsSnapshotIntact() {
        var config = config();
        var original = RlSettings.read(config);
        config.set("bot-rtt-ms", 100);
        config.set("equipment-profile", RlSettings.FIXED_EQUIPMENT);
        var reloaded = RlSettings.read(config);
        assertEquals(20, original.rttMs());
        assertFalse(original.fixedEquipment());
        assertEquals(100, reloaded.rttMs());
        assertTrue(reloaded.fixedEquipment());
    }

    private static YamlConfiguration config() {
        var config = new YamlConfiguration();
        config.set("bot-rtt-ms", 20);
        config.set("max-active-matches", 8);
        return config;
    }
}
