# League-G5 and League-RL (G4)

G5 and G4 are available in the play client, spectator mode and CLI. Use `league-rl` for G4 and `league-g5` for G5.

```powershell
# Play G5. Press Enter on the ready screen.
.\tools\play\start.bat --opponent league-g5

# Spectate G5 (A) vs G4 (B) with independent network draws.
.\tools\play\start.bat --bot-a league-g5 --opponent league-rl

# Spectate with both RTTs fixed at 120 ms, 60 ms in each direction.
.\tools\play\start.bat --bot-a league-g5 --opponent league-rl --ping-profile fixed --human-ping 120 --bot-ping 120
```

## Independent G5 evaluation

G5 won a majority head-to-head against G4 with both RTTs sampled from the existing ping distribution and with equal RTTs fixed at 20, 60 or 120 ms. For each of these four tests the null was a G5 win rate of at most 50%. The largest Bonferroni-adjusted one-sided exact binomial p-value was 3.69 × 10⁻⁶⁹. The Wilson intervals below are individual 95% intervals. The 120 vs 20 ms matchup was an additional stress test.

| Conditions vs G4 | G5 wins / matches | G5 win rate | 95% CI |
| --- | ---: | ---: | ---: |
| Normal ping | 620 / 768 | 80.7% | 77.8–83.4% |
| Both 20 ms | 652 / 768 | 84.9% | 82.2–87.3% |
| Both 60 ms | 650 / 768 | 84.6% | 81.9–87.0% |
| Both 120 ms | 668 / 768 | 87.0% | 84.4–89.2% |
| G5 120 ms, G4 20 ms | 204 / 256 | 79.7% | 74.3–84.2% |

The table gives win percentages per opponent. Each normal, 20, 60 and 120 ms cell used 768 matches, and the adverse condition used 256. With all 17 opponents weighted equally at normal ping, G5 won **11,384 / 13,056 (87.2%)** and G4 won **9,640 / 13,056 (73.8%)** under identical conditions. There were no timeouts. By G5's own RTT at normal ping, G5 won 91.7% (5,272/5,749) below 50 ms, 87.2% (4,487/5,146) from 50 to under 100 ms, and 75.2% (1,625/2,161) at 100 ms or above.

| Opponent | G4 normal | G5 normal | G5 equal 20 ms | G5 equal 60 ms | G5 equal 120 ms | G5 120 ms, opponent 20 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Duel-RL | 82.9 | 99.3 | 100.0 | 99.7 | 96.4 | 98.4 |
| Critical+ | 82.3 | 77.3 | 79.9 | 84.5 | 78.5 | 77.3 |
| Critical | 94.4 | 93.5 | 98.6 | 96.6 | 82.9 | 93.0 |
| Pressure | 73.6 | 73.7 | 75.5 | 77.3 | 57.2 | 64.1 |
| Orbit | 99.1 | 91.7 | 95.4 | 94.5 | 98.6 | 87.5 |
| Counter | 96.5 | 99.1 | 97.7 | 99.9 | 99.9 | 100.0 |
| Baseline | 93.0 | 95.4 | 95.4 | 92.4 | 99.9 | 94.5 |
| Generation 1 | 88.0 | 99.9 | 100.0 | 100.0 | 99.6 | 99.6 |
| Generation 2 | 63.7 | 88.7 | 100.0 | 96.7 | 67.3 | 62.5 |
| Generation 3 | 69.8 | 93.9 | 98.8 | 98.0 | 67.4 | 91.0 |
| G2 specialist RL | 61.8 | 88.5 | 99.6 | 98.2 | 49.9 | 64.8 |
| G3 specialist RL | 55.7 | 82.3 | 83.2 | 93.9 | 82.7 | 67.6 |
| G4 specialist RL (training opponent) | 43.9 | 88.0 | 99.9 | 98.2 | 73.8 | 63.7 |
| G4 (League-RL) | 51.2 | 80.7 | 84.9 | 84.6 | 87.0 | 79.7 |
| Reach | 83.1 | 69.9 | 89.5 | 88.2 | 25.0 | 43.8 |
| Orbit/Pressure | 82.9 | 74.9 | 78.5 | 83.3 | 87.4 | 83.6 |
| G4 specialist RL (held-out lineage) | 33.3 | 85.4 | 99.2 | 94.0 | 77.3 | 49.2 |

G5 won a majority against all 17 opponents at normal ping, with regressions against Critical+ (82.3% to 77.3%) and Reach (83.1% to 69.9%). Against Reach at equal 120 ms G5 won 192/768 (25.0%) and G4 won 455/768 (59.2%). Against the G2 specialist at equal 120 ms, wins rose from G4's 131/768 (17.1%) to 383/768 (49.9%). A promising follow-up is to train pursuit timing on slow links against several opponents that retreat at close range and then re-engage. Results apply to the specified opponents and conditions in this Rust simulator.

## Jump tests

In 28 local human-vs-G4 recordings, the bundled G4 reproduced the recorded G4 action at every step. Of 504 human attacks accepted by the server, 237 (47.0%) had a grounded jump input on the hit decision step or within one step on either side. On 195 hits (38.7%), G4 was airborne at the start of the hit step. The two measures can overlap. The recordings do not quantify knockback prevented specifically by jump resets.

In 384 other settings we fixed G4 as the opponent and disabled only the acting bot's jump input, pairing matches by initial condition. G5 won 337 (87.8%) normally and 325 (84.6%) with jumping disabled. As the actor, G4 won 197 normally and 193 without jumping. The comparison covers jumping during approach, aerial attacks and incoming hits. The isolated effect of jump resets in human matches remains untested.

## Changes since G4

G4 uses 16 features from a single client observation. G5 started from G4's weights and added recent damage taken, recent attack input, changes in spacing across ticks and the fraction of time the opponent spent airborne. Its 28 features and four outputs control forward/backward motion, strafing, jump and attack click at 20 Hz. Each seat resets its own memory for each new match. The policy has no access to configured RTT, opponent identity or inputs, or server internals.

In user-provided equal-ping tests across two seed series, G4 at equal 120 ms won 13.2–16.5% against the G2 specialist and 20.6–25.8% against generation 3. This prompted a search for weak opponent/network combinations in G5.

1. G4 trained in a league with Duel-RL, six fixed bots, current self-play and past generations. Generation 4 added the G2 specialist and sampled more matches on the slower side. G4's independent normal-ping evaluation yielded 8,648 wins / 10,752 matches against 14 opponents.
2. A G4 specialist trained for 30,720 matches for use in G5 training. G5 faced six fixed bots, Duel-RL, generations 1–4, three specialists and its current policy. ARS evaluated positive and negative weight perturbations under identical conditions and updated from rewards based on outcomes, damage differential and duration. Each policy used only its own seat's observations.
3. Stage one ran 479,232 matches and selected update 72 using worst-opponent win rate, weak opponent/ping combinations and G4 head-to-head results. Stage two ran 221,184 matches, allocating trials to conditions that had regressed from G4, and selected update 88 on separate selection matches. G5 training totaled **700,416 matches**.
4. After freezing the bundled weights, we evaluated G5 and G4 against the same opponents from the same initial conditions using a new random stream. Evaluation-only opponents included Reach, which retreats at close range, a bot alternating Orbit and Pressure within a match, and an independent G3 lineage trained solely to beat G4. Its weights were held out of G5 training and selection.

AlphaStar's league training informed the use of historical policies and opponents trained to beat a particular policy. [League training paper](https://www.nature.com/articles/s41586-019-1724-z) · [ARS paper](https://arxiv.org/abs/1803.07055)

## Evaluation conditions

Normal ping uses the existing `PingProfile`. Each seat independently draws RTT from a truncated gamma distribution over 5–150 ms and an upstream share from 40–60%. The three equal-ping conditions fix both directional delays identically for both players. A separate adverse condition sets G5 to 120 ms and its opponent to 20 ms. Delays stay fixed for the match. [Ping model](../ping/realistic-ping.md)

In each condition, 35% of matches used the play client's default spacing, 60 FPS and phases. The remainder sampled a 3.5–32 block gap, lateral offset, 30/60/144/240 FPS, and receive/tick phases. Each setting sampled seats and simultaneous-input order once for both models. Both players had equal equipment and 20 HP. Reach (3 blocks) and auto-aim matched existing bots. Timeouts counted as non-wins.

## Reproduction

```powershell
# Reproduce all evaluations with the bundled model.
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-rl --example league_rl -- g5-evaluate

# Reproduce both G5 training stages from G4 and historical models.
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-rl --example league_rl -- g5-train
Get-FileHash build/benchmarks/league-rl/g5-final.json, models/bundled/league_g5.json

# Recount G4 jump inputs around incoming hits in the user's local recordings.
.\tools\rust\cargo.bat run --release --locked --offline -p pvp-rl --example league_rl -- audit-jumps
```

Training logs, aggregates and paired match results are in `g5-training.json`, `g5-refinement.json`, `g5-summary.json` and `g5-episodes.json` under `build/benchmarks/league-rl/`. Evaluation uses the bundled G5. Training saves candidates under build. See [inference](../../../crates/pvp-policy/src/learned.rs), [training and evaluation](../../../crates/pvp-rl/examples/league_rl/g5.rs) and the [selected weights](../../../models/bundled/league_g5.json).
