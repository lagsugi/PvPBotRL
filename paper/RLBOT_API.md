# PvPBotRL Java API quickstart

PvPBotRL supplies the NPC and controls it in sword rounds. Your Paper plugin prepares the fighters and arena, starts and pauses combat, determines the result, and restores players. [Server setup](RLBOT_SETUP.md)

## Install and build

Use Paper 26.1.2 build 74, Citizens 2.0.44 build 4270, and JDK 25. Install Citizens and `pvpbot-rl-0.1.0.jar` in the server's `plugins/` directory.

Run these commands from the repository root in PowerShell to build the runtime, API JAR, and example.

```powershell
.\tools\paper\setup.bat -BuildOnly
$apiJar = (Resolve-Path 'paper/rlbot/build/libs/pvpbot-rl-0.1.0-api.jar').Path
if (Test-Path '.gradle-user-home' -PathType Container) {
    $env:GRADLE_USER_HOME = (Resolve-Path '.gradle-user-home').Path
}
.\paper\gradlew.bat -p paper/examples/rlbot-api "-PpvpBotApiJar=$apiJar" build
```

To use the API in your own Gradle project, add `compileOnly files('libs/pvpbot-rl-0.1.0-api.jar')` alongside your Paper API dependency and declare `depend: [PvPBotRL]` in `plugin.yml`. Use the API JAR at compile time and install the runtime JAR on the server. The runtime supplies the classes loaded by `ServicesManager`.

## Run a round

Look up the service on the server thread when your plugin enables.

```java
PvPBotApi api = getServer().getServicesManager().load(PvPBotApi.class);
if (api == null) {
    throw new IllegalStateException("PvPBotRL API is unavailable. Check the startup log.");
}
```

| Step | API and host responsibilities |
| --- | --- |
| Choose a model | `api.models()` returns the available IDs. `recurrent-ppo` is bundled. `random` appears when the training catalog has eligible models. |
| Prepare the round | Call `api.spawn(this, spawn, modelId)` on the server thread. Use a full, flat floor. Equip `bot.player()` and the human with an unenchanted diamond sword in slot 0 and Protection IV diamond armor. Leave other storage and the offhand empty. |
| Start or pause | Call `bot.start(human)` after countdown. The runtime makes the bot visible to the opponent and supplies inputs each tick. Call `bot.pause()` to clear inputs, then `bot.start(human)` to resume that round. |
| End or repeat | Call `bot.close()` to free the NPC and capacity. Spawn a fresh handle for the next round. Use `bot.modelId()` before closing to retain the same weights, including a resolved `training:<sha256>` ID. |
| Handle automatic removal | Listen for `BotClosedEvent` and match its owner and bot UUID. Use its reason to handle a lost participant, owner/runtime shutdown, or input failure. The event also fires after explicit close. |

For `sword_pvp_fixed_equipment_v1` use unbreakable equipment and disable natural health regeneration in the arena world. The runtime's 300-second model clock runs while the round is active. Your host sets the round duration and owns the result.

The host can show the bot to a spectator with `spectator.showEntity(this, bot.player())` and hide it with `spectator.hideEntity(this, bot.player())`. The server's `max-active-matches` setting limits the number of spawned bots. An idle bot uses a slot until it closes.

## Try the example

With the server stopped, install `paper/examples/rlbot-api/build/libs/pvpbot-rl-example.jar` in `paper/run-rl-standalone/plugins/` alongside the runtime JAR. The arena is created when the first player starts a duel.

| Command | What it does |
| --- | --- |
| `/rlbotexample start [model]` | Enter the arena with the selected opponent. The default is `recurrent-ppo`. |
| `/rlbotexample list` | Show installed models. |
| `/rlbotexample rematch` | Start another duel with the same resolved weights, including a previous random draw. |
| `/rlbotexample leave` | Leave the arena and restore your state. |
| `/rlbotexample status` | Check availability or time remaining. |
| `/rlbotexample recover` | Retry a saved restoration after resolving an inaccessible destination. |

Players have access through `pvpbotrlexample.demo` by default. Administrators with `pvpbotrlexample.admin` can pause or resume their duel. One duel uses the arena at a time.

The host gives both fighters 20 HP, a diamond sword in slot 0, Protection IV diamond armor, and empty remaining slots. Fighters start 20 blocks apart facing each other on an 80 × 80 block smooth-stone arena. The three-second countdown precedes 6,000 active server ticks. The first death decides the result, a double death is a draw, and time expiry awards no winner. Hunger, natural regeneration and contact pushing are disabled. The arena has physical walls, and the host protects the kit and blocks outside interference. The timer and both health values are shown during combat.

The host saves your original location, inventory, equipment, game mode, health, food, and effects before changing them. It restores them on exit, or on respawn or reconnect when needed. Both equipment profiles described in the [simulator rules](../docs/simulator/README.md#supported-match-profile) are supported. Use the same profile in the [host and runtime settings](RLBOT_SETUP.md#fixed-equipment-profile).

The [practice host source](examples/rlbot-api/src/main/java/dev/sugi/pvpbot/example/ExamplePlugin.java) uses the public Java API to create, start, pause, and close each bot.
