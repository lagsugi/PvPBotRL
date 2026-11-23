// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import java.io.IOException;
import java.nio.file.Path;
import java.util.UUID;
import org.bukkit.Bukkit;

final class MotionTrace {
    private static final int MAX_EVENTS = 4096;
    private final Path path;
    private final StringBuilder events = new StringBuilder("server_tick\tevent\tdetail\n");
    private int count;
    private long dropped;

    MotionTrace(Path directory, UUID id) {
        path = directory.resolve(id + "-" + UUID.randomUUID() + ".tsv");
    }

    void record(String event, String detail) {
        if (++count > MAX_EVENTS && !event.equals("remove")) { dropped++; return; }
        events.append(Bukkit.getCurrentTick()).append('\t').append(event).append('\t').append(detail).append('\n');
    }

    void save() throws IOException {
        String summary = Bukkit.getCurrentTick() + "\ttrace-summary\tdropped=" + dropped + "\n";
        StateFiles.replace(path, temporary -> java.nio.file.Files.writeString(temporary, events.toString() + summary));
    }
}
