# Repository architecture

Rust runs simulation, Python trains recurrent policies, and Paper runs the exported policies in Minecraft. When changing a model or the observation or action contract, update its consumers and verify compatibility in the same change.

1. **Simulation in Rust.** `pvp-sim` owns duel physics, combat, networking, and replay. `pvp-rl` batches environment steps and exposes observation features, action masks, rewards, and outcomes to Python through a C ABI. Rust runs opponent policies during training and evaluates exported models offline.
2. **RL training in Python.** The [PyTorch recurrent PPO trainer](../tools/research/recurrent-ppo/) runs policy updates and exports JSON weights for Rust and Paper. Campaign outputs include checkpoints and evaluation results under `build/`. [Training guide](research/README.md)
3. **Live deployment on Paper.** [PvPBotRL](../paper/rlbot/) loads the model weights, runs inference in Java, and drives Citizens bots. [PvPBotRLExample](../paper/examples/rlbot-api/) uses the public API to host matches, and other Paper plugins can use the same API. [Java API](../paper/RLBOT_API.md)

## Rust workspace

| Component | Responsibility | Rust workspace dependencies |
| --- | --- | --- |
| [`pvp-sim`](../crates/pvp-sim/) | Physics, combat, networking, observations, parallel environments and replays | None |
| [`pvp-policy`](../crates/pvp-policy/) | Bot decisions, feature transforms, inference and bot matchups | `pvp-sim` |
| [`pvp-rl`](../crates/pvp-rl/) | Batched training environments, rewards, league conditions, offline evaluation, and the C ABI for Python | `pvp-sim`, `pvp-policy` |
| [`pvp-cli`](../crates/pvp-cli/) | CLI for `duel`, `batch` and `replay` | `pvp-sim`, `pvp-policy` |
| [`pvp-play`](../crates/pvp-play/) | Deprecated local duel window for rendering, input, audio and recordings | `pvp-sim`, `pvp-policy` |

The CLI package is `pvp-cli` and its executable is `pvp-sim`. From the repository root, `cargo run -- duel --seed 42` runs the CLI. Run the environment API example with `cargo run -p pvp-sim --example vector_env`.

### Legacy Rust training

The Duel-RL and League trainers and their shared [Augmented Random Search implementation](../crates/pvp-rl/examples/common/ars.rs) are legacy code for reproducing earlier experiments. Run them with `cargo run -p pvp-rl --example duel_rl` or `cargo run -p pvp-rl --example league_rl` and follow the arguments in the experiment guide. [Reproduction guides](research/README.md)

[`fabric`](../fabric/) is a separate Gradle project for real-client instrumentation.

## Models and interface contracts

- [`models/bundled`](../models/bundled/) contains the model files embedded by Rust and Paper at build time. Keep provenance beside each model. [`models/archive`](../models/archive/) holds weights from earlier generations for training and evaluation. Write candidates and experiment logs under `build/`.
- A bot acts on client-visible observations and its own client state. Read the latter with `Duel::local_player`. The `pvp-policy` crate defines GRU feature order, input variants and action decoding. Authoritative server state supplies outcomes, training rewards and diagnostics.
- Rust and Java use [shared fixtures](../tests/fixtures/policy/) to compare decisions for identical observations. When changing a model or its inputs, verify the Python export and both the Rust and Java loaders.

Run `.\tools\rust\check.bat` for formatting, dependency-direction checks, Clippy and Rust tests. See the [verification commands](../AGENTS.md#verify-the-affected-area) for Paper and training regression tests.
