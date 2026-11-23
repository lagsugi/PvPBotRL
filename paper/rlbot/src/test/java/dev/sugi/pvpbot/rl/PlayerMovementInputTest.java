package dev.sugi.pvpbot.rl;

import static org.junit.jupiter.api.Assertions.assertEquals;
import org.junit.jupiter.api.Test;

class PlayerMovementInputTest {
    @Test void diagonalMatchesMeasuredClientFirstTickAcceleration() {
        var input = PlayerMovementInput.shape(1, 1, false, 0.3f);
        // Paper 26.1.2 build 74 + actual 26.1.2 LocalPlayer, motion-04/diagonal_0.
        float acceleration = (float) ((double) 0.1f * (1.0 + (double) 0.3f))
                * (0.21600002f / (0.6f * 0.6f * 0.6f));
        assertEquals(0.5919238793974007, 0.5 + (double) input.sideways() * acceleration, 1e-12);
        assertEquals(input.sideways(), input.forward());
    }

    @Test void cardinalAndCrouchingKeepFloatInputShape() {
        assertEquals(0.98f, PlayerMovementInput.shape(0, 1, false, 0.3f).forward());
        var diagonal = PlayerMovementInput.shape(1, 1, true, 0.3f);
        assertEquals(0.4157788, Math.hypot(diagonal.sideways(), diagonal.forward()), 1e-7);
        assertEquals(new PlayerMovementInput.Axes(0, 0), PlayerMovementInput.shape(0, 0, true, 0.3f));
    }
}
