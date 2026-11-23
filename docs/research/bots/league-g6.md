# League-G6

League-G6 continues ARS training from G5. Training in the Minecraft sword PvP simulator covers additional timing conditions so that the bot can land critical hits even against an opponent with localhost latency.

## Policy and training conditions

The linear policy extends G5's 28 features with eight more, for 36 features and four outputs. The additions encode grounded state, descent without sprinting, descent combined with charge and reach margin, ascent or sprinting descent combined with charge, aim overlap, and reach margin at full charge. G6 uses G5's short-term memory and action decoding. Inference uses only the observation from the bot's own seat.

Training RTT ranges from 0 to 150 ms. For independent draws each seat samples uniformly from 0–5 ms with 20% probability, uniformly from 0–150 ms with 40%, and from the previous 5–150 ms truncated gamma distribution with 40%. Other conditions give both seats 0, 2, 20, 60, 120 or 150 ms, give one side 0–5 ms, or use 150 vs 0 ms and learner 20 vs opponent 0 ms. Half of the 0–5 ms draws use exactly 0 ms. When the opponent has low latency, half the learner draws use 20 ms. Starting gap, lateral offset, FPS, tick and render phases, and seat assignments vary as in G5.

Opponents include G1–G5, Duel-RL, fixed bots, Reach, specialist opponents and the current G6. ARS evaluates paired weight perturbations from identical starts and updates on the ten highest-return directions (12 in the final stage). G6 uses the G4/G5 outcome reward (±1), damage differential (0.12 per HP) and time penalty (−0.0001 per step). It also rewards the first six critical hits that deal server-confirmed damage. In stages one and two each earns 0.055 in low-latency-focused conditions or 0.018 elsewhere. The final stage uses 0.035 and 0.012. Selection matches compare head-to-head performance against G5, multi-opponent win rates, regressions from G5 and critical-hit share against near-zero-latency opponents.

The final stage adds a bot-at-20 ms/opponent-at-0 ms case with 60 FPS, client phases at zero and the server tick 11 ms after the decision boundary. A test confirms the bot's 10 ms upstream attack arrives one tick earlier than under the usual phase of zero. Paper applies attacks directly from a saved observation at tick start. This Rust condition measures sensitivity to earlier application. Reproducing Paper observations, Citizens movement and human inputs requires separate tests.

The three stages ran 258,048, 331,776 and 331,776 matches for **921,600 matches** in total. The selected updates were 138, 84 and 126, respectively.

## Independent evaluation

After checkpoint selection we evaluated with a separate random stream. Across 13 conditions G6 won **5,485 / 9,984 matches (54.9%)** against G5. Against 16 other opponents, each opponent played 256 matches per condition. Overall results were **G6 46,284 / 53,248 (86.9%) and G5 45,796 / 53,248 (86.0%)**. Head-to-head timeouts do not count as wins. Critical-hit share is the fraction of damaging attacks that were criticals.

| RTT and attack application | G6 wins vs G5 | Win rate vs 16 opponents, G5 → G6 | Critical-hit share vs 16 opponents, G5 → G6 |
| --- | ---: | ---: | ---: |
| Mixed 0–150 ms | 54.0% | 86.2% → 88.1% | 66.1% → 68.3% |
| Both 0 ms | 62.6% | 90.5% → 87.6% | 63.3% → 66.2% |
| Opponent 0–5 ms | 58.9% | 88.1% → 89.2% | 62.6% → 65.1% |
| Bot 20 ms, opponent 0 ms | 63.2% | 85.7% → 89.0% | 59.0% → 63.8% |
| Bot 20 ms, opponent 0 ms, earlier attack phase | 35.8% | 74.3% → 77.4% | 64.2% → 63.4% |
| Both 120 ms | 47.8% | 79.0% → 79.8% | 73.5% → 75.2% |
| Both 150 ms | 44.8% | 79.4% → 79.1% | 75.4% → 77.5% |

In the earlier-attack case against G5, critical-hit share rose from G5's 62.0% to G6's 91.0%, but G6 finished 275–493. Across the 16 opponents, win rate and critical-hit share moved in opposite directions. These are Rust simulator results.

## Reproduction

```powershell
# Evaluate the bundled model with randomness held out of training and selection.
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-rl --example league_rl -- g6-evaluate

# Reproduce training from G5 weights.
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-rl --example league_rl -- g6-train
Get-FileHash build/benchmarks/league-rl/g6-final.json, models/bundled/league_g6.json

# Spectate G6 vs G5 in Rust with G6 at 20 ms and G5 at 0 ms.
.\tools\play\start.bat --bot-a league-g6 --opponent league-g5 --ping-profile fixed --human-ping 20 --bot-ping 0
```

Training and evaluation summaries are stored under `build/benchmarks/league-rl/` in `g6-training.json`, `g6-refinement.json` and `g6-summary.json`. See the [implementation](../../../crates/pvp-rl/examples/league_rl/g6.rs) and [selected weights](../../../models/bundled/league_g6.json).
