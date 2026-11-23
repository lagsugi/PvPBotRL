// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import java.util.ArrayDeque;

/** Tick-polled queue for one leg of the bot link or the legacy diagnostic round trip. */
final class BotPing<T> {
    static final int DEFAULT_RTT_MS = 20;
    static final int MAX_RTT_MS = 10_000;
    static final long TICK_NS = 50_000_000L;

    private final long oneWayNs;
    private final long delayNs;
    private final ArrayDeque<Pending<T>> pending = new ArrayDeque<>();

    BotPing(int rttMs) {
        this(rttMs, false);
    }

    static <T> BotPing<T> oneWay(int rttMs) { return new BotPing<>(rttMs, true); }

    private BotPing(int rttMs, boolean singleLeg) {
        if (rttMs < 0 || rttMs > MAX_RTT_MS) throw new IllegalArgumentException("Invalid bot RTT: " + rttMs);
        oneWayNs = rttMs * 500_000L;
        delayNs = singleLeg ? oneWayNs : 2 * oneWayNs;
    }

    long oneWayNs() { return oneWayNs; }

    void send(long sampledAtNs, T snapshot) {
        pending.addLast(new Pending<>(sampledAtNs + delayNs, snapshot));
    }

    T poll(long nowNs) {
        Pending<T> next = pending.peekFirst();
        return next != null && next.readyAtNs <= nowNs ? pending.removeFirst().snapshot : null;
    }

    void clear() { pending.clear(); }

    private record Pending<T>(long readyAtNs, T snapshot) {}
}
