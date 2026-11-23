# PvPBotRL Java API quickstart

PvPBotRL supplies the NPC and controls it in sword rounds. Your Paper plugin prepares the fighters and arena, starts and pauses combat, determines the result, and restores players. [Server setup](RLBOT_SETUP.md)

## Install and build

Use Paper 26.1.2 build 74, Citizens 2.0.44 build 4270, and JDK 25. Install Citizens and `pvpbot-rl-0.1.0.jar` in the server's `plugins/` directory.

Run this command from the repository root in PowerShell to build the runtime, API JAR, and Example.

```powershell
.\tools\paper\setup.bat -BuildOnly -WithExample
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
| Record the bot settings | Call `bot.info()` for the resolved model ID, weights SHA-256 when applicable, policy version, and simulated RTT. |

For `sword_pvp_fixed_equipment_v1` use unbreakable equipment and disable natural health regeneration in the arena world. The runtime's 300-second model clock runs while the round is active. Your host sets the round duration and owns the result.

The host can show the bot to a spectator with `spectator.showEntity(this, bot.player())` and hide it with `spectator.hideEntity(this, bot.player())`. The server's `max-active-matches` setting limits the number of spawned bots. An idle bot uses a slot until it closes.

## Run a Bot vs Bot round

Spawn both handles under the same plugin in the same world, and equip both before calling `api.startPair(botA, botB)`. The pair consumes two capacity slots. Each bot has its own policy state, observation replica, and action delay. Pausing either handle pauses both. Resume using `startPair` with the same handles. If preparation fails after spawning the first bot, close any handles you obtained before handling the error.

Read the fighters' UUIDs while their handles are open and decide the result from Paper combat events. Close both handles at round end. When one closes early, the other stops and the runtime releases it on its next tick.

## Try the Example

Start Paper with `.\tools\paper\start.bat -WithExample`. The Example provides Player vs Bot, Bot vs Bot and Player vs Player rounds. See the [server commands and rules](README.md) and the [match lifecycle source](examples/rlbot-api/src/main/java/dev/sugi/pvpbot/example/MatchManager.java).
