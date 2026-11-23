// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import java.util.ArrayDeque;

/** Logical downlink deadlines; advancing physics must never move a deadline earlier. */
final class ParityMotionInbox<T> {
    record Delivery<T>(long sequence, long sentNs, long readyNs, T value) {}
    private final long delayNs;
    private final ArrayDeque<Delivery<T>> pending = new ArrayDeque<>();
    private long sequence;
    private long lastNs = Long.MIN_VALUE;

    ParityMotionInbox(long delayNs) {
        if (delayNs < 0 || delayNs > 10_000_000_000L) throw new IllegalArgumentException("Invalid downlink delay");
        this.delayNs = delayNs;
    }

    long delayNs() { return delayNs; }

    Delivery<T> send(long nowNs, T value) {
        checkClock(nowNs);
        var delivery = new Delivery<>(sequence++, nowNs, Math.addExact(nowNs, delayNs), value);
        pending.addLast(delivery);
        return delivery;
    }

    Delivery<T> poll(long nowNs) {
        checkClock(nowNs);
        var next = pending.peekFirst();
        return next != null && next.readyNs() <= nowNs ? pending.removeFirst() : null;
    }

    private void checkClock(long nowNs) {
        if (nowNs < lastNs) throw new IllegalArgumentException("Clock moved backwards");
        lastNs = nowNs;
    }
}
