# Next-generation GRU training plan

Revised November 12, 2026. Continue training from the reigning champion in 512 environments for up to 24 hours. Rank candidates by win rate first and remaining HP on wins second. Focus on league training, compare memory configurations in a short trial, and train against known high-ping weaknesses throughout the main run.

This is an implementation and experiment plan. The allocations, thresholds, and learning rates are proposals whose effects have yet to be tested. The 24-hour limit covers elapsed training and evaluation time after implementation and verification. Budget implementation time separately. Use reproducible promotion criteria against the champion, existing bots, historical league models, and high-ping opponents. These comparisons cannot establish absolute playing strength.

## 1. Corrections and established facts

- The previous proposal's 13 conditions × 256 seat pairs lacked power to detect small gains. Use statistical power to set match counts.
- The previous direct evaluation ran 768 matches × 13 conditions = 9,984 matches. The earlier proposal reduced this to 6,656. The previous opponent was G6. This experiment faces the reigning champion.
- The current `crates/pvp-rl/src/eval.rs` derives seats from the seed. Implement explicit seat-swapped pairs and head-to-head matches between different GRUs. Update the match-level aggregation in Python as well.
- The evaluator currently limits `anchor` comparisons to Mixed. Compare against the champion on common opponents in every condition.
- The user measured approximately two minutes for final evaluation at the old scale. Benchmark GRU head-to-head throughput at the new scale before setting the budget. The old budget was 2 hours 30 minutes.
- In the Both150 experiment, the model won 182/256 against League-G2 (71.1%), 206/256 against Critical+ (80.5%), 214/256 against League-G3 (83.6%), and 218/256 against League-RL (85.2%). These are the four priority opponents. The earlier reference to G4 was incorrect.
- The champion originates from `build/recurrent-ppo/full/seed-61002/candidate-0007.pt`. The same directory contains 110 `snapshot-*.json` files.
- The local champion checkpoint has SHA-256 `9fa3392bb732e92dfb9118ca5958c5d90a0f99ca8520ef29191c17bc1e61a2cc`. The checkpoint lives outside Git. Reproducing this training start from a fresh checkout still requires a documented way to obtain or regenerate it. Verify the resulting file against this hash before comparing runs.
- PPO updates mix samples across conditions. “About 15 updates per opponent” is an equivalent allocation. Measure actual steps, completed matches, and valid training tokens by opponent and network condition.

## 2. The 24-hour budget

| Phase | Limit | Work |
|---|---:|---|
| Baseline, throughput, and variance | 0.5 hours | Re-evaluate the champion, fix evaluation counts, and check throughput with 512 environments |
| Short memory comparison | 1 hour | Compare behavior, learning trends, and throughput across three configurations, 20 minutes each |
| Exploiters | 2 hours | One hour initially and one hour after updating the target midway |
| Main training | 16 hours | Two independent seeds, eight hours each, with high-ping training from the start |
| Fine-tuning | 3 hours | Two 1.5-hour branches from the candidate chosen in development evaluation |
| Final evaluation and Rust verification | 0.5 hours | Evaluate one frozen candidate |
| Reserve | 1 hour | Evaluation overruns, restarts, and additional measurements |
| Total | 24 hours | Includes development evaluation, saving, and loading within each phase |

Alternate between the two main training seeds in two-hour blocks. When each has accumulated four hours, update the mid-run exploiter and introduce it in the second half. Budget for sequential execution of the CPU-intensive runs. Decide whether to spend unused reserve on training before starting final evaluation.

If measured throughput puts final evaluation over 30 minutes, use reserve time first, then shorten fine-tuning if needed. Fix match counts and promotion criteria before viewing final results. Record the decision as inconclusive if required verification cannot finish within 24 hours.

## 3. League and high-ping training distribution

Sample in two stages at the start of each main-training match. Percentages refer to match starts.

### 3.1 Allocate 30% of all matches to priority conditions

Within this allocation, draw Both150 two-thirds of the time and Both120 one-third. For either condition, sample League-G2, Critical+, League-G3, and League-RL uniformly. This explicitly allocates 20% of all matches to Both150 and 10% to Both120 throughout training.

The original distribution adds further high-ping matches. Longer matches contribute a larger share of steps, so record actual step counts too. In the first 30 minutes, verify that all eight cells (four opponents × two conditions) consistently produce samples.

### 3.2 Draw the remaining 70% from the league

| Opponent | Share of league allocation | Share of all matches |
|---|---:|---:|
| Frozen reigning champion | 30% | 21% |
| Champion's 110 historical models | 20% | 14% |
| Frozen next-generation models | 10% | 7% |
| Exploiters | 20% | 14% |
| G6 and existing training bots | 20% | 14% |

Use the original network distribution for this 70% allocation. Mixed accounts for 50%, and each of the other 12 conditions accounts for 1/24. This retains ordinary, asymmetric, and mixed-ping matches. Save realized opponent and network draws separately.

Split the existing-bot allocation evenly between G6 and the 15 previous training bots. Hold Reach and Orbit/Pressure out of training again to check generalization in development and final evaluation. Their previous results are known, so these opponents are only partially unseen.

### 3.3 Use the 110 historical models

- Validate format, input dimensions, finite values, and hashes for all 110. Merge identical hashes and fold any champion duplicate into the champion's dedicated allocation. Freeze the input list and hashes.
- Use uniform sampling for 30% of historical draws and prioritize difficult opponents for 70%. Use weights `(1 - s)^2` and initialize the smoothed score `s` at 0.5 for unplayed opponents. Fall back to uniform sampling if all weights are zero.
- Track scores per training seed. Keep the last 256 completed matches per opponent and condition to limit the influence of old results. Score wins as 1, losses as 0, and timeouts as 0.5, with a Beta(1,1)-equivalent prior. Use this score only for sampling. Reward and final win rate have separate definitions.
- Draw the network condition first and use scores for that condition to draw the opponent. Shrink sparse cells toward the same opponent's all-condition score. Cap each model at 10% of the historical allocation and redistribute the excess among the others.
- Keep introducing next-generation models and exploiters to discover new counters. Test how much tactical diversity the 110 models from a single training lineage actually provide.
- Normally retain the latest eight next-generation models and eight older ones. Share frozen snapshots from both seeds. Avoid redundant neighboring snapshots and retain models with distinct head-to-head outcomes. Until history is available, assign its 7% share to champion matches.
- Expand Rust's 16 fixed slots. Associate model IDs with weight hashes and fix the opponent at match start. Ongoing matches retain their model through an update. Never transfer GRU state between models.
- Checkpoint the model registry, draw statistics, RNG state, and elapsed time for each seed. Each environment only needs inference for its active opponent, even with 110 models registered.

Mixing historical opponents and exploiters draws on AlphaStar's league training. The proportions above are experimental settings for this environment. [AlphaStar paper](https://storage.googleapis.com/deepmind-media/research/alphastar/AlphaStar_unformatted.pdf)

### 3.4 Update the exploiter midway

Branch the initial exploiter from the champion and train for one hour against the champion (80%) and existing bots (20%). After four hours on each main seed, freeze the provisional best development candidate and use it as the mid-run exploiter's primary target for one hour. Compare the champion and promising historical initializations in short matches and choose one that shows potential against the target.

Allocate 25% of exploiter matches to Both150, 25% to Both120, and 50% to the original distribution. Retain both exploiters. If one loses nearly all development matches against its target, split its share equally between champion and next-generation history until it demonstrates strength. The initial operating threshold is a match outcome score of at least 0.4 against the target. Also inspect win and timeout rates separately.

## 4. Revisit GRU and PPO settings

### 4.1 Short comparison

Train each configuration below for 20 minutes from the same champion checkpoint with the same league, rewards, and priority distribution. Use this single-seed comparison to choose a configuration within the 24-hour budget. It offers limited evidence about performance in other settings.

| Configuration | Gradient sequence length | Burn-in |
|---|---:|---:|
| A | 32 | 0 |
| B | 32 | 16 |
| C | 64 | 16 |

For burn-in, replay immediately preceding observations and submitted actions with current weights to update hidden state without a loss. Start from the saved hidden state, respect match boundaries, and retain the required prefix across rollout boundaries. Finite burn-in may reduce hidden-state staleness without eliminating it.

The current code reuses the hidden state from collection over four update epochs. Check for drift after weight changes, reset at match boundaries, and verify the sequence padding mask. R2D2 discusses stale recurrent state. Measure its effect in this PPO setup. [R2D2 authors' page](https://willdabney.com/publication/r2d2/)

Use 512 environments × a 256-step rollout and aim for roughly 2,048 valid training tokens per minibatch, excluding burn-in and padding. Report actual tokens and compute time. Compare development performance under the same 20-minute wall-clock budget even if length 64 is slower. If win-rate differences are within measurement error, favor the shorter configuration on throughput and stability.

GRU state persists throughout a match. Sequence length 32 allows memory to carry beyond 1.6 seconds. Start with 44 inputs, including received opponent HP, observation ages, and the previous action. Only add stacked observations if separately justified.

### 4.2 Training stability and objective alignment

- Inherit the policy, value function, and input normalization from the champion's `.pt`. Start fresh optimizer and match statistics. Use `--resume` to resume and `--init-checkpoint` for initialization.
- Start with learning rate `1e-4`, at most four epochs, clipping 0.2, gradient-norm limit 0.5, gamma 1, and GAE 0.95. Prioritize training volume on known weaknesses before exploring multiple learning rates and entropy settings.
- Aggregate KL, clip fraction, entropy, value error, and gradient norm over valid tokens. Fix the current KL display, which only reports the last minibatch. Use the nonnegative approximate KL `mean(exp(log_ratio) - 1 - log_ratio)` and stop subsequent epochs when the epoch mean exceeds 0.015. Check this threshold in the short run.
- Start with +1 for a win, −1 for a loss or timeout, up to +0.1 for the remaining HP fraction on a win, and a potential difference with terminal correction. This weighted sum alone cannot strictly prioritize expected win rate. Require candidates to meet win-rate criteria first during selection and promotion.
- Apply reward changes at match boundaries. Optimize on trajectories recently collected by the policy being updated. Use saved older models as opponents.
- Training samples actions stochastically. Production Rust selects the most probable action. Use the deterministic production Rust policy for candidate selection and final evaluation.
- Start the 512-environment throughput benchmark with eight Rust threads and four Torch threads. Use a monotonic clock and include model export, development evaluation, and checkpointing in each phase's budget.

## 5. Main training, fine-tuning, and candidate selection

Train two independent seeds for eight hours each from the same champion and selected GRU configuration. Share only frozen league models. Keep optimizer, RNG, and results independent. Record both win rate and HP. Save and evaluate a candidate every 30 minutes within each phase's budget.

Use a small fixed development suite and compare leading candidates on a confirmation suite with separate seeds. Include the champion, the four priority opponents, ordinary conditions, and historical opponents. Require the trend to recur on the confirmation suite.

Retain candidates with small regressions in ordinary conditions and against common opponents. Compare their win-rate margins against the champion and both mean and lowest win rates against the four priority opponents. Use HP to break unresolved win-rate ties. Keep the best intermediate checkpoint eligible alongside the final update.

Fine-tune two branches from the same selected checkpoint for 1.5 hours each. Use a 30% priority share in one and 50% in the other. Keep the priority opponent distribution and Both150/Both120 ratio fixed. Use learning rate `5e-5` for both and document the changes. Retain the pre-fine-tuning candidate. Choose one model that improves weaknesses while retaining ordinary performance in development, then freeze its weights and hash before final evaluation.

## 6. Evaluation units, sample sizes, and power

### 6.1 Use seat pairs as independent units

A head-to-head seat pair consists of two matches with one independently drawn initial-condition seed, swapping the candidate and champion between left and right. In asymmetric conditions, keep network parameters attached to seats and swap models. Save every result, seat, condition, and seed.

Score each match +1 for a candidate win, −1 for a champion win, and 0 for timeouts or other draws. Let `D` be the two-match average. The mean of `D` with equal weighting across the 13 conditions estimates candidate win rate minus champion win rate. Apply a separate timeout limit.

For common opponents, the candidate and champion each play the same opponent with the same seed and seat, then swap seats. Each comparison pair comprises four matches. If a seed is reused across conditions, cluster the entire seed. By default generate independent seeds per cell.

Stratify confidence intervals by condition and opponent and resample whole seat pairs. Counting the two matches as independent would double the apparent sample size. Calculate common-opponent averages with prespecified equal cell weights so oversampling priority opponents preserves the overall weighting.

### 6.2 Target a one-percentage-point head-to-head difference

For the previous proposal's 3,328 pairs, assuming independent, closely matched games within a pair and few draws gives `SD(D)=sqrt(0.5)`. The approximate 95% interval half-width is 2.40 points. The probability of detecting a positive difference is about 12.6% for a true one-point difference and 37.1% for two points, consistent with the user's observation. Within-pair correlation changes these values.

The revised head-to-head design schedules **13 conditions × 8,192 seat pairs = 106,496 pairs, or 212,992 matches**.

| Assumption | Approximate 95% interval half-width | Power at a true one-point win-rate difference |
|---|---:|---:|
| Independent, evenly matched games within a pair, SD ≈ 0.707 | 0.425 points | 99.6% |
| Conservative approximation using SD 1 because D ranges from −1 to 1 | 0.601 points | 90.4% |

These normal-approximation values guide the design. Actual intervals may differ. With no draws, a one-point difference corresponds to a 50.5% candidate win rate. This power calculation applies only to the head-to-head test. Passing every promotion criterion is a separate event.

For common opponents, variance depends on how often candidate and champion outcomes disagree for the same seed. Estimate paired variance and power from baseline measurements and development candidates, then fix minimum sample sizes before main training. [Stata documentation on power for paired proportions](https://www.stata.com/manuals/pss-2powerpairedproportions.pdf)

### 6.3 Initial sample sizes for other evaluations

| Evaluation | Initial sample size | Purpose |
|---|---|---|
| Common opponents | 18 existing bots × 13 conditions × 512 comparison pairs | Retention overall, under ordinary conditions, and at high ping |
| Four priority opponents at Both150 | 8,192 comparison pairs each | Required weakness improvement test. Replaces the corresponding common-opponent cells |
| Four priority opponents at Both120 | 2,048 comparison pairs each | Check for regressions in a neighboring condition. Also replaces the corresponding cells |
| Champion history | All deduplicated models × Mixed/Both120/Both150 × 64 comparison pairs each | Detect forgetting across history |

With up to 110 historical models, these evaluations and direct matches total about 924,000 games. Measure throughput including GRU vs. GRU, fixed bots, JSON output, and aggregation. The previous two-minute measurement was at a different scale. Resample in chunks to bound memory while retaining raw matches.

Size the initial priority cells for a true three-point improvement in each cell. Use initial variance estimates to check power for the common-opponent −1-point noninferiority margin and for priority-cell gains. Fix the required counts before viewing final seeds. The 64 historical pairs per model support an aggregate history decision. They are insufficient to establish small differences for individual models.

## 7. Promotion criteria

Require every condition below. Win-rate differences are in percentage points. Freeze one final candidate before evaluation and evaluate it once. Selecting another candidate after viewing results would reuse the final test.

| Metric | Requirement |
|---|---|
| Head-to-head vs. reigning champion | Lower 95% bound on candidate win rate minus champion win rate exceeds zero |
| Four priority opponents at Both150 | Equally weighted mean gain of at least three points, with lower 95% bound above zero for the mean and each of the four individual improvements |
| All common opponents | Lower 95% bound on the win-rate difference from the champion is at least −1 point |
| Ordinary conditions | Same lower bound of at least −1 point for the equal-weight mean over the 11 conditions excluding Both120/Both150. Fix this definition because Mixed can include high ping |
| All high-ping conditions | Same lower bound of at least −1 point for the mean across 18 common opponents × Both120/Both150 |
| Four priority opponents at Both120 | Same lower bound of at least −1 point for the four-opponent mean |
| All historical opponents | Same lower bound of at least −1 point for the equal-weight mean across three conditions × all historical models |
| HP | Across common-opponent matches both models won, the estimated HP difference is at least zero and its lower 95% bound at least −0.1 HP. The estimated difference in win-weighted HP across all matches must also be at least zero |
| Timeouts | Below 1% in both direct and common-opponent matches. Report timeout increases separately for each of the four priority cells |
| Execution quality | Pass Rust matches, inference agreement, memory reset, resume, and replay verification |

Fix the proposed margins before starting the experiment: +3 points for the priority mean, −1 point for noninferiority, and −0.1 HP. Demonstrating strictly positive gains for both win rate and HP on every measure would require more matches. For HP, require a nonnegative point estimate and a small noninferiority margin. A separate claim of improved HP requires the lower 95% bound on the HP difference to exceed zero.

HP in mutual wins is a selective metric that excludes matches where the outcome changed. Also report win-weighted HP, win rate, and the 10th percentile of remaining HP on wins to expose improvements confined to easy matches.

Use an intersection–union promotion test requiring every hypothesis to pass. Passing each criterion does not confer 95% simultaneous coverage on the intervals. For exploratory claims selected from many cells, adjust for multiple comparisons, for example with Holm's method. Check margins in development because joint power is lower than power for an individual test.

On failure, retain the model and results and report which criteria fell short. Before reading final results, freeze the seeds, conditions, match counts, candidate hash, and aggregation specification in a manifest. Keep the final evaluation independent of subsequent promotion attempts.

## 8. Implementation and short-run completion criteria

1. Implement matches between distinct GRUs, explicit seats and seeds, 13 conditions, and champion comparisons in every condition. Use symmetric matches to check model A/B assignment.
2. For each of the 44 inputs, verify received information, observation timestamp, and mask. Check that unreceived HP and server internals cannot reach the policy and that Python and Rust agree on opponent HP and its age.
3. Verify independent management of 110 historical models, champion, exploiters, and next-generation history. Check opponent pinning during matches, statistics restoration, and initialization from older checkpoint formats.
4. Check burn-in, match boundaries, padding, previous actions, terminal values, and timeouts. If ongoing environment state cannot be restored after interruption, log this, discard partial rollouts and match state, and resume consistently from new matches.
5. Complete a short run that verifies priority-cell counts, phase timing, KL aggregation, and candidate-selection metrics.
6. Test seat-pair correlation, equal cell weighting, priority-cell replacement, missing HP, draws, timeouts, and duplicate seeds in evaluation aggregation. Check intervals and decisions on synthetic data with zero, positive, and negative differences.
7. Start the 24-hour experiment after the short run succeeds. Save phase settings, model hashes, match results, and the promotion decision.
