// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.client.mixin;

import dev.sugi.pvpbot.client.ParityTelemetry;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.network.protocol.game.ClientboundEntityPositionSyncPacket;
import net.minecraft.network.protocol.game.ClientboundMoveEntityPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityMotionPacket;
import net.minecraft.network.protocol.game.ClientboundSetHealthPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ClientPacketListener.class)
public abstract class ClientPacketListenerMixin {
    @Inject(method = "handleUpdateAttributes", at = @At("RETURN"))
    private void parity$attributes(ClientboundUpdateAttributesPacket packet, CallbackInfo ci) {
        ParityTelemetry.attributesApplied(packet);
    }
    @Inject(method = "handleSetEntityMotion", at = @At("RETURN"))
    private void parity$motion(ClientboundSetEntityMotionPacket packet, CallbackInfo ci) {
        ParityTelemetry.motionApplied(packet);
    }

    @Inject(method = "handleMoveEntity", at = @At("RETURN"))
    private void parity$relative(ClientboundMoveEntityPacket packet, CallbackInfo ci) {
        ParityTelemetry.relativeApplied(packet);
    }

    @Inject(method = "handleEntityPositionSync", at = @At("RETURN"))
    private void parity$sync(ClientboundEntityPositionSyncPacket packet, CallbackInfo ci) {
        ParityTelemetry.positionSyncApplied(packet);
    }

    @Inject(method = "handleSetEntityData", at = @At("RETURN"))
    private void parity$metadata(ClientboundSetEntityDataPacket packet, CallbackInfo ci) {
        ParityTelemetry.metadataApplied(packet);
    }

    @Inject(method = "handleSetHealth", at = @At("RETURN"))
    private void parity$health(ClientboundSetHealthPacket packet, CallbackInfo ci) {
        ParityTelemetry.healthApplied(packet);
    }
}
