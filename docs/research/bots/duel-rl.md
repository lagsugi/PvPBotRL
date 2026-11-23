# Duel-RL against Critical+

In 512 independent evaluation matches in the Rust simulator, Duel-RL recorded **410 wins and 102 losses (80.08%)**. Both bots started at 20 HP with standard equipment, reach, aim, input rate and physics. The opponent was the bundled Critical+.

| Evaluation conditions | Wins | Losses | Timeouts |
| --- | ---: | ---: | ---: |
| Held-out randomized settings, Duel-RL in seat A | 195 | 50 | 0 |
| Held-out randomized settings, Duel-RL in seat B | 215 | 52 | 0 |
| Standard settings, 2 seats × 2 processing orders | 4 | 0 | 0 |

### Evaluation method

We evaluated after selecting the weights. Each match sampled a starting Z gap of 3.5–32 blocks and a lateral offset of −2 to 2 blocks. Each seat drew independent upstream and downstream delays of 0–90 ms and an FPS of 30, 60, 144 or 240. Render, client tick and server tick phases also varied. Each setting drew one seat assignment and simultaneous-input order and counted as one match. Timeouts count in the win-rate denominator.

The 95% Wilson interval for the win rate under this distribution is **76.40–83.31%**. A one-sided exact binomial test against a win rate of at most 50% gave **p = 4.66 × 10⁻⁴⁵**. The four standard games also appeared in training and selection and were excluded from the test. Evaluation took place entirely in the simulator.

### Training method

Duel-RL uses its own client observation at 20 decisions per second to choose forward/backward movement, left strafe, jump and attack click. Its linear policy has 10 features including bias and 4 outputs. Inputs cover distance and height difference, closing speed, attack charge and both players' grounded and movement states. Forward motion beyond 8 blocks, permission to click within 3 blocks and automatic aiming are fixed. The executable embeds the trained weights for inference.

Training used Augmented Random Search (ARS). Each update applied positive and negative perturbations in 24 directions and played each pair on the same 10 conditions. Rewards were +1 for a win, −1 for a loss, +0.15 per HP of damage dealt minus damage taken, and −0.0001 per step. The return differences in the best eight directions determined each update. Training ran 64 updates and 30,720 matches. [Original ARS paper](https://arxiv.org/abs/1803.07055)

The initial policy and every second update played four standard and 96 selection matches. Selection ranked by standard wins, selection wins and selection return, in that order. Update 56 won. The initial policy won 0 standard and 48 selection matches. The selected policy won 4 and 87. Final evaluation used a separate random stream.

### Play, spectate and reproduce

Run from the project root.

```powershell
# Play against Duel-RL.
.\tools\play\start.bat --opponent duel-rl

# Spectate. On the ready screen press A twice to select Duel-RL as Bot A, then Enter.
.\tools\play\start.bat --spectate --opponent critical-plus

# Run one standard match in the CLI.
.\tools\rust\cargo.bat run --release --locked -p pvp-cli -- duel --bot-a duel-rl --bot-b critical-plus --seed 42

# Re-evaluate the bundled weights on the 512 held-out and four standard matches.
.\tools\rust\cargo.bat run --release --locked -p pvp-rl --example duel_rl -- evaluate

# Reproduce training and compare the weight hash with the bundled model.
.\tools\rust\cargo.bat run --release --locked -p pvp-rl --example duel_rl -- train
Get-FileHash build/benchmarks/duel-rl/checkpoint.json, models/bundled/duel_rl.json
```

Training writes candidate weights and selection history to `build/benchmarks/duel-rl/`. Evaluation saves aggregates, weights and individual match results in `evaluation.json` under that directory. See the [bundled model](../../../models/bundled/duel_rl.json) and the [training and evaluation implementation](../../../crates/pvp-rl/examples/duel_rl.rs).
