package dev.sugi.pvpbot.example;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;

import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class MatchLogTest {
    @TempDir Path logs;

    private MatchLog.Result active(String id) {
        var fighter = new MatchLog.Fighter("baseline", null, "baseline", null, "test", 20, 20, 20, 0);
        return new MatchLog.Result(2, id, "2026-01-01T00:00:00Z", null, "Bot vs Bot", "host",
                "alpha", fighter, fighter, null, null, null, 0);
    }

    @Test void restartAbortsAnUnfinishedMatchAndDoesNotRewriteTheResultOnReplay() throws Exception {
        var active = active("interrupted");
        new MatchLog(logs, active);
        MatchLog.recover(logs);

        Path file = logs.resolve("interrupted.json");
        String contents = Files.readString(file);
        var recovered = StateFiles.JSON.fromJson(contents, MatchLog.Result.class);
        assertEquals(2, recovered.schema_version());
        assertEquals("aborted", recovered.outcome());
        assertEquals("server_restart", recovered.reason());
        assertEquals(active.a(), recovered.a());

        // Simulate a crash after writing the result but before deleting the active journal.
        StateFiles.writeJson(logs.resolve("interrupted.active.json"), active);
        MatchLog.recover(logs);
        assertEquals(contents, Files.readString(file));
        assertFalse(Files.exists(logs.resolve("interrupted.active.json")));
    }
}
