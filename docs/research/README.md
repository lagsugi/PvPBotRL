# Training, evaluation and Minecraft mechanics

Run the reproduction commands from the repository root.

| Topic | Experiments and supporting material |
| --- | --- |
| Bot training and evaluation | [League-G6](bots/league-g6.md) at 0–150 ms and under different attack timings, [League-G5 and G4](bots/league-rl.md) by generation and ping, [Duel-RL](bots/duel-rl.md) training and weight verification, and the [Critical+](bots/critical-plus.md) parameter search |
| Tactics | [Combos and first-hit distance](bots/combo-research.md) and [practice opponent comparisons](bots/practice-opponents.md) |
| Ping | [RTT and directional delay model](ping/realistic-ping.md) and [win rates, aggregation and plots across 112,128 matches](ping/ping-impact.md) |
| Minecraft mechanics | [Combat, movement and networking](minecraft/README.md) and [verification using official JARs and generated sources](minecraft/source-index.md) |

Training and evaluation programs live in the [Rust examples](../../crates/pvp-rl/examples/). Selected weights live in [models](../../models/bundled/). Training commands write candidate weights and evaluation logs under `build/benchmarks/`.
