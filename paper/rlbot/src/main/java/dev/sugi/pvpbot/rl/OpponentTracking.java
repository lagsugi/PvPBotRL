// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.rl;

import java.lang.reflect.Field;
import java.util.IdentityHashMap;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.Predicate;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientGamePacketListener;
import net.minecraft.server.level.ServerEntity;
import net.minecraft.server.level.ServerPlayer;

/** Copies the pinned tracker's outbound updates without changing its player watchers. */
final class OpponentTracking implements AutoCloseable {
    private final ServerPlayer observer;
    private ServerEntity entity;
    private Feed feed;
    private OpponentReplica replica;

    OpponentTracking(ServerPlayer observer) { this.observer = observer; }

    OpponentReplica.Frame observe(ServerPlayer target, long delayNs, Consumer<Object> trace) {
        var tracker = target.level().getChunkSource().chunkMap.entityMap.get(target.getId());
        if (tracker == null) {
            close();
            return null;
        }
        if (entity != tracker.serverEntity) {
            close();
            var next = new OpponentReplica(target, delayNs, trace);
            tracker.serverEntity.sendPairingData(observer, next::receive);
            feed = Feed.attach(tracker.serverEntity, observer, next);
            entity = tracker.serverEntity;
            replica = next;
        }
        replica.tick();
        return replica.frame();
    }

    @Override public void close() {
        if (feed == null) return;
        feed.detach(entity, observer);
        entity = null;
        feed = null;
        replica = null;
    }

    private static final class Feed implements ServerEntity.Synchronizer {
        private static final Field SYNCHRONIZER = synchronizerField();
        private final ServerEntity.Synchronizer original;
        private final Map<ServerPlayer, OpponentReplica> observers = new IdentityHashMap<>();

        private Feed(ServerEntity.Synchronizer original) { this.original = original; }

        static Feed attach(ServerEntity entity, ServerPlayer observer, OpponentReplica replica) {
            try {
                var current = (ServerEntity.Synchronizer) SYNCHRONIZER.get(entity);
                Feed feed;
                if (current instanceof Feed existing) {
                    feed = existing;
                } else {
                    feed = new Feed(current);
                    SYNCHRONIZER.set(entity, feed);
                }
                feed.observers.put(observer, replica);
                return feed;
            } catch (IllegalAccessException e) {
                throw new IllegalStateException("Could not subscribe to the pinned Paper tracker", e);
            }
        }

        void detach(ServerEntity entity, ServerPlayer observer) {
            observers.remove(observer);
            if (!observers.isEmpty()) return;
            try {
                if (SYNCHRONIZER.get(entity) == this) SYNCHRONIZER.set(entity, original);
            } catch (IllegalAccessException e) {
                throw new IllegalStateException("Could not restore the pinned Paper tracker", e);
            }
        }

        private static Field synchronizerField() {
            try {
                var field = ServerEntity.class.getDeclaredField("synchronizer");
                field.setAccessible(true);
                return field;
            } catch (ReflectiveOperationException e) {
                throw new ExceptionInInitializerError(e);
            }
        }

        @Override public void sendToTrackingPlayers(Packet<? super ClientGamePacketListener> packet) {
            original.sendToTrackingPlayers(packet);
            for (var replica : observers.values()) replica.receive(packet);
        }

        @Override public void sendToTrackingPlayersAndSelf(Packet<? super ClientGamePacketListener> packet) {
            original.sendToTrackingPlayersAndSelf(packet);
            for (var replica : observers.values()) replica.receive(packet);
        }

        @Override public void sendToTrackingPlayersFiltered(Packet<? super ClientGamePacketListener> packet,
                                                             Predicate<ServerPlayer> filter) {
            original.sendToTrackingPlayersFiltered(packet, filter);
            observers.forEach((observer, replica) -> {
                if (filter.test(observer)) replica.receive(packet);
            });
        }
    }
}
