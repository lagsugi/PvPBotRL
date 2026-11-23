// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import java.util.ArrayDeque;

/** Fixed symmetric link between the Paper tick and an observation-only policy. */
final class BotPing<T> {
    static final int DEFAULT_RTT_MS = 20;
    static final int MAX_RTT_MS = 10_000;
    static final long TICK_NS = 50_000_000L;

    private final long oneWayNs;
    private final ArrayDeque<Pending<T>> pending = new ArrayDeque<>();

    BotPing(int rttMs) {
        if (rttMs < 0 || rttMs > MAX_RTT_MS) throw new IllegalArgumentException("Invalid bot RTT: " + rttMs);
        oneWayNs = rttMs * 500_000L;
    }

    long oneWayNs() { return oneWayNs; }

    void send(long sampledAtNs, T snapshot) {
        // The policy only depends on the snapshot and its own decision history.
        // Evaluate at delivery, using the logical receipt time in the snapshot,
        // so both legs share one deadline without rounding each leg to a tick.
        pending.addLast(new Pending<>(sampledAtNs + 2 * oneWayNs, snapshot));
    }

    T poll(long nowNs) {
        Pending<T> next = pending.peekFirst();
        return next != null && next.readyAtNs <= nowNs ? pending.removeFirst().snapshot : null;
    }

    private record Pending<T>(long readyAtNs, T snapshot) {}
}
