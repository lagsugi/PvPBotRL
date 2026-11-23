// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

package dev.sugi.pvpbot.paper;

import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.numbers.BlankFormat;
import net.minecraft.network.chat.numbers.FixedFormat;
import net.minecraft.network.protocol.game.ClientboundSetDisplayObjectivePacket;
import net.minecraft.network.protocol.game.ClientboundSetObjectivePacket;
import net.minecraft.network.protocol.game.ClientboundSetScorePacket;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.scores.DisplaySlot;
import net.minecraft.world.scores.Objective;
import net.minecraft.world.scores.Scoreboard;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import org.bukkit.craftbukkit.entity.CraftPlayer;
import org.bukkit.craftbukkit.scoreboard.CraftScoreboard;
import org.bukkit.entity.Player;

/** Sends a match's below-name objective only to its fighters and spectators. */
final class BotHealthDisplay {
    private record Viewer(Objective belowName) {}

    private final Duel match;
    private final Objective objective;
    private final Map<Player, Viewer> viewers = new HashMap<>();
    private final int[] lastSent = {-1, -1};

    BotHealthDisplay(Duel match) {
        this.match = match;
        // This objective lives only on the recipients' clients. Do not replace
        // their Bukkit scoreboard or disturb its teams and other display slots.
        objective = new Objective(new Scoreboard(), "pvpbot-" + match.id,
                ObjectiveCriteria.DUMMY, Component.empty(), ObjectiveCriteria.RenderType.INTEGER,
                false, BlankFormat.INSTANCE);
    }

    void show(Player player) {
        if (viewers.containsKey(player)) return;
        viewers.put(player, new Viewer(currentBelowName(player)));
        var connection = connection(player);
        connection.send(new ClientboundSetObjectivePacket(objective, ClientboundSetObjectivePacket.METHOD_ADD));
        for (Duel.Fighter fighter : match.fighters) {
            if (fighter.avatar != null) connection.send(score(fighter, displayedHp(fighter)));
        }
        display(connection, objective);
    }

    void update() {
        if (viewers.isEmpty()) return;
        for (int side = 0; side < match.fighters.length; side++) {
            Duel.Fighter fighter = match.fighters[side];
            if (fighter.avatar == null) continue;
            int hp = displayedHp(fighter);
            if (hp == lastSent[side]) continue;
            lastSent[side] = hp;
            var packet = score(fighter, hp);
            for (Player player : viewers.keySet()) {
                if (player.isOnline()) connection(player).send(packet);
            }
        }
        viewers.replaceAll((player, viewer) -> {
            if (!player.isOnline()) return viewer;
            Objective current = currentBelowName(player);
            if (current == viewer.belowName) return viewer;
            display(connection(player), objective);
            return new Viewer(current);
        });
    }

    void hide(Player player) {
        if (viewers.remove(player) == null || !player.isOnline()) return;
        var connection = connection(player);
        connection.send(new ClientboundSetObjectivePacket(objective, ClientboundSetObjectivePacket.METHOD_REMOVE));
        display(connection, currentBelowName(player));
    }

    void close() {
        for (Player player : List.copyOf(viewers.keySet())) hide(player);
    }

    private static int displayedHp(Duel.Fighter fighter) {
        return (int) Math.ceil(fighter.hp());
    }

    private ClientboundSetScorePacket score(Duel.Fighter fighter, int hp) {
        String holder = ((CraftPlayer) fighter.entity).getHandle().getScoreboardName();
        return new ClientboundSetScorePacket(holder, objective.getName(), hp, Optional.empty(),
                Optional.of(new FixedFormat(Component.literal(hp + " HP"))));
    }

    private static Objective currentBelowName(Player player) {
        return ((CraftScoreboard) player.getScoreboard()).getHandle().getDisplayObjective(DisplaySlot.BELOW_NAME);
    }

    private static void display(ServerGamePacketListenerImpl connection, Objective objective) {
        connection.send(new ClientboundSetDisplayObjectivePacket(DisplaySlot.BELOW_NAME, objective));
    }

    private static ServerGamePacketListenerImpl connection(Player player) {
        return ((CraftPlayer) player).getHandle().connection;
    }
}
