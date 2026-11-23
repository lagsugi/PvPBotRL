# Paper PvPBot — Production Plugin Implementation Plan

Implementation notes for the legacy PvPBot. For the current Paper server, see the [PvPBotRL setup and verification guide](RLBOT_SETUP.md).

Updated November 13, 2026 (Asia/Tokyo)

Status: implemented. The plugin's user interface is in English.

## 1. Objectives and decisions

Allow the Paper plugin to host independent 1v1 matches concurrently on multiple stages. Support bot vs. bot, human vs. bot, and human vs. human, with spectating in all modes.

- Provide commands for all primary operations and equivalent controls in a DeluxeMenus inventory GUI.
- Use DeluxeMenus for the GUI without a separate PvPBot inventory implementation.
- Use PlaceholderAPI to supply dynamic match, stage, and HP information to DeluxeMenus.
- Use English for menus, item names, descriptions, command help, notices, errors, countdowns, results, HP displays, and operator messages. Ship English as the sole display language.
- Show current HP below each bot's name. Show both players' current and maximum HP in spectator view and match details.
- Support generated and manually registered stages with a flat floor and perimeter walls. Remove out-of-bounds losses and interruptions.
- Support invitations and a waiting queue for human matches. Ordinary players may host bot matches.
- Scope covers duels, spectating, operations, and recovery. Team matches, free-for-alls, ratings, cumulative rankings, and terrain pathfinding are outside it.

## 2. Change scope and experiment isolation

- Confine changes to `paper/` and the installation/launch support needed in `tools/paper/`.
- Preserve `crates/`, `tools/research/`, research documents, training outputs, weights, experiment settings and environments, and any existing changes.
- Preserve existing Paper policies, inference, and comparison data. Change the surrounding code that invokes separate policy instances per bot.
- Read bundled weights at build time only. Importing or replacing next-generation models is outside this work.
- Leave Rust/Python builds and training running independently. Use one Gradle worker for Java checks and defer load testing until the main experiment ends.
- Retain Paper 26.1.2 build 74, Citizens build 4269, and JDK 25. Pin tested distributions of DeluxeMenus and PlaceholderAPI and record download sources and SHA-256 in setup. Compatibility claims apply only to tested versions.

## 3. Match management

### Structure and state

- Replace the single match field with management indexed by match ID, stage ID, entity UUID, participant UUID, and host UUID.
- Use two slots, A and B, each accepting a human or bot. Remove fixed human/bot field assignments.
- Use match states `PREPARING → COUNTDOWN → RUNNING → FINISHING → CLOSED`. Make finalization idempotent.
- Allow one active match per stage and at most one hosted or played match per person in total. Make the overall concurrent limit configurable, default eight.
- Reject conflicting queued, pending-invitation, or participating states. When a match starts, clear obsolete invitations and queue entries involving that player.
- Modify Bukkit, Citizens, and match state on the server main thread. Validate and commit stage reservation and participant registration together.

### Bot independence

- Give each bot its own policy, observation history, delay queue, and NPC, including mirror matches.
- For bot matches, collect both observations before applying inputs. Each opponent must occupy the opposite slot in the same match.
- Display actual server HP on the UI. Keep that value outside the policy observation path and preserve existing observation, delay, and inference behavior.

### Rules and starting matches

- Retain the standard loadout, durability setting, three-second countdown, default five-minute limit, and default bot ping.
- Death decides the match. Both deaths in one tick or a timeout produce a draw. Leaving or disconnecting after start forfeits, while leaving before start aborts.
- Operator stop, NPC loss, internal failure, and server shutdown abort the affected match. Isolate exceptions per match.
- Invitations expire after 60 seconds. Reserve an available stage on acceptance. If the requested stage is unavailable, return the reason and leave the match unstarted.
- Use a FIFO queue with automatic stage selection. When a stage opens, pair the first two eligible players. Preserve position and inventory while queued.
- When stage ID is omitted, choose the first enabled free stage in ID order. An explicit ID restricts selection to that stage.
- The host of a bot match enters spectator mode after start. Leaving spectator mode keeps the match running and the host slot occupied until completion or stop.
- Treat a human rematch as a new invitation requiring the opponent's acceptance.

## 4. Spectating, interference prevention, and recovery

- Save state before moving a spectator into spectator mode. Support free camera and either player's point of view.
- Prevent active participants from joining another match or spectating. Allow spectators to switch matches while retaining their original restoration data.
- Restore spectators when the match or spectating ends. If a camera target dies or disappears, return to free camera before cleanup.
- Bound free spectating to the stage and the spectator space up to 16 blocks above it. Prevent travel outside that area and ordinary spectator teleportation to another match.
- Allow attacks only against the opponent in the same match. Prevent interference during countdown and from third parties, other matches, and spectators.
- Protect registered stages against building, breaking, explosions, liquids, pistons, and other area changes. Administrators may edit a manual stage after disabling it and clearing users.
- Persist player state before changing it and retain it until restoration and player-data save succeed. Restore after respawn on death or on next login after disconnect.
- On partial start failure, clean up created NPCs, HP displays, and logs, restore modified players, and release stage and participant slots.
- Persist minimum information about unfinished matches. After restart, record them as aborted and restore pending players.

## 5. Multiple stages

- Save ID, display name, generated/manual type, world UUID, cuboid bounds, floor elevation, A/B spawns, spectator location, enabled status, and processing status per stage.
- IDs use lowercase ASCII letters, digits, hyphens, and underscores. Reject overlapping registered stages.
- Default generated exterior is 64×64 with eight blocks above the floor. Build a one-block-thick perimeter wall eight blocks high and clear the interior above the floor to air.
- Allow width and depth from 16 to 128 blocks. Keep starts at least one block from walls and reject a stage that cannot fit the configured starting separation.
- Manual registration specifies two corners, A/B spawns, and spectator location. Use the lowest layer as the floor. Before enabling, verify at least eight blocks of headroom, flat unbroken floor, clear combat area, and continuous perimeter walls.
- Before every start, recheck safe spawn positions, loaded world, world border, and PvP enablement.
- Remove out-of-bounds defeat and abort checks. Return an escaped participant to the last safe position or their start if unavailable, resetting velocity and fall distance.
- Back up blocks and block entities before generating stages and restore on removal. Removing a manual stage only unregisters it.
- Journal saving, generation, and restoration per stage. Apply a global per-tick work limit and process stages in batches in turn.
- Reject modification, disabling, and removal of occupied stages. Other stage matches can continue.
- Migrate the legacy single stage to `default`. Preserve the existing restoration backup and keep the stage unavailable until explicit wall addition via `stage upgrade default` completes. Make migration idempotent.

## 6. Commands and DeluxeMenus

### Public commands

| Operation | Command |
| --- | --- |
| Menu | `/pvpbot` / `/pvpbot menu` |
| Help | `/pvpbot help` |
| List bots | `/pvpbot list` |
| Human vs. bot | `/pvpbot start <bot> [stage]` |
| Bot vs. bot | `/pvpbot bots <botA> <botB> [stage]` |
| Invite | `/pvpbot challenge <player> [stage]` |
| Accept or decline | `/pvpbot accept <player>` / `/pvpbot deny <player>` |
| Waiting queue | `/pvpbot queue join` / `/pvpbot queue leave` |
| List matches | `/pvpbot matches` |
| Status | `/pvpbot status [matchId]` |
| Spectate | `/pvpbot spectate <matchId>` |
| Spectator camera | `/pvpbot view <free\|a\|b>` |
| Leave match or spectator mode | `/pvpbot leave` |
| Stop own match | `/pvpbot stop` |
| Rematch | `/pvpbot rematch` |
| Administrator stop | `/pvpbot stop <matchId>` |
| Stage list and details | `/pvpbot stage list` / `/pvpbot stage info <id>` |
| Generate stage | `/pvpbot stage create <id> [width] [depth]` |
| Register manual stage | `/pvpbot stage register <id>` |
| Set bounds | `/pvpbot stage pos1 <id>` / `/pvpbot stage pos2 <id>` |
| Set spawn | `/pvpbot stage spawn <id> <a\|b>` |
| Set spectator location | `/pvpbot stage spectator <id>` |
| Validate, enable, disable | `/pvpbot stage <validate\|enable\|disable> <id>` |
| Add walls to legacy stage | `/pvpbot stage upgrade <id>` |
| Remove or unregister | `/pvpbot stage remove <id>` |
| Reload configuration | `/pvpbot reload` |

Preserve existing `start <bot>`, `list`, `status`, `stop`, and `rematch`. Legacy stage commands select the only stage if exactly one exists. Otherwise prompt for an ID. For ordinary participants, `stop` forfeits after start. For bot-match hosts and administrators, `stop` aborts.

### GUI structure and integration

- Bundle English menus `Main Menu`, `Fight a Bot`, `Bot vs Bot`, `Challenge a Player`, `Invitations`, `Matchmaking`, `Select Arena`, `Live Matches`, `Match Details`, `Spectator Controls`, and `Arena Management`.
- Paginate stages, bots, players, and matches. Use PvPBot chat input for names, sizes, and similar values with cancellation and a 60-second timeout. Commands accept these values directly.
- Menu clicks invoke commands that call the PvPBot operations service under the player's own permissions. Keep ordinary actions at player privilege.
- Revalidate permissions, participation, target existence, and stage availability when executing GUI actions. DeluxeMenus visibility conditions only control presentation.
- Give dynamic lists a per-viewer page snapshot with stable target IDs. Keep rows fixed while open and report a failed click if the target disappears. Show additions and reorderings on refresh or reopen.
- Use `pvpbot` as the PlaceholderAPI expansion ID. Expose participation, match ID, stage, A/B names, HP/max HP, elapsed time, spectator count, and pagination. Placeholder evaluation performs no state mutation or disk I/O.
- The GUI requires DeluxeMenus and PlaceholderAPI. Declare both as soft dependencies and keep commands operational without them. Report missing menu dependencies in English.
- Ship menu YAML and installation steps. Register PvPBot-specific menu IDs and preserve existing DeluxeMenus configuration and user-edited menus.
- Make menu definitions, icons, and English wording editable in YAML. Check for existing menu-command conflicts during installation.

## 7. HP display

- Show `15 HP` below each bot's name. Round fractions up only for display.
- Use Minecraft `below_name` so the client renders the line with the bot. In 26.1.2 the HP line is visible within strictly less than ten blocks of the camera.
- Assign separate score names even to identical bot types. Update scores only when displayed HP changes. Send to participants and spectators of that match and restore the prior `below_name` on exit or match end.
- Every five ticks show `A: <name> <hp>/<max> HP | B: <name> <hp>/<max> HP` in the spectator action bar, both in free view and player views.
- Display both players' HP/max HP in DeluxeMenus match details, refreshed every second through PlaceholderAPI. Show final values and result for a finished match.
- Also show action-bar and match-detail HP for human matches. Preserve existing overhead displays for humans.
- Clamp displayed HP to [0, max HP] and show zero on death. Keep display formatting independent of actual health, damage, and bot inputs.

## 8. Operations, records, and compatibility

- Separate `pvpbot.play`, `pvpbot.spectate`, `pvpbot.host`, and `pvpbot.admin` permissions. Grant the first three by default and default the administrator permission to op.
- Apply configuration atomically after successful validation. Running matches retain their start settings and new matches use updated settings. Lowering the limit leaves ongoing matches running.
- Extend results to schema version 2 with A/B participants. Record mode, host, participant UUID or bot ID, stage, winner, end reason, elapsed ticks, remaining HP, damage dealt, and each bot's model hash, policy version, and ping.
- Add participant side to each detailed log row to distinguish bot-vs-bot inputs. Preserve older logs and document format differences in the README.
- Save restoration data and progress journals by replacement from a temporary file. On failure retain previous data and log the cause and affected ID in English.
- Document plugin installation, menu registration, permissions, commands, configuration, legacy stage migration, backups, recovery, and supported versions in the README.

## 9. Verification and acceptance

### Automated verification

- Test state transitions for all three match modes, stage/participant exclusivity, global limit, partial-start rollback, and duplicate finalization.
- Test invitation expiry, simultaneous acceptance, disconnects, FIFO cancellation and waiting for a stage, and renewed rematch acceptance.
- Verify independent policy state and delay queues even for identical bots. Retain model and policy parity tests.
- Test stage bounds, walls, spawns, overlap rejection, legacy migration, and resume from generation/restoration journals.
- Test fixed GUI targets, stale clicks, missing permission, missing dependencies, and invalid input.
- Test HP formatting, changed maximum HP, death, spectator switching, display cleanup after match end, and PlaceholderAPI output.

### Live Paper verification

- Run bot vs. bot, human vs. bot, and human vs. human simultaneously on three stages, each with spectators.
- Verify one match ending, disconnecting, stopping, or losing an NPC leaves other matches intact.
- Verify wall collisions and absence of out-of-bounds losses or interruptions.
- Confirm overhead HP for players and spectators, including both bots in bot matches. Show actual HP in the action bar and menus too.
- Switch among free view and both player views, move between matches as a spectator, and restore state at match end.
- Run equivalent operations by command and DeluxeMenus. Prevent removal of menu items.
- Check that all menus and PvPBot user-facing text are in English.
- After death, disconnect, restart, or start failure, restore inventory, position, game mode, and other state. Leave no NPC, HP display, or stage reservation behind.
- On generated stage removal, restore containers and contents placed before generation. Preserve structures when unregistering a manual stage.
- Identify items still unverified on live Paper in the implementation report. Run load tests separately after the main experiment.

## 10. Implementation order

1. Implement multiple-stage definition, persistence, migration, and wall generation.
2. Introduce match management and A/B participants, then implement three modes, invitations, queue, and shared commands.
3. Add spectating, interference protection, player recovery, and finalization.
4. Add overhead HP, spectator action bar, and the PlaceholderAPI read interface.
5. Implement English DeluxeMenus, pagination, input assistance, and installation.
6. Run automated checks and lightweight live verification, then finish English wording and documentation.

## 11. Post-implementation verification

- Built with JDK 25 and one Gradle worker. All 14 automated tests passed, including existing policy and delay comparisons.
- Started Paper 26.1.2 build 74, Citizens build 4269, DeluxeMenus 1.14.1-Release, and PlaceholderAPI 2.12.3 on an isolated local server. Test clients verified all three concurrent modes, rejection of cross-match damage, queueing, spectator switching, overhead HP for two bots, HP changes, and menu updates.
- Switched pages in DeluxeMenus and started a human-vs-bot match. Verified disconnect after start, inventory restoration on reconnect, restoration after death, and spectator restoration.
- Verified generated arena walls and clear interior, restoration of a container and its contents on removal, validation of a manual arena, and unregistration with its structure intact.
- Verified commands without DeluxeMenus or PlaceholderAPI, plus legacy arena migration, restart, wall addition, and restoration from backup.
- After switching to `below_name`, isolated-server traffic confirmed separate profile-name scores for two bots of the same type, HP updates, spectator recipient scope, and restoration of existing score displays after exit.
- Visual client inspection, load testing, and exhaustive failure reproduction were still outstanding.

## References

- [Official DeluxeMenus documentation](https://wiki.helpch.at/helpchat-plugins/deluxemenus)
- [DeluxeMenus Actions](https://wiki.helpch.at/helpchat-plugins/deluxemenus/options-and-configurations/actions)
- [DeluxeMenus GUI configuration](https://wiki.helpch.at/helpchat-plugins/deluxemenus/options-and-configurations/gui)
