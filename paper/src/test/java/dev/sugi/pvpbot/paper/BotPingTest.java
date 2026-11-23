package dev.sugi.pvpbot.paper;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import org.junit.jupiter.api.Test;

class BotPingTest {
    @Test void zeroPingDeliversOnTheSamplingTick() {
        var ping = new BotPing<String>(0);
        ping.send(BotPing.TICK_NS, "observation");
        assertEquals("observation", ping.poll(BotPing.TICK_NS));
        assertNull(ping.poll(BotPing.TICK_NS));
    }

    @Test void twentyMsSplitsEvenlyAndDeliversOnTheNextServerTick() {
        var ping = new BotPing<String>(BotPing.DEFAULT_RTT_MS);
        assertEquals(10_000_000L, ping.oneWayNs());
        ping.send(0, "first");
        assertNull(ping.poll(0));
        assertNull(ping.poll(19_999_999L));
        ping.send(BotPing.TICK_NS, "second");
        assertEquals("first", ping.poll(BotPing.TICK_NS));
        assertNull(ping.poll(BotPing.TICK_NS));
        assertEquals("second", ping.poll(2 * BotPing.TICK_NS));
    }

    @Test void oddRttPreservesEqualLegsAndQueuesAcrossMultipleTicks() {
        var ping = new BotPing<String>(101);
        assertEquals(50_500_000L, ping.oneWayNs());
        ping.send(0, "first");
        ping.send(BotPing.TICK_NS, "second");
        assertNull(ping.poll(2 * BotPing.TICK_NS));
        assertEquals("first", ping.poll(3 * BotPing.TICK_NS));
        assertNull(ping.poll(3 * BotPing.TICK_NS));
        assertEquals("second", ping.poll(4 * BotPing.TICK_NS));
    }
}
