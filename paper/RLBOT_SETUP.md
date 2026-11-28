# Run PvPBotRL on Paper

PvPBotRL supplies sword-practice opponents through its Java API. It requires Minecraft Java Edition 26.1.2, Paper build 74, JDK 25 and Citizens 2.0.44 build 4270. The [Example server](README.md) uses that API to offer three duel modes, spectating, multiple arenas, and player recovery.

## Build

Run from the repository root in PowerShell.

```powershell
.\tools\paper\setup.bat -BuildOnly
```

The build verifies the pinned dependency JARs, runs the runtime tests, and writes the runtime and Java API JARs under `paper/rlbot/build/libs/`. Add `-WithExample` to also test and build `paper/examples/rlbot-api/build/libs/pvpbot-rl-example.jar`. Add `-Offline` after the dependencies are cached.

## Start and stop the local server

Start the server in a PowerShell window.

```powershell
.\tools\paper\start.bat -WithExample
```

Use another PowerShell window to check status or stop the server.

```powershell
.\tools\paper\console.bat 'rlbot status'
.\tools\paper\stop.bat
```

Read the [Minecraft EULA](https://www.minecraft.net/eula) before using `start.bat -AcceptEula` on first launch. The launcher checks and installs the pinned Paper and Citizens JARs, plus PvPBotRL, into `paper/run-rl-standalone/`. The server listens on `127.0.0.1:25565` and accepts authenticated local RCON commands on port 25575. It uses a fresh run directory. Back up the world and plugin data together with the server stopped.

On the first installation, `-WithExample` builds and installs the Example beside the runtime. Subsequent runs of `start.bat` or `setup.bat` update an installed Example together with the runtime, even without `-WithExample`. Connect with Minecraft Java Edition 26.1.2 and run `/rlbotexample start` to enter the arena. Run `/rlbotexample help` to see the other match options. API integrators can use `start.bat` on a fresh installation to install the runtime. The runtime commands `/rlbot list` and `/rlbot status` show available models and bot capacity. Administrators can apply runtime settings with `/rlbot reload`.

## Server configuration

Edit `plugins/PvPBotRL/config.yml` in the standalone server directory. `bot-rtt-ms` sets the total modeled delay for the bot's incoming information and outgoing actions. `max-active-matches` limits the number of bot handles, including idle ones. `diagnostics: true` writes motion TSV files to `plugins/PvPBotRL/diagnostics/` as bots close. Reloaded RTT settings apply to newly spawned bots.

### Fixed-equipment profile

Both plugins use `sword_pvp_fixed_equipment_v1`. The Example issues an unbreakable diamond sword and unbreakable Protection IV diamond armor. Natural regeneration must be off in the arena world. The Example sets this rule in its built-in arena world. The legacy `finite_durability` value in an existing YAML file is accepted as a compatibility alias for fixed equipment.

### Training models

Setup imports eligible models from `build/recurrent-ppo/full` and `build/recurrent-ppo/nextgen-24h` when those runs are available. It uses the training Python environment or a system Python 3.8 or newer. The server reads the existing catalog in `plugins/PvPBotRL/training-models/` at startup. The `random` model appears in `/rlbot list` when the catalog contains eligible models. A host can keep the resolved model ID to reuse the same weights for later rounds.
