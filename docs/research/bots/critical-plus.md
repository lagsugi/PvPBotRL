# Critical+ tuning results (2026-11-06)

**Critical+** adds a tuned parameter set alongside the original **Critical** bot.
The search covered **4,998 distinct configurations**. We froze the parameters selected in tuning matches before testing on separate evaluation conditions.
These results apply to the Rust simulator and the search space tested. Strength against other opponents or in Minecraft itself remains unverified.

## Head-to-head against the original Critical

| Conditions | Critical+ wins | Original Critical wins | Timeouts | Critical+ win rate |
| --- | ---: | ---: | ---: | ---: |
| Standard settings with seat and simultaneous-input order swapped | 4 | 0 | 0 | 100% |
| Evaluation conditions held out of tuning | 430 | 82 | 0 | 83.98% |

Standard settings used a 20-block starting gap, 20 HP per player, 60 FPS, 10 ms per network direction and seed 42.
Critical+ averaged **3.576 / 20 HP** remaining. Matches ended after **21.025 seconds** on average.

The separate evaluation covered 128 settings × 2 seats × 2 processing orders, or 512 matches. Seeds 10001–10128 varied the starting gap (3.5–32 blocks, including some standard starts), lateral offset, upstream and downstream delays (0–90 ms each), FPS (30/60/144/240) and client/server processing phases.
Critical+ went 214–42 in seat A and 216–40 in seat B. Mean match duration was 20.875 seconds.
This deterministic evaluation repeats each setting across seats and processing orders, so the 512 matches are correlated.

## Other opponents

Each opponent played 128 matches under 32 separate settings × 2 seats × 2 processing orders. Critical and Critical+ faced identical conditions.

| Opponent | Original Critical W–L | Critical+ W–L |
| --- | ---: | ---: |
| Baseline | 124–4 | 124–4 |
| Pressure | 56–72 | 76–52 |
| Orbit | 120–8 | 126–2 |
| Counter | 128–0 | 126–2 |
| Total | 428–84 (83.59%) | 452–60 (88.28%) |

The largest improvement came against Pressure. Critical+ lost two more matches against Counter.

## Selected parameters

| Parameter | Original Critical | Critical+ |
| --- | ---: | ---: |
| Jump when horizontal distance is below | 7.5 | 7.5 |
| Jump when charge exceeds | 0.35 | 0.95 |
| Release W in the air when horizontal distance is below | 4.5 | 4.0 |
| Release W in the air when vertical velocity is below | 0.25 | 0.0 |
| Press W when horizontal distance exceeds | 1.4 | 0.0 |
| Press S when horizontal distance is below | 1.0 | 1.7 |
| Attack when charge exceeds | 0.93 | 0.90 |
| Ticks to wait for a critical before switching to a sprint attack | 15 | 12 |
| Ticks to release W after attacking | 1 | 4 |

One tick is 50 ms. Both bots require less than 2.99 blocks from the eye to the opponent's hitbox, require falling without sprinting for a critical, and check sprint state before making a normal attack.
The selected combination jumps at higher charge, releases W after descent begins, and switches to sprint attacks sooner.
Recovery, airborne movement and attack preparation can override W/S. The distance thresholds alone do not determine direction.

## Search and reproduction

The controller uses only the client's Observation. Server HP and traces are available for post-match scoring. Physics, combat rules and equipment stayed fixed.

1. Generate 4,096 candidates with seed 20261106 and screen each in four standard matches.
2. Compare the top 256 in 144 matches each (36 settings × 2 seats × 2 orders). Play the top 32 against four other opponents for 256 additional matches each.
3. With seed 20261107, change one to three parameters in the leading candidates. Compare 935 candidates, including repeats from the previous stage, in 160 matches each (5 opponents × 8 settings × 2 seats × 2 orders).
4. Compare the top 96 in 720 matches each (5 opponents × 36 settings × 2 seats × 2 orders). Rank by match record and break ties by HP differential. The selected candidate went **690–30**.
5. Freeze the parameters and evaluate under the separate conditions above. Evaluation results did not feed back into tuning.

The search ran 280,160 matches including repeat evaluations. Tracing was disabled, so zero hits/crits/damage in search JSON means unmeasured. Those fields in `evaluation.json` contain measurements.

```powershell
# Run the search, additional tuning and evaluation in order.
.\tools\rust\cargo.bat run --release --locked -p pvp-rl --example critical_tune
.\tools\rust\cargo.bat run --release --locked -p pvp-rl --example critical_tune -- refine
.\tools\rust\cargo.bat run --release --locked -p pvp-rl --example critical_tune -- evaluate

# Record one match between original Critical (A) and Critical+ (B).
.\tools\rust\cargo.bat run --release --locked -p pvp-cli -- duel --bot-a critical --bot-b critical-plus --seed 42 --record build/benchmarks/critical-plus/original-vs-plus.replay.json --trace build/benchmarks/critical-plus/original-vs-plus.trace.ndjson

# Spectate original Critical (Bot A) against Critical+ (Bot B). Press Enter to start.
.\tools\play\start.bat --spectate --opponent critical-plus
```

The selected values are in `CriticalParams::PLUS` in `crates/pvp-policy/src/policy.rs`. The search command writes results to JSON and leaves the selected values in source under manual control.
The results are stored under `build/benchmarks/critical-plus/` (Git ignores build outputs).
`evaluation.json` contains the settings and individual results for 512 matches, plus per-opponent totals.
`original-policy.rs` is the pre-tuning source (SHA-256 `fe8d03495722f7eb7c09efab6cb2740c04193d269e431a6d0f0f2a170cd63703`).

Tests confirmed that the original Critical and the controller with its original parameters choose identical actions, and that Critical+ wins the standard matchup in both seats and both processing orders.
`test --locked --workspace` passed 76 tests and skipped one test requiring a local Minecraft sound cache. Release builds of the simulator and play client also succeeded.
