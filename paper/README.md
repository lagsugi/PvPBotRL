# Legacy PvPBot for Paper

This page documents the legacy PvPBot server and its `/pvpbot` commands. The current `tools/paper/start.bat` and `tools/paper/setup.bat` manage PvPBotRL in a separate run directory. See the [current setup guide](RLBOT_SETUP.md).

PvPBot runs independent 1v1 matches in multiple arenas. Players can fight a bot, invite another player, join the matchmaking queue, or host two bots. Spectators can fly freely or follow either fighter. The client renders current HP as an integer (for example, 15 HP) below each bot's name using Minecraft's `below_name` scoreboard slot. On Minecraft 26.1.2 this HP line appears within 10 blocks of the camera. The spectator action bar and the DeluxeMenus match details show both fighters' HP at any distance.

After a match, fighters and spectators see the winner's name with a short sound. Bot names include the same identifier shown on their in-game name tags so identical styles can be told apart. A fighter who dies sees the result on respawn.

## Restore the legacy server

The archived runtime uses **Paper 26.1.2 build 74**, **Citizens 2.0.44 build 4269**, and **JDK 25**. The menu integration uses **DeluxeMenus 1.14.1-Release** and **PlaceholderAPI 2.12.3**.

Stop any server using the legacy server's port. In the archived legacy server directory, verify that the world and the PvPBot plugin data are present. Launch its Paper JAR from that directory with JDK 25. Connect with Minecraft Java Edition 26.1.2 and run `op <player>` in the server console to grant arena management. Use `stop` in the server console for a clean shutdown.

The legacy Gradle project is `paper/` and its plugin artifact is `paper/build/libs/pvpbot-paper-0.1.0.jar`. The command interface requires Citizens. Menus and their live values also require DeluxeMenus and PlaceholderAPI.

## Arenas

Stand at the desired arena center, one block above the future floor, and face toward fighter B's starting point. Run `/pvpbot stage create east`. The default arena is 64 × 64 with a stone floor, eight blocks of air inside, and eight-block-high walls around the perimeter. You can supply width and depth in blocks (16–128). The configured starting distance must fit between the walls. Create other arenas at non-overlapping locations. Stage operations continue in batches until each arena is ready.

To use a custom build:

1. Run `/pvpbot stage register <id>`.
2. Stand in the block at one corner of the floor and set `stage pos1 <id>`. Set `stage pos2 <id>` at the diagonally opposite corner at the **top of the wall**. Both selected blocks count toward the bounds.
3. Stand at floor level inside the arena and set `stage spawn <id> a` and `stage spawn <id> b`, facing the desired directions. Leave a full block of clearance between the starting point and the wall. Set `stage spectator <id>` inside the arena or up to 16 blocks above it.
4. Use `stage enable <id>` to validate and enable the arena. The floor and outer walls need solid blocks. Keep the interior clear and enable PvP in the world.

Disable a custom arena before editing it. In-use arenas cannot be edited, disabled, upgraded, or removed. Removing a custom arena unregisters it. Removing a generated arena restores the saved blocks and block entities, including container data. Do not store items or build inside a generated arena that you plan to remove. Match participants who cross the arena boundary are returned to their last safe position. Arena boundaries do not decide the result.

On first launch with a previous single-arena installation, PvPBot migrates its journal to arena `default` and keeps its original backup. Run `/pvpbot stage upgrade default` to add walls and make a legacy arena available. An interrupted arena snapshot, generation, or restoration resumes from the beginning of its current phase at startup.

## Play and spectate

Open `/pvpbot` for the English DeluxeMenus interface. Choose a mode, then pick an arena or choose automatically. Live Matches opens Match Details, which refreshes both fighters' HP once per second and shows the final result when the match ends. You can switch to another live match without overwriting your original player state.

Players in the queue are matched in order when an arena is free. Invitations expire after 60 seconds. A human rematch sends a new invitation. Bot vs Bot hosts automatically enter spectator mode. They may leave spectating while the match continues, and they keep the host slot until it ends.

Commands below follow `/pvpbot`. Each match has a UUID, available from `matches` or `status`. An omitted arena is picked in ID order.

| Command | Action |
| --- | --- |
| `menu`, `help`, `list` | Open the menu, show help, or list bot styles. |
| `start <bot> [arena]` | Fight a bot. |
| `bots <botA> <botB> [arena]` | Host two bots. |
| `challenge <player> [arena]` | Invite an online player to a 1v1. |
| `accept <player>`, `deny <player>` | Answer an invitation. |
| `queue join`, `queue leave` | Enter or leave matchmaking. |
| `matches`, `status [matchId]` | List live matches or show a match's status. |
| `spectate <matchId>`, `view <free\|a\|b>` | Watch a match or switch the camera. |
| `leave` | Leave spectating or forfeit your match after the countdown. |
| `stop [matchId]` | Stop your match. A specific match ID requires admin permission. |
| `rematch`, `recover` | Play a new match against the previous opponent or retry pending player recovery. |
| `stage list`, `stage info [id]` | List arenas or inspect an arena. |
| `stage create <id> [width] [depth]` | Generate a walled arena. A single dimension creates a square. |
| `stage register <id>` | Register a custom arena draft. |
| `stage <pos1\|pos2\|spectator> <id>`, `stage spawn <id> <a\|b>` | Set arena points from your current position. |
| `stage <validate\|enable\|disable\|upgrade\|remove> <id>` | Manage the selected arena. |
| `reload` | Validate and reload PvPBot settings. |

`pvpbot.play` controls participation, invitations, and matchmaking. `pvpbot.host` and `pvpbot.spectate` control hosting and spectating respectively. Hosting also requires spectate permission. These three permissions default to allowed. `pvpbot.admin` defaults to operators and grants access to arena management, reload, and administrative stops.

## Edit the DeluxeMenus menus

On first enable, PvPBot copies its eleven `pvpbot_*.yml` files to the DeluxeMenus GUI menu directory (usually `plugins/DeluxeMenus/gui_menus`). It registers them with DeluxeMenus at startup and again on demand after a DeluxeMenus reload. Existing files and menu registrations are preserved. To use custom filenames, add the corresponding IDs under `gui_menus` in the DeluxeMenus config with the desired `file` entries. The supplied menus reserve no separate opening commands.

Edit the English labels, materials, and layout in those files. Keep the `session` argument and the `[player] pvpbot:pvpbot ui {session} ...` actions. The session identifies the page snapshot so a click cannot silently target a different match. Reload DeluxeMenus after editing. Opening a page, clicking Refresh, or changing pages updates which targets are listed.

DeluxeMenus disables its NBT item options on this Paper build. The supplied menus use standard items, lore, and PlaceholderAPI values.

PlaceholderAPI uses the `pvpbot` identifier. Examples:

- `%pvpbot_state%`, `%pvpbot_queued%`, `%pvpbot_camera%`, `%pvpbot_queue_size%`, `%pvpbot_live_count%`, and `%pvpbot_invite_count%`.
- `%pvpbot_match_id%`, `%pvpbot_match_arena%`, `%pvpbot_match_seconds%`, `%pvpbot_match_spectators%`, and `%pvpbot_match_status%`.
- `%pvpbot_match_a_name%`, `%pvpbot_match_a_hp%`, and `%pvpbot_match_a_max_hp%`. Use `b` for fighter B or replace `match` with `detail` for the selected match, including its final HP and `%pvpbot_detail_result%`.
- `%pvpbot_page%`, `%pvpbot_pages%`, and `%pvpbot_row_0_id%`, `%pvpbot_row_0_name%`, `%pvpbot_row_0_detail%`, `%pvpbot_row_0_present%` (rows 0–6).

## Settings and recovery

The legacy server stores plugin data in `plugins/PvPBot/` inside its run directory. Active matches retain their original settings when you reload.

| Key | Default | Effect |
| --- | --- | --- |
| `match.max-concurrent` | 8 | Global match limit. Each arena holds one match. |
| `match.max-ticks` | 6000 | Match duration after the countdown (5 minutes at 20 TPS). |
| `match.start-distance` | 20.0 | Separation between the fighters in newly generated arenas. |
| `match.bot-ping-ms` | 20 | Simulated bot round-trip latency (0–10000 ms). |
| `match.durability` | true | Whether issued equipment wears down. |
| `stage.blocks-per-tick` | 512 | Shared batch budget for arena operations. |
| `logging.detailed` | false | Record bot observations and actions in NDJSON. |

Back up the plugin data directory and world together while the server is stopped. The per-arena JSON journal and compressed original-block snapshot are under `arenas/`. Keep both files until the terrain has been restored. If an operation errors, check the server log for the arena ID. Restart to replay the operation or run `stage remove <id>` to restore from a valid backup.

The `recovery/` directory holds original player state while they fight or watch. It is restored when they leave, on respawn, or on their next login. After fixing an unavailable original world or a blocked teleport, use `/pvpbot recover` to retry without reconnecting. Keep the recovery file until PvPBot confirms restoration.

`matches/` stores schema 2 results with A/B fighter objects, mode, arena, winner, outcome and reason. Each bot fighter records its model hash, policy version, and configured ping. Detailed traces add a `side` field to each action. Schema 1 results remain readable as their original files. A restart converts unfinished match journals to aborted results and restores players as they become available.
