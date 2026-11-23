# Run PvPBotRL on Paper

PvPBotRL supplies sword-practice opponents through its Java API. It requires Minecraft Java Edition 26.1.2, Paper build 74, JDK 25 and Citizens 2.0.44 build 4270. The included [practice host example](RLBOT_API.md#try-the-example) provides a dedicated arena, match commands, and player recovery.

## Build

Run from the repository root in PowerShell.

```powershell
.\tools\paper\setup.bat -BuildOnly
```

The build verifies the pinned dependency JARs, runs the plugin tests, and writes the runtime and Java API JARs under `paper/rlbot/build/libs/`. Add `-Offline` after the dependencies are cached. See [API build instructions](RLBOT_API.md#install-and-build) for the example plugin.

## Start and stop the local server

Start the server in a PowerShell window.

```powershell
.\tools\paper\start.bat
```

Use another PowerShell window to check status or stop the server.

```powershell
.\tools\paper\console.bat 'rlbot status'
.\tools\paper\stop.bat
```

Read the [Minecraft EULA](https://www.minecraft.net/eula) before using `start.bat -AcceptEula` on first launch. The launcher checks and installs the pinned Paper and Citizens JARs, PvPBotRL, and ViaVersion into `paper/run-rl-standalone/`. The server listens on `127.0.0.1:25565` and accepts authenticated local RCON commands on port 25575. It uses a fresh run directory. Back up the world and plugin data together with the server stopped.

Connect with Minecraft Java Edition 26.1.2. After installing the [practice host example](RLBOT_API.md#try-the-example), run `/rlbotexample start` to enter the arena. The runtime commands `/rlbot list` and `/rlbot status` show available models and bot capacity. Administrators can apply runtime settings with `/rlbot reload`.

## Server configuration

Edit `plugins/PvPBotRL/config.yml` in the standalone server directory. `bot-rtt-ms` sets the total modeled delay for the bot's incoming information and outgoing actions. `max-active-matches` limits the number of bot handles, including idle ones. `diagnostics: true` writes motion TSV files to `plugins/PvPBotRL/diagnostics/` as bots close. Reloaded RTT and equipment settings apply to newly spawned bots.

### Fixed-equipment profile

To use fixed equipment with the practice host, set `equipment-profile: sword_pvp_fixed_equipment_v1` in both `plugins/PvPBotRL/config.yml` and `plugins/PvPBotRLExample/config.yml`, then restart. The host equips both fighters with an unbreakable unenchanted diamond sword and unbreakable Protection IV diamond armor. It disables natural health regeneration in the arena world. The default `finite_durability` profile uses ordinary item wear.

### Training models

Setup imports eligible models from `build/recurrent-ppo/full` and `build/recurrent-ppo/nextgen-24h` when those runs are available. It uses the training Python environment or a system Python 3.8 or newer. The server reads the existing catalog in `plugins/PvPBotRL/training-models/` at startup. The `random` model appears in `/rlbot list` when the catalog contains eligible models. A host can keep the resolved model ID to reuse the same weights for later rounds.
