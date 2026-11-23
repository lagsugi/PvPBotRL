# Working in this repository

This project models Minecraft Java Edition 26.1.2 sword duels, trains bot policies, and runs live duels on Paper. Run the commands below from the repository root in PowerShell. Write repository documentation and player-facing plugin text in English.

## Work areas and development priorities

Prioritize areas 1, 2, 3, 5, and 6. The local Rust duel window in area 4 is deprecated and retained for existing workflows. Direct new player-facing match features to the current Paper integration.

| Work area | Main locations | How to verify |
| --- | --- | --- |
| 1. Minecraft mechanics research | `docs/research/minecraft/`, `tools/research/minecraft/` | Record the target version, official JAR hashes, inspected classes, and corresponding probe results. [Research notes](docs/research/minecraft/README.md) |
| 2. Simulator and replay | `crates/pvp-sim/`, `crates/pvp-cli/`, `configs/` | Test physics, combat, and networking. Verify deterministic replay from the same seed and inputs. [Simulator guide](docs/simulator/README.md) |
| 3. Bot policy, training, and evaluation | `crates/pvp-policy/`, `crates/pvp-rl/`, `tools/research/recurrent-ppo/`, `models/` | Compare features and actions using shared fixtures in `tests/fixtures/policy/`, evaluate with seats swapped, and record promotion criteria and model hashes. [Research index](docs/research/README.md) |
| 4. Local Rust duel window (deprecated) | `crates/pvp-play/`, `tools/play/` | For changes affecting existing workflows, check input, rendering, audio, spectating, and recording and replay. [Legacy play guide](docs/play/README.md) |
| 5. Minecraft server duels | Runtime in `paper/rlbot/`, Example server in `paper/examples/rlbot-api/`, and launchers in `tools/paper/`. | Run plugin tests and verify match start, end, and player restoration on an isolated Paper server. [Current setup](paper/RLBOT_SETUP.md) |
| 6. Parity with real clients | `fabric/`, `tools/parity/`, `tests/fixtures/parity/`, and Paper diagnostics | Compare the ordinary client, Paper bot, and Rust at the same processing stage. Record the first divergence. [Parity report](docs/simulator/paper-parity.md) |

The **PvPBotRL** plugin owns observations, inference, and Citizens control. Its Java API lets a Paper host manage the match lifecycle, kits, spectators, and player restoration. **PvPBotRLExample** uses it to run Player vs Bot, Bot vs Bot, and Player vs Player rounds. [Java API](paper/RLBOT_API.md) · [Example server](paper/README.md)

## Preserve the contracts

- Keep simulator physics independent of policies, training, and rendering. The [architecture guide](docs/architecture.md) lists allowed Rust dependencies.
- Build actor inputs from the local player's state and information that its client has received. Keep packet delay, observation age, and remote interpolation in this path. Authoritative state is available for outcomes, rewards, and diagnostics.
- Preserve feature order and normalization, action masks and ordering, and per-seat recurrent state across the Python exporter, Rust inference, and Java inference. Reset memory and previous action at the episode boundary. For schema changes, exercise existing bundled models and older recordings through their normal loaders.
- Keep promoted weights and their provenance in `models/bundled/`. Use `models/archive/` for historical models used by experiments. Put new candidates, checkpoints, bulk traces, and generated Minecraft sources under `build/`. An experiment's promotion result and deployment of its weights are separate decisions.
- For parity work, compare the same processing stage and record the first divergence, including seed, seat, profile, and packet order where relevant. Use the isolated fixtures and fresh output directories in the [parity guide](tools/parity/README.md). A component-level pass only establishes agreement for that component under the tested conditions.

## Verify the affected area

| Change | Check |
| --- | --- |
| Rust workspace | `.\tools\rust\check.bat` checks formatting, crate dependencies, Clippy, and tests. For a focused iteration use `.\tools\rust\cargo.bat test --locked -p <crate>`. |
| Paper plugins | Use JDK 25 and `.\tools\paper\setup.bat -BuildOnly -WithExample` to prepare pinned dependencies and build/test both plugins without deploying. With dependencies available, rerun `.\paper\gradlew.bat -p paper/rlbot --max-workers=1 test` for a runtime-only check. |
| Fabric instrumentation | With JDK 25, run `.\fabric\gradlew.bat -p fabric build`. If you need generated Minecraft sources, invoke the separate task in the [Fabric guide](fabric/README.md). |
| Python training | Prepare the Python 3.12 environment with `.\tools\research\recurrent-ppo\setup.bat`, then run `.\build\rl-venv\Scripts\python.exe -B tools/research/recurrent-ppo/test_nextgen.py` for next-generation training changes. |
| Parity analysis and inference | Run `python -m unittest discover -s tools/parity` for the comparison tools. For inference changes, generate the Rust policy corpus and pass it to the Paper test with `-PparityFixture` as shown in the [parity guide](tools/parity/README.md#inference-corpus). The default Gradle test skips this optional corpus. |

Record which checks ran and what they measured. Use the experiment-specific short run for training workflow checks. The full campaigns have dedicated budgets and output directories.

## Publishing and CI

Base releases on the current `main` branch of `lagsugi/PvPBotRL`. Follow the [asset policy](README.md#asset-policy).

One open infrastructure task is Windows CI from a clean checkout with normal Git line-ending conversion. Run the Rust workspace checks, the Python parity unit tests, and `python tools/parity/verify_saved.py`. The last check exercises the saved input hashes and both expected outcomes for sprint feedback.
