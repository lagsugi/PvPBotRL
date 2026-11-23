# PvP Bot

This project studies and simulates sword PvP in Minecraft Java Edition 26.1.2, trains and evaluates bots, and runs live duels on Paper. Run the commands below in PowerShell from the repository root.

PvP Bot is an unofficial Minecraft project by lagsugi. It is neither approved by nor associated with Mojang or Microsoft.

## Simulation, training, and deployment

| Stage | Stack | Responsibility |
| --- | --- | --- |
| Simulation | Rust | Runs duels, replays, and batched training environments. [Simulator guide](docs/simulator/README.md) |
| RL training | Python and PyTorch | Trains recurrent policies with PPO against the Rust environments and exports model weights as JSON. [Training guide](docs/research/README.md) |
| Live deployment | Paper and Java | Loads model weights, runs inference, and controls Citizens bots. The Example plugin hosts live duels. [Paper setup](paper/RLBOT_SETUP.md) |

The Rust Augmented Random Search (ARS) trainers are legacy code retained to reproduce the Duel-RL and League experiments. [Architecture guide](docs/architecture.md)

## Quickstart

| Task | First step | Guide |
| --- | --- | --- |
| Play or watch duels in Minecraft | Start Paper with `.\tools\paper\start.bat -WithExample` | [Paper setup](paper/RLBOT_SETUP.md) · [Duel commands and rules](paper/README.md) |
| Use an RL bot in a Paper plugin | Build against the PvPBotRL API JAR | [Java API quickstart and example](paper/RLBOT_API.md) |
| Use the simulator | Run one duel with `.\tools\rust\cargo.bat run --release --locked -- duel --seed 42` | [CLI, replays and Rust API](docs/simulator/README.md) |
| Reproduce training or evaluation | Choose a bot or ping experiment in the research guide and run its commands | [Training, evaluation and Minecraft mechanics](docs/research/README.md) |
| Develop the Fabric mod | With JDK 25, run `.\fabric\gradlew.bat -p fabric build` | [Build, configure the IDE and launch a development client](fabric/README.md) |
| Compare behavior with Minecraft | Use the isolated client and server fixtures | [Parity workflow](tools/parity/README.md) |

**Deprecated.** The native Rust duel window (`pvp-play`) remains available for existing local play, spectating, and recording workflows. See the [legacy local play guide](docs/play/README.md).

Development focuses on Minecraft mechanics research, the simulator and replay CLI, bot training and evaluation, Paper match operations, and parity with real clients. See the [work areas and verification criteria](AGENTS.md#work-areas-and-development-priorities).

## Development files and data

- The [architecture guide](docs/architecture.md) describes the simulator, policies, training, CLI and play client, including their dependency directions. Run `.\tools\rust\check.bat` to check the Rust workspace.
- [`configs/`](configs/) contains sample match and network configurations. Research tools write generated files under `build/` at the repository root.
- [`fabric/`](fabric/) contains the Minecraft integration mod. Its build artifacts go to `fabric/build/libs/` and its development worlds and settings go to `fabric/run/`.

The [implementation and validation guide](docs/simulator/implementation.md) describes the combat, movement and networking modeled by the simulator and the remaining comparisons with Minecraft clients and servers.

## License and release contents

Copyright © 2026 lagsugi. The project-authored source, documentation and original art are available under the [GNU General Public License, version 3 only](LICENSE), with [additional permissions for platform integration and model bundling](LICENSE-EXCEPTIONS.md).

The [model files](models/README.md) are available under CC BY 4.0 with credit to lagsugi. The [project's test fixtures](tests/fixtures/README.md) are available under CC0 1.0 within the scope described there. [Third-party notices](THIRD_PARTY_NOTICES.md) identify files that retain their upstream terms and dependencies installed from separate sources.

Distribution of Minecraft integrations and use of Minecraft servers, including server monetization, are also subject to Mojang's terms. [Minecraft EULA](https://www.minecraft.net/en-us/eula) · [Minecraft Usage Guidelines](https://www.minecraft.net/en-us/usage-guidelines)

### Binary releases

For a Paper release, build the PvPBotRL runtime, API and Example JARs from the release tag with `.\tools\paper\setup.bat -BuildOnly -WithExample`. Publish those JARs as free downloads on the release page. Obtain Paper and Citizens separately from their upstream distributions through the standalone setup. [Paper setup](paper/RLBOT_SETUP.md)

Next to the binary downloads, provide free access to the complete Corresponding Source for that exact release. Link to the release tag's source archive when it contains the source, build and installation scripts, and any bundled models needed for that build. Keep the GPL text, additional permissions and applicable model attribution with the release. A Rust executable release must also carry the required notices for the dependencies it includes. [GPLv3 sections 1 and 6](LICENSE) · [Model attribution](models/README.md)

## Asset policy

Use only original artwork and audio. Never use assets taken or adapted from Minecraft or any other game.
