// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client.mixin;
import dev.sugi.pvpbot.client.ParityCapture;
import dev.sugi.pvpbot.client.ParityTelemetry;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;
@Mixin(Minecraft.class)
public abstract class MinecraftMixin {
    @Inject(method = "tick", at = @At("HEAD"))
    private void parity$input(CallbackInfo ci) { ParityCapture.beforeTick((Minecraft) (Object) this); }

    @Inject(method = "pick", at = @At("RETURN"))
    private void parity$pick(float partialTick, CallbackInfo ci) {
        ParityTelemetry.aim((Minecraft) (Object) this, partialTick, false);
    }

    @Inject(method = "startAttack", at = @At("HEAD"))
    private void parity$attack(CallbackInfoReturnable<Boolean> cir) {
        ParityTelemetry.aim((Minecraft) (Object) this, 1.0f, true);
    }
}
