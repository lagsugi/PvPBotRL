# Training a Recurrent-PPO bot from scratch

Created November 8, 2026. Implementation details and commands follow.

## Running the bot

Run from the repository root in Windows PowerShell. The promoted model is bundled with the Rust build.

```powershell
# Play against the bot. Press Enter to start.
.\tools\play\start.bat --opponent recurrent-ppo

# Watch a match against G6.
.\tools\play\start.bat --bot-a league-g6 --opponent recurrent-ppo
```

Use `uv` and Python 3.12 to reproduce training and evaluation.

```powershell
.\tools\research\recurrent-ppo\setup.bat

# Verify training, evaluation, and replay with the short configuration.
& .\build\rl-venv\Scripts\python.exe -B tools/research/recurrent-ppo/experiment.py build/recurrent-ppo/check --quick

# Run the full CPU experiment. Supply an empty output directory.
& .\build\rl-venv\Scripts\python.exe -B tools/research/recurrent-ppo/experiment.py build/recurrent-ppo/full
```

The full run writes the candidate model path to `selected.json`, win rate, remaining HP and promotion decision to `final/summary.json`, and per-match results to `final/trials.json`. Comparison summaries go in `comparisons.json` and each seed's training history in `training.jsonl`. The short configuration verifies operation with different seeds and fewer matches.

For one seed, run `train.py OUTPUT_DIR`. To resume, use the same directory and seed and add `--resume OUTPUT_DIR/latest.pt`. `--seconds` is the total budget including time before resuming. Use checkpoints produced by your own training run. Run standalone evaluation with `evaluate.py MODEL.json EVAL_OUTPUT_DIR`. Use `--anchor` to compare against the model from before the HP improvement phase.

Use `--recurrent-model FILE` in the CLI or match UI to load different weights. Omitting it selects the bundled model.

```powershell
.\tools\rust\cargo.bat run --release --locked -p pvp-cli -- duel --bot-a recurrent-ppo --bot-b league-g6 --recurrent-model build/recurrent-ppo/seed-61001/last.json --record build/recurrent-ppo/duel.json
.\tools\play\start.bat --bot-a league-g6 --opponent recurrent-ppo --recurrent-model build/recurrent-ppo/seed-61001/last.json
```

## Bundled model

The bundle includes `candidate-0007.json` from seed `61002`, which passed promotion on November 9, 2026. It won 98.2% of 9,984 matches against G6 and 98.5% against 17 common opponents, compared with G6's 85.1%. SHA-256: `cc78c1ec607ceb39ce0909548083b9d7f10454b73a28cac1b1059291f7407eb1`. [Bundled weights](../../../models/bundled/recurrent_ppo.json)

## Approach

Build a **GRU neural policy with memory, trained using PPO**, independently of G6's linear policy, ARS, and handcrafted short-term memory. Previous bots serve as opponents. Training starts without inherited weights or imitation. [PPO paper](https://arxiv.org/abs/1707.06347)

Target the current Rust simulator. Keep auto-aim, reach, equipment, 20 Hz decisions, and continuous sprint input consistent. Budget roughly 12 hours on a local PC. Rank win rate first and remaining HP on wins second. Paper integration is a later phase.

## Policy and training

- **Network.** A 64-unit fully connected layer and a 64-unit GRU. Shared memory feeds the action and state-value outputs.
- **Inputs.** Forty current observation features plus four previous submitted action components give 44 inputs, including received opponent HP. Omit stacked observations, handcrafted moving averages, configured RTT, opponent identity, unreceived server state, and reward.
- **Outputs.** Three forward/backward choices, three strafe choices, jump on/off, and click on/off. Allow jump when grounded and click within the existing reach condition. Let the policy learn to approach at a distance.
- **Learning.** Recurrent PPO samples actions stochastically. Defaults going forward are 512 environments × 256 steps, sequence length 32, four epochs, learning rate `3e-4`, discount `1.0`, GAE coefficient `0.95`, and PPO clip `0.2`. Compute return to the terminal state of this finite-horizon match. Evaluation and play use the most probable choice per output.
- **Reward.** A win earns `+1`. Losses and timeouts earn `−1`. Wins alone earn up to `+0.1` for remaining HP fraction. Use the terminal-corrected potential difference below for dense feedback and omit the previous damage-difference bonus and time penalty.
- **Opponents.** Spend the first 10% of training time on fixed bots and Duel-RL. Then allocate 50% to existing bots, historical generations, and exploiters, 25% to G6, and 25% to frozen checkpoints of the new bot. Save self-play models every 50 updates and retain the latest 16.
- **Environment.** Reuse G6's 13 network conditions and its variation in starting positions, FPS, phases, and seats. Allocate 50% of training to mixed ping and split the remainder equally across the other 12 conditions. Reserve Reach and Orbit/Pressure for final evaluation.

## Reward for winning first and increasing remaining HP (revised November 8, 2026)

### Limitations of the previous proposal and revised objective

The old damage-difference reward already values remaining HP. With both sides starting at 20 HP, no healing, and no discount, the total for a win is approximately `1 + 0.12 × remaining HP − 0.0001 × steps`. An additional HP reward counts the same outcome twice. At 20 Hz, a 0.995 discount weights a terminal reward 30 seconds later by only about 0.049, which can undervalue a slower, reliable win with HP to spare.

Use the following terminal reward with `h = clamp(own server HP when victory is confirmed / own initial HP, 0, 1)`.

| Result | Terminal reward |
| --- | --- |
| Win | `1 + β × h` |
| Loss | `−1` |
| Timeout at match limit | `−1` (no remaining-HP bonus) |

- In each four-hour run, spend the first two hours at `β=0` focusing on win rate and the second two at `β=0.1` also improving HP. Restore policy, value function, and optimizer from the best selected first-half checkpoint. When changing the HP reward, discard ongoing matches and untrained rollout data, start new matches, and reset GRU state and previous action.
- From 20 starting HP, winning with 1, 10, or 20 HP yields 1.005, 1.05, or 1.10 respectively. Survival alone earns no periodic reward. Evading until timeout earns −1.
- Measure terminal HP from the server when victory is confirmed. Only reward and evaluation may use it. The policy continues to see delayed opponent HP, even if the terminal observation has yet to update.
- Set discount to 1.0 and time penalty to zero. The objective places no premium on winning quickly. Retain the 6,000-decision limit (maximum 300 seconds).
- A single expected reward with a finite HP bonus can trade win rate for HP. Apply win-rate-first selection at promotion as described below. There is no theoretical guarantee against a drop in win rate.

### Dense reward and terminal handling

To help training from scratch, use `Φ(s) = 0.25 × (own HP fraction − opponent HP fraction)` in ordinary states, `Φ=0` at a terminal state, and add `Φ(s_next) − Φ(s)` at each step. Divide each player's HP by their own initial HP and clamp to [0, 1]. Both fractions start at 1, so initial Φ is zero. Reward computation may use server values. Keep Φ, reward, and true HP out of network inputs.

Over a complete match, the potential differences sum to `Φ(terminal) − Φ(start) = 0`. Damage provides intermediate feedback, while total reward equals the outcome plus HP reward above. Include the `−Φ(s)` correction on the last step for both death and timeout. [Original paper on potential-based reward shaping](https://people.eecs.berkeley.edu/~pabbeel/cs287-fa09/readings/NgHaradaRussell-shaping-ICML1999.pdf)

A match-limit timeout is a terminal non-win. The existing API may retain `truncated=true`, but the learner sets next-state value to zero and stops GAE propagation as for death. At a 256-step rollout cutoff the match continues. Bootstrap the next-state value and preserve memory, without terminal reward or terminal potential correction. A 32-step training sequence boundary also leaves the game running. Budget expiry and manual interruption have separate meanings from a match timeout.

Compute the new reward in a new training environment wrapper. Preserve the meaning of `RewardConfig` and the G1–G6 reproduction commands. Avoid adding the old `StepResult.rewards` a second time. Record the reward version, β, Φ coefficient, discount, and timeout handling in the model and training logs.

## Input review and revised specification (November 8, 2026)

### Findings

The 42 existing features are a general-purpose observation API. Their suitability for the new policy was unproven. The decisions below follow a static implementation review and still require empirical comparison.

| Feature | Decision and rationale |
| --- | --- |
| World position and velocity, absolute yaw | World coordinates require learning the mapping to forward and strafe actions. Transform horizontal components into the player's view frame and drop absolute yaw sine/cosine. |
| Own and opponent walking/rising/falling | Derivable from velocity, observed displacement, and grounded state. Keep on_ground because vertical velocity can be negative while grounded. |
| distance | Derivable from relative position. Retain horizontal_distance for approach and eye_to_target for attack checks. |
| opponent_velocity and opponent_displacement | The former comes from a motion packet, the latter from one local tick of render interpolation. Motion velocity can remain zero while walking. Keep both and do not divide displacement again by the decision interval. |
| target_under_crosshair and pitch | Decision and click processing times differ even with auto-aim. Clicks can also hit the floor. Keep both. |
| Age fields | Each measures time since the last receipt/application. It increases even when no update arrives because the value is unchanged. It measures neither RTT nor time since server-side generation. Retain and document this meaning. |
| Equipment presence | Keep all five own-equipment fields because default durability settings allow breakage. |
| Opponent HP | Add it to help estimate remaining damage to a kill and favorable trades. The existing Observation and Replica lack this field, so delivery needs a new path. |
| Local control state | jump_delay and miss_time constrain jumps and clicks. Add them with own fall distance and height above the floor. Keep opponent internal timers private. |
| Time remaining | Finite matches require decisions about timeouts. Derive from the public match limit and elapsed time. |

### New policy inputs (44 dimensions)

Fix model input order to the table below. Each vector uses forward, left, up. The horizontal forward axis follows current yaw, and left is its perpendicular according to simulator conventions. Use the current angle before subsequent auto-aim.

| Group | Features in order | Dimensions |
| --- | --- | ---: |
| HP and attack | Own HP, received opponent HP, attack_charge | 3 |
| Range and aim | Three view-relative position components, horizontal distance, eye_to_target, crosshair flag, pitch | 7 |
| Motion | View-relative own velocity, opponent motion velocity, opponent interpolated displacement (three components each) | 9 |
| State | Own sprinting, sneaking, on_ground, then the same three for the opponent | 6 |
| Update ages | Time since receipt of position, metadata, velocity, own HP, opponent HP | 5 |
| Equipment | Presence of own sword, helmet, chestplate, leggings, and boots | 5 |
| Control, fall, and time | jump_delay, miss_time, own fall_distance, own height above floor, fraction of match time remaining | 5 |
| Previously submitted action | Longitudinal axis (back −1, stop 0, forward +1), strafe (right −1, stop 0, left +1), jump (0/1), click (0/1) | 4 |

Validate the implemented model dimensions against this list as well as the handwritten dimension constant.

Use fixed normalization throughout training. Divide HP by 20, positions, distances, and heights by 8, pitch by 90, and jump_delay/miss_time by 10. Keep velocities and interpolated displacement in blocks/tick. Leave attack_charge, time fraction, and flags unchanged. Convert ages to seconds and apply `log1p(min(age, 10)) / log(11)`. Use `log1p(min(distance, 64)) / log(65)` for fall_distance. Clip transformed continuous values to ±8 and reject nonfinite values. Save the transform version and coefficients with the model. Measure saturation rates and make any revisions before final evaluation.

### Roles of GRU and previous action

The GRU reads successive observations and retains relevant information without stacking multiple observation frames. The actual action chosen at the previous step supplies additional information. For recurrent control using both observation and action history under partial observability, see [Memory-based control with recurrent neural networks](https://arxiv.org/abs/1512.04455)

Processing order:

1. Concatenate current observation `o_t` and previous action `a_(t−1)`, pass through the fully connected layer and GRU, and update memory `h_t`.
2. Produce action distributions from `h_t` and then sample action `a_t`.
3. Submit the selected, mask-valid Action and save its four components for the next input.

Sampling occurs after the GRU update. Without action feedback, memory directly represents the selection probabilities but lacks the actual sampled choice. Delays and unsuccessful inputs can make the choice impossible to reconstruct from the next position and velocity. Keep the previous action in the main model. Other recurrent architectures can incorporate chosen actions differently.

Replace the earlier ten-dimensional one-hot encoding and history-valid flag with four natural control axes/buttons. The previous action records what was submitted after masking, independent of whether the jump or attack succeeded. Set previous action and GRU state to zero at match start. The runner uses `episode_start` to trigger initialization outside the network inputs.

Preserve chronological order during training. Save the GRU state and previous action at the beginning of a sequence and the match-end masks. State persists across individual samples. The 32-step limit applies to gradient propagation. Inference memory continues past it. Compare against a 40-dimensional GRU without previous action to assess its contribution.

### Opponent HP delivery and compatibility

- Add an explicit opponent-HP observation profile. Enable it for the new bot's training, evaluation, and play. Leave it disabled by default for old configurations. This disclosure belongs to the experimental environment and makes no claim about HP availability on live Minecraft.
- Add `OpponentHealth { entity, health }` as an independent downstream message. On the same health-change detection used for own HP, send the update to the opponent's receiving side. Apply receiver-side downstream delay and frame processing while preserving existing metadata delivery conditions and frequency.
- Store opponent HP and a dedicated receive timestamp in Replica. Add `opponent_health: Option<f32>` and `opponent_health_age_ns: Option<u64>` to Observation. Use None when disclosure is disabled or old recordings omit them. The new policy requires both to be Some. Missing HP must never be interpreted as full health.
- At synchronized spawn, initialize from `Config.initial_health[opponent_seat]` only if disclosure is enabled. Support starting HP other than 20 and repeat the initialization on reset.
- Death ends the environment immediately, so observed opponent HP at termination can still exceed zero. Use Outcome for victory. Neither learner nor inference may correct observations with true snapshot HP.
- Preserve the existing 42-field v1 schema and old model input order. Give the new policy its own schema and defaults for reading older recordings. Verify that enabling opponent HP disclosure preserves old bots' outcomes, actions, and physics.
- G6 does not use opponent HP. Report improvements against G6 as the combined effect of observation, reward, and training changes. Use the comparison without opponent HP to estimate the observation's contribution separately.

## Implementation and interfaces

- **Training infrastructure.** Pin Python 3.12 and CPU PyTorch dependencies in a dedicated environment. Batch-step Rust environments through a separate crate's C ABI and `ctypes`. Preserve the simulator's prohibition on `unsafe`.
- **Environment API.** Support creation, per-environment configured reset, batched stepping, and destruction. Return observations and rewards, distinguishing death from timeout. Preserve terminal observations across reset.
- **Memory management.** Isolate memory and previous action per environment and seat. Reset at death and match-limit timeout and set next-state value to zero. Carry state across rollout/sequence boundaries and use the continuing state's value.
- **Model format.** Store version, observation order, fixed normalization coefficients, required HP profile, network architecture, action definitions, and weights in JSON. Match PyTorch's GRU equations in Rust.
- **Entry points.** Provide commands for training, resuming, exporting models, and independent evaluation. Add promoted models as `recurrent-ppo` in the CLI, match UI, spectator mode, and recording.
- **Artifacts.** Save candidates, resume checkpoints, experiment settings, seeds, learning curves, matchup summaries, and representative replays. Resume training state and start fresh matches.

## Evaluation and promotion criteria

Separate training, checkpoint-selection, and final-evaluation seeds. Select every 30 minutes by maximizing score S = 40% G6 win rate + 40% mean common-opponent win rate + 20% mean win rate over the lowest 10% of opponent × network cells. Break ties with mean `win indicator × remaining HP fraction` across all common-opponent matches. If still tied, retain the earlier model. Never add HP to S or accept a lower S for more HP. These priorities apply to selection matches and cannot guarantee win-rate retention on unseen opponents. Exclude evaluation-only opponents from selection too.

Save the best model from the first two hours and keep it eligible in the HP phase. If it remains the final candidate, record that the experiment did not establish an HP improvement at retained win rate.

Freeze the selected model for final evaluation and run the following comparisons.

- Against G6, play 768 matches under each of 13 conditions.
- Against 17 common opponents, play 256 matches per condition across 13 conditions. Compare the new bot with G6 under the same initial conditions.
- Count timeouts as non-wins. Report Wilson intervals per condition and 95% confidence intervals for aggregate results and win-rate differences using bootstrap stratified by network condition.
- Report mean, median, and 10th percentile of remaining HP on wins, mean `win indicator × remaining HP` over all matches, and match duration. Always report win rate alongside winning HP. Losing difficult matches can raise mean HP on wins.
- For common-opponent trials that both models won under identical conditions, report paired remaining-HP differences and a 95% confidence interval. Mark comparisons with no mutual wins as unavailable.
- To evaluate the HP phase, additionally test the best first-half model from the candidate's seed lineage against all 17 common opponents under Mixed, 256 matches each. Compare with the final candidate under identical conditions. Report win rate and HP difference on mutual wins. This before/after comparison alone cannot establish a causal effect of the HP reward.

**Promotion requires every criterion below.**

- Overall win rate against G6 is at least 60% and its lower 95% confidence bound exceeds 50%.
- Mean common-opponent win rate exceeds G6's and the lower 95% bound on the difference exceeds zero.
- Mean common-opponent win rate under each network condition falls at most five percentage points below G6.
- The timeout rate across final evaluation is below 1%.

If criteria fail, retain results and candidate and record that promotion failed. Report win-rate qualification and HP improvement separately. Confirm HP improvement only if Mixed win rate holds relative to the first-half model and the lower 95% bound on HP difference in mutual wins exceeds zero. Otherwise report uncertainty or unconfirmed improvement. Keep final results out of further tuning on the same evaluation set.

## Verification and time allocation

- **Correctness.** Check sequential Rust/PyTorch inference agreement, memory isolation and reset, action constraints, terminal and timeout value calculations, model save/load, and batch/single execution agreement. Run existing Rust checks as well.
- **Learning checks.** In a short trial, inspect gradients, reward, action distribution, and termination rate for NaNs or collapse onto a single action.
- **Inputs and HP.** Under zero and asymmetric delays, check that HP changes only after receipt and that its dedicated age updates. Cover non-20 starting HP, seat swaps, unreceived HP immediately after death, resets, missing fields in old recordings, and rejection of missing new-policy inputs. Test coordinate transforms at known yaws, left/right directions, and overlapping positions. Check equipment breakage, cooldowns, fixed normalization, and the order of all 44 inputs. World-axis-aligned physics AABBs preclude requiring identical outcomes under arbitrary rotations.
- **Reward.** Test wins with 1, 10, and 20 HP, normalization at other initial HP, and absence of bonuses for losses or high-HP timeouts. For matching outcomes and terminal HP, cumulative rewards must agree across different damage orders and match lengths. Check that potential differences sum to zero for deaths and timeouts, terminal HP is measured on the server, and value bootstrapping and potential correction distinguish actual termination from rollout boundaries.
- **Memory.** Verify the actual action sampled after the GRU update enters the next input, masked actions cannot enter, and state stays isolated across environments, seats, and matches. With fixed weights, continuous inference must match inference split into 32-step chunks with state carried across.
- **12-hour experiment budget.** Allow 30 minutes for benchmarking and a pilot, four hours for each of two independent seeds, 30 minutes for each of four comparisons, and 90 minutes for final evaluation. Include intermediate evaluation in each training budget.
- **Comparisons.** Use the four alternatives below, matching the main model's first seed, opponent distribution, and first-half reward (β=0). Compare at 30 minutes elapsed and report steps processed. End the initial curriculum at 24 minutes as in the main run. Keep the specified 44-dimensional GRU as the main model regardless of short-run results. A short single-seed comparison is diagnostic and cannot establish optimality or final superiority.

| Comparison model | Inputs | Question |
| --- | ---: | --- |
| Existing observations + GRU | Existing 42 + opponent HP and age + four previous actions = 48 dimensions | Effect of the overall input redesign |
| GRU without opponent HP | Remove opponent HP and its age from the main model, leaving 42 dimensions | Contribution of the opponent HP observation |
| GRU without previous action | 40 current-observation dimensions | Contribution of feeding the submitted action back to the GRU |
| No GRU | Same 44 inputs, replacing GRU with a 64-unit fully connected layer | Contribution of recurrent memory. Previous action still supplies one step of history |

For the existing observation design, apply the existing projection to its original 42 fields, normalize using training mean and standard deviation, and freeze those statistics at evaluation. Transform added HP, age, and previous action as in the main model. This comparison measures the combined input design, including coordinate transforms, feature changes, and normalization. It cannot isolate the individual changes.

- **Assumptions.** The 12-hour budget starts after implementation and dependency setup. Defaults are 512 environments, eight Rust threads, and four PyTorch threads. Both standalone training and the full experiment accept `--lanes`. Benchmark the pilot at the requested parallelism. The experiment that produced the promoted model used 256 environments. Explicitly report unfinished work or unmet criteria.
