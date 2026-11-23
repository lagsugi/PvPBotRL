# Training, evaluation and Minecraft mechanics

Run the reproduction commands from the repository root.

| Topic | Experiments and supporting material |
| --- | --- |
| Recurrent PPO in Python | [Next-generation GRU training and evaluation](bots/next-gen-gru-run.md) and the [recurrent-G2+ campaign](bots/recurrent-g2-plus-plan.md) |
| Legacy ARS training in Rust | Reproduce [League-G6](bots/league-g6.md), [League-G5 and G4](bots/league-rl.md), and [Duel-RL](bots/duel-rl.md) |
| Tactics | [Combos and first-hit distance](bots/combo-research.md), [practice opponent comparisons](bots/practice-opponents.md), and the [Critical+](bots/critical-plus.md) parameter search |
| Ping | [RTT and directional delay model](ping/realistic-ping.md) and [win rates, aggregation and plots across 112,128 matches](ping/ping-impact.md) |
| Minecraft mechanics | [Combat, movement and networking](minecraft/README.md) and [verification using official JARs and generated sources](minecraft/source-index.md) |

The [Python trainers](../../tools/research/recurrent-ppo/) use batched Rust environments and write campaign outputs under `build/recurrent-ppo/`. The [Rust examples](../../crates/pvp-rl/examples/) include the legacy ARS trainers, which write outputs under `build/benchmarks/`. Selected weights and provenance live in [`models/bundled/`](../../models/bundled/).
