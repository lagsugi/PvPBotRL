// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.parity;

import java.lang.classfile.ClassFile;
import java.lang.classfile.ClassHierarchyResolver;
import java.lang.classfile.ClassTransform;
import java.lang.classfile.MethodModel;
import java.lang.classfile.instruction.ReturnInstruction;
import java.lang.classfile.instruction.InvokeInstruction;
import java.lang.constant.ClassDesc;
import java.lang.constant.ConstantDescs;
import java.lang.constant.MethodTypeDesc;
import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.security.ProtectionDomain;
import java.util.Map;
import java.util.function.BiConsumer;

/** Loaded only by the isolated runner. Reports completed vanilla handler calls. */
public final class PaperPacketAgent {
    private static final String SERVER_HANDLER = "net/minecraft/server/network/ServerGamePacketListenerImpl";
    private static final String LIVING_ENTITY = "net/minecraft/world/entity/LivingEntity";
    private static final String TRACKED_ENTITY = "net/minecraft/server/level/ChunkMap$TrackedEntity";
    private static final String PACKET = "(Lnet/minecraft/network/protocol/game/Serverbound";
    private static final Map<String, String> TARGETS = Map.of("handleMovePlayer", "MovePlayerPacket",
            "handleAttack", "AttackPacket", "handleClientTickEnd", "ClientTickEndPacket",
            "handlePlayerInput", "PlayerInputPacket", "handlePlayerCommand", "PlayerCommandPacket",
            "handleAnimate", "SwingPacket");
    private static final ClassDesc AGENT = ClassDesc.of(PaperPacketAgent.class.getName());
    private static final MethodTypeDesc CALLBACK = MethodTypeDesc.of(
            ConstantDescs.CD_void, ConstantDescs.CD_Object, ConstantDescs.CD_Object);
    private static volatile BiConsumer<Object, Object> sink;
    private static volatile BiConsumer<Object, Double> randomSink;
    private static volatile BiConsumer<Object, Object> trackerSink;

    public static void premain(String args, Instrumentation instrumentation) {
        instrumentation.addTransformer(new ClassFileTransformer() {
            @Override public byte[] transform(ClassLoader loader, String name, Class<?> type,
                    ProtectionDomain domain, byte[] bytes) {
                boolean packetHandler = SERVER_HANDLER.equals(name);
                if (!packetHandler && !LIVING_ENTITY.equals(name) && !TRACKED_ENTITY.equals(name)) return null;
                var file = ClassFile.of(ClassFile.ClassHierarchyResolverOption.of(ClassHierarchyResolver.ofResourceParsing(loader)));
                var input = file.parse(bytes);
                if (LIVING_ENTITY.equals(name)) return instrumentKnockback(file, input);
                if (TRACKED_ENTITY.equals(name)) {
                    return file.transformClass(input, ClassTransform.transformingMethodBodies(method ->
                            method.methodName().equalsString("sendToTrackingPlayers") &&
                                    method.methodType().equalsString("(Lnet/minecraft/network/protocol/Packet;)V"),
                            (builder, element) -> {
                                if (element instanceof ReturnInstruction) {
                                    builder.aload(0).aload(1).invokestatic(AGENT, "trackedPacket", CALLBACK);
                                }
                                builder.with(element);
                            }));
                }
                if (input.methods().stream().filter(PaperPacketAgent::isTarget).count() != TARGETS.size()) {
                    throw new IllegalStateException("Pinned Paper handler methods changed");
                }
                return file.transformClass(input, ClassTransform.transformingMethodBodies(PaperPacketAgent::isTarget,
                        (builder, element) -> {
                            if (element instanceof ReturnInstruction) {
                                builder.aload(0).aload(1).invokestatic(AGENT, "afterHandler", CALLBACK);
                            }
                            builder.with(element);
                        }));
            }
        });
    }

    private static byte[] instrumentKnockback(ClassFile file, java.lang.classfile.ClassModel input) {
        String descriptor = "(DDDLnet/minecraft/world/entity/Entity;Lio/papermc/paper/event/entity/EntityKnockbackEvent$Cause;)V";
        var callback = MethodTypeDesc.of(ConstantDescs.CD_void, ConstantDescs.CD_double, ConstantDescs.CD_Object);
        return file.transformClass(input, ClassTransform.transformingMethodBodies(method ->
                method.methodName().equalsString("knockback") && method.methodType().equalsString(descriptor),
                (builder, element) -> {
                    builder.with(element);
                    if (element instanceof InvokeInstruction invoke && invoke.name().equalsString("nextDouble")
                            && invoke.owner().asInternalName().equals("net/minecraft/util/RandomSource")) {
                        builder.dup2().aload(0).invokestatic(AGENT, "knockbackRandom", callback);
                    }
                }));
    }

    private static boolean isTarget(MethodModel method) {
        String type = TARGETS.get(method.methodName().stringValue());
        return type != null && method.methodType().equalsString(PACKET + type + ";)V");
    }

    public static void setSink(BiConsumer<Object, Object> callback) { sink = callback; }
    public static void setRandomSink(BiConsumer<Object, Double> callback) { randomSink = callback; }
    public static void setTrackerSink(BiConsumer<Object, Object> callback) { trackerSink = callback; }

    public static void trackedPacket(Object tracker, Object packet) {
        var callback = trackerSink;
        if (callback != null) callback.accept(tracker, packet);
    }

    public static void knockbackRandom(double value, Object entity) {
        var callback = randomSink;
        if (callback != null) callback.accept(entity, value);
    }

    public static void afterHandler(Object listener, Object packet) {
        var callback = sink;
        if (callback != null) callback.accept(listener, packet);
    }

    private PaperPacketAgent() {}
}
