# Repository architecture

The simulator, training code and play client share a repository. When changing a model or the observation or action contract, update its consumers and verify compatibility in the same change.

| Component | Responsibility | Rust workspace dependencies |
| --- | --- | --- |
| [`pvp-sim`](../crates/pvp-sim/) | Physics, combat, networking, observations, parallel environments and replays | None |
| [`pvp-policy`](../crates/pvp-policy/) | Bot decisions, feature transforms, inference and bot matchups | `pvp-sim` |
| [`pvp-rl`](../crates/pvp-rl/) | Training environments, rewards, league conditions, experiments, evaluation and the C ABI for Python | `pvp-sim`, `pvp-policy` |
| [`pvp-cli`](../crates/pvp-cli/) | CLI for `duel`, `batch` and `replay` | `pvp-sim`, `pvp-policy` |
| [`pvp-play`](../crates/pvp-play/) | Deprecated local duel window for rendering, input, audio and recordings | `pvp-sim`, `pvp-policy` |

The CLI package is `pvp-cli` and its executable is `pvp-sim`. From the repository root, `cargo run -- duel --seed 42` runs the CLI. Run Rust experiments with `cargo run -p pvp-rl --example <name>` and the environment API example with `cargo run -p pvp-sim --example vector_env`. The Python training tools live in [`tools/research/recurrent-ppo`](../tools/research/recurrent-ppo/).

[`paper/rlbot`](../paper/rlbot/) runs bot inference and control in Java for live matches managed by a [Paper host plugin](../paper/RLBOT_API.md). [`fabric`](../fabric/) is a separate Gradle project for real-client instrumentation.

## Models and interface contracts

- [`models/bundled`](../models/bundled/) contains the model files embedded by Rust and Paper at build time. Keep provenance beside each model. [`models/archive`](../models/archive/) holds weights from earlier generations for training and evaluation. Write candidates and experiment logs under `build/`.
- A bot acts on client-visible observations and its own client state. Read the latter with `Duel::local_player`. The `pvp-policy` crate defines GRU feature order, input variants and action decoding. Authoritative server state supplies outcomes, training rewards and diagnostics.
- Rust and Java use [shared fixtures](../tests/fixtures/policy/) to compare decisions for identical observations. When changing a model or its inputs, verify the Python export and both the Rust and Java loaders.

Run `.\tools\rust\check.bat` for formatting, dependency-direction checks, Clippy and Rust tests. See the [verification commands](../AGENTS.md#verify-the-affected-area) for Paper and training regression tests.
