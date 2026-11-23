// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client.mixin;
import dev.sugi.pvpbot.client.ParityCapture;
import dev.sugi.pvpbot.client.ParityTelemetry;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.network.protocol.Packet;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
@Mixin(LocalPlayer.class)
public abstract class LocalPlayerMixin {
    @Inject(method = "tick", at = @At("HEAD"))
    private void parity$before(CallbackInfo ci) {
        ParityTelemetry.beforeTick((LocalPlayer) (Object) this);
    }
    @Inject(method = "tick", at = @At("RETURN"))
    private void parity$after(CallbackInfo ci) {
        ParityCapture.afterPlayerTick((LocalPlayer) (Object) this);
        ParityTelemetry.afterTick((LocalPlayer) (Object) this);
    }

    @Redirect(method = "sendPosition", at = @At(value = "INVOKE", target =
            "Lnet/minecraft/client/multiplayer/ClientPacketListener;send(Lnet/minecraft/network/protocol/Packet;)V"),
            require = 4)
    private void parity$movementSend(ClientPacketListener connection, Packet<?> packet) {
        ParityTelemetry.movementSubmitted((LocalPlayer) (Object) this, packet);
        connection.send(packet);
    }
}
