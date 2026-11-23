// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client.mixin;

import dev.sugi.pvpbot.client.ParityTelemetry;
import net.minecraft.client.player.RemotePlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(RemotePlayer.class)
public abstract class RemotePlayerMixin {
    @Shadow private int lerpDeltaMovementSteps;

    @Inject(method = "tick", at = @At("RETURN"))
    private void parity$afterTick(CallbackInfo ci) {
        ParityTelemetry.remoteTick((RemotePlayer) (Object) this, lerpDeltaMovementSteps);
    }
}
