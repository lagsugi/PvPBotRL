package dev.sugi.pvpbot.rl;

import static org.junit.jupiter.api.Assertions.*;
import org.junit.jupiter.api.Test;

class ParityMotionInboxTest {
    @Test void exactDeadlinesAndTickRoundingRemainDistinct() {
        for (long ms : new long[]{0, 1, 20, 49, 50, 51, 99, 100, 101, 120, 150, 300}) {
            var inbox = new ParityMotionInbox<String>(ms * 1_000_000);
            var sent = inbox.send(0, "velocity");
            assertEquals(ms * 1_000_000, sent.readyNs());
            if (ms > 0) assertNull(inbox.poll(sent.readyNs() - 1));
            assertEquals(sent, inbox.poll(sent.readyNs()));
            assertNull(inbox.poll(sent.readyNs()));
            var tickOnly = new ParityMotionInbox<String>(ms * 1_000_000);
            tickOnly.send(0, "velocity");
            long firstTick = ((ms + 49) / 50) * 50_000_000;
            for (long at = 0; at < firstTick; at += 50_000_000) assertNull(tickOnly.poll(at));
            assertEquals("velocity", tickOnly.poll(firstTick).value());
        }
    }

    @Test void sameDeadlineRetainsAllPacketsInSendOrder() {
        var inbox = new ParityMotionInbox<Integer>(10);
        var a = inbox.send(0, 1);
        var b = inbox.send(0, 2);
        assertEquals(a, inbox.poll(10));
        assertEquals(b, inbox.poll(10));
        assertNull(inbox.poll(10));
        assertEquals(0, a.sequence());
        assertEquals(1, b.sequence());
    }

    @Test void invalidDelayAndBackwardClockFailExplicitly() {
        assertThrows(IllegalArgumentException.class, () -> new ParityMotionInbox<>(-1));
        var inbox = new ParityMotionInbox<Integer>(0);
        inbox.send(100, 1);
        assertThrows(IllegalArgumentException.class, () -> inbox.poll(99));
        assertThrows(IllegalArgumentException.class, () -> inbox.send(99, 2));
    }
}
