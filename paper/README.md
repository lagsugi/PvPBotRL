# PvPBotRL Example server

Install the [Example plugin](RLBOT_SETUP.md) to run sword duels on Paper. It uses the [PvPBotRL Java API](RLBOT_API.md) to create and control bots. On the first startup it prepares an 80 × 80 arena named `practice` in its own world. You can play there immediately or add arenas in other loaded worlds.

## Play and spectate

These commands use the `/rlbotexample` prefix and support tab completion.

| Command | Action |
| --- | --- |
| `start [model] [arena]` | Fight a bot. The default model is `recurrent-ppo`. |
| `list` | List available models. |
| `bots <modelA> <modelB> [arena]` | Start a Bot vs Bot match and watch it. |
| `challenge <player> [arena]` | Invite an online player to a duel. The invitation expires after 60 seconds. |
| `accept <player>`, `deny <player>` | Respond to an invitation. |
| `queue join`, `queue leave` | Enter or leave the first-in, first-out Player vs Player queue. |
| `matches`, `status [matchId]` | List live matches, inspect a match, or look up a recent result. |
| `spectate <matchId>`, `view <free\|a\|b>` | Watch a match and fly within the arena or follow either fighter. |
| `leave`, `stop [matchId]` | Leave spectating, forfeit as a fighter, or stop a hosted match. Stopping a match by ID requires admin permission. |
| `rematch` | Start a new round with the previous bot models, including resolved random weights, or send a new invitation to the previous human opponent. |
| `recover` | Retry a pending player restoration. |
| `pause [matchId]`, `resume [matchId]` | Pause or resume a started round. Requires admin permission. |

When the arena is omitted, the server selects a ready, free arena in ID order. Each arena holds one match. Spectators can switch matches and return to their original location and state when they leave.

Run `/rlbotexample start g3-prototype` to fight G3 Prototype, or `/rlbotexample bots g3-prototype recurrent-g2-plus` to watch it fight G2+.

All three modes use an unenchanted diamond sword and full Protection IV diamond armor. A three-second countdown precedes 6,000 active server ticks. One death or forfeit during the match gives the opponent the win. Two deaths in the same tick or time expiry end in a draw. A boss bar shows the countdown or time remaining. Both fighters' HP appears below their names for nearby fighters and spectators. The result is announced in chat and as a title with sound, or after respawn for a fighter who died.

The command requires `pvpbotrlexample.demo`. Hosting also uses `pvpbotrlexample.host` and `pvpbotrlexample.spectate`. These three permissions default to allowed. `pvpbotrlexample.admin` defaults to operators and allows arena edits, reload, pause, resume and administrative stops.

## Manage arenas

- Run `/rlbotexample stage create <id> [width] [depth]` while standing one block above the future floor and facing toward seat B. The default is 64 × 64 with an eight-block wall. Dimensions must be 16–128 blocks. Generation saves the original blocks and block entities before making edits. Removing a generated arena restores its snapshot, including container contents.
- For an existing build, run `stage register <id>`, then set `stage pos1 <id>` at one corner of the floor and `stage pos2 <id>` at the opposite corner at the top of the wall. Set `stage spawn <id> a`, `stage spawn <id> b`, and `stage spectator <id>` from their intended positions. Use `stage enable <id>` to validate and open it for play. Use solid full blocks for the floor and walls, and keep the interior clear.
- Use `stage list` and `stage info <id>` to inspect arenas. Disable a custom arena before editing it. Arena operations run in batches, and in-use arenas cannot be modified. Removing a registered custom arena unregisters it. The built-in `practice` arena is managed by the plugin.

PvP must be enabled and natural regeneration must be off in the arena world. The built-in arena sets these rules when it loads. The Example supplies the fixed Sword PvP kit in every match. [Equipment setup](RLBOT_SETUP.md#fixed-equipment-profile)

## Settings and recovery

`plugins/PvPBotRLExample/config.yml` sets `match.max-concurrent` (default 8), `match.start-distance` (default 20 blocks for generated arenas), and `stage.blocks-per-tick` (default 512). Run `/rlbotexample reload` to apply settings to subsequent matches. `PvPBotRL` separately limits the number of bot handles. Bot vs Bot uses two handles.

Player state is saved before a fighter or spectator is moved. The Example restores it on exit, respawn, or the next login. If recovery fails, make the original destination available and use `recover`. Keep the `recovery/` directory and the per-arena journals and backups under `arenas/`. Back up the Example data and the associated worlds together with the server stopped.

`matches/` stores JSON results with the arena, fighters, winner, reason and final HP. Bot records include the resolved model ID, weights hash when applicable, policy version and RTT. A restart records unfinished matches as aborted. Interrupted arena snapshot, generation and restoration operations resume from the start of the current phase.

## Moving from the retired PvPBot plugin

With the legacy server and its original plugin still installed, finish player recovery and remove any generated arenas whose original terrain you want restored. Stop the server and back up its worlds and `plugins/PvPBot` together. Keep those files for recovery and historical results. Install the new Example with PvPBotRL and register any retained custom arenas through the new commands. The Example uses its own `plugins/PvPBotRLExample` directory.
