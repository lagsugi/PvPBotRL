# Running the next-generation GRU experiment

This implements the revised plan in `next-gen-gru-plan.md` and continues training from the reigning champion. Promotion and deployment are separate steps. The experiment preserves the existing model distribution and champion.

## Full experiment

Run from the repository root in PowerShell.

```powershell
.\tools\research\recurrent-ppo\run-nextgen.bat
```

The default output directory is `build/recurrent-ppo/nextgen-24h`. The run uses 512 environments, eight Rust threads, four Torch threads, and a cumulative 24-hour limit for the whole experiment. The launcher builds Rust in release mode before starting Python. On a machine without the Python environment, run `tools/research/recurrent-ppo/setup.bat` first.

New campaigns use `sword_pvp_fixed_equipment_v1` for training and evaluation. Resume uses the rules recorded in the campaign. `-FixedEquipment` can require the fixed profile when resuming.

To specify a fresh output directory:

```powershell
.\tools\research\recurrent-ppo\run-nextgen.bat -Output build/recurrent-ppo/nextgen-run2
```

To resume the full experiment:

```powershell
.\tools\research\recurrent-ppo\run-nextgen.bat -Resume
```

Reuse the original output directory, environment count, and other settings. Resume restores the optimizer, RNG state, opponent scores, candidate list, phase, and elapsed time. Ongoing simulator matches are not saved. Resume discards any partial rollout and GRU state, starts new matches, and logs this decision. The time ledger is saved every five seconds. After forced termination, it conservatively charges the last five unsaved seconds. Downtime is excluded. Run only one process against a given output directory.

## GPU training on the Radeon 780M

The Rust simulator and rollout policy run on the CPU. With `-Gpu`, PyTorch trains the policy on the GPU and copies the updated weights back after each rollout. The launcher uses a separate Python 3.12 environment. To prepare the Windows environment tested on the 780M, run these commands from the repository root.

```powershell
uv --cache-dir build/rl-cache venv --python 3.12 build/rl-gpu-bench-venv
uv --cache-dir build/rl-cache pip install --python build/rl-gpu-bench-venv/Scripts/python.exe --index https://stable.repo.amd.com/rocm/whl-next/ 'torch[device-gfx1103]==2.15.0+rocm10.2.0' 'numpy==2.2.6'
```

This example starts a new GPU campaign with the standard 2,048-token PPO minibatch.

```powershell
.\tools\research\recurrent-ppo\run-nextgen.bat -Gpu -TorchThreads 2 -Output build/recurrent-ppo/nextgen-gpu
```

For a higher-throughput experiment, add `-MinibatchTokens 8192` and choose a fresh output directory. This processes the same rollout in fewer Adam steps per epoch and can change playing strength. Repeat the selected flags when resuming a campaign.

## Short verification run

```powershell
.\tools\research\recurrent-ppo\run-nextgen.bat -Quick -Lanes 32 -Output build/recurrent-ppo/nextgen-smoke
```

The short run uses 80 steps per match and a small evaluation to verify that phases work together. Its results do not measure playing strength. It must finish with `accepted: false` and `status: smoke_only`. Use a separate output directory from the full experiment.

Individual regression tests:

```powershell
.\build\rl-venv\Scripts\python.exe tools/research/recurrent-ppo/test_nextgen.py
.\tools\rust\cargo.bat test --locked -p pvp-policy recurrent
```

## Training and selection implemented

- Initialization inherits policy, value, and normalization from `build/recurrent-ppo/full/seed-61002/candidate-0007.pt` and starts a fresh optimizer. In the Python CLI, `--init-checkpoint` initializes a new run and `--resume` resumes the same experiment.
- Validate every `snapshot-*.json` in the same directory, pin hashes, and merge duplicate content. Track the dedicated champion slot, old history, frozen next-generation candidates, and exploiters separately.
- Assign 30% of all matches to the four priority opponents with a 2:1 Both150/Both120 ratio. Historical draws are 30% uniform and 70% biased toward difficult opponents. Use the last 256 matches per opponent and condition, shrinking sparse conditions toward all-condition results.
- Activate the latest eight next-generation models and eight older models with distinct outcomes on fixed development evaluation. Each ongoing match retains a reference to its opponent so registry or active-set updates cannot transfer memory to another model.
- Compare sequence length/burn-in 32/0, 32/16, and 64/16 for 20 minutes each. Train two main seeds for eight hours each, alternating every two hours. Update the exploiter target at the four-hour midpoint.
- Compare three exploiter initializations drawn from the champion and history. Older JSON files lack a critic, so initialize with their policy and normalization plus the champion critic and record the sources. Activate an exploiter only with an outcome score of at least 0.4 and timeout rate below 1%.
- Fine-tune two branches with 30% and 50% priority shares at learning rate 5e-5. Keep the pre-fine-tuning candidate eligible. Recheck the top three development candidates on separate seeds.
- Apply the 0.1 victory HP bonus from the start. Save valid learning tokens, burn-in tokens, KL, gradient norm, and starts, steps, and completed matches per opponent and condition. After 30 minutes, stop if any of the eight priority cells lacks collection or training coverage.

The 44 inputs are listed below. Opponent HP and its age come from received observations. True server HP is used only for reward and result aggregation.

| Indices | Contents |
|---|---|
| 0–2 | Own HP, received opponent HP, attack charge |
| 3–9 | View-relative position, distance, aim, pitch |
| 10–18 | Own velocity, received opponent velocity and displacement |
| 19–24 | Sprint, sneak, and grounded flags for self and opponent |
| 25–29 | Observation ages for position, metadata, velocity, own HP, and opponent HP |
| 30–34 | Sword and armor presence |
| 35–39 | Local jump cooldown, miss cooldown, fall distance, height, time remaining |
| 40–43 | Previously submitted forward/back, strafe, jump, and click actions |

## Final evaluation

Fix match counts before main training and freeze one candidate hash after selection. Direct evaluation uses 13 conditions × 8,192 seat pairs. Compare the candidate and champion against 18 common opponents and every historical model. Bind network conditions to physical seats and swap the candidate's seat with the same seed. A common-opponent comparison pair comprises four matches.

Initial minimums are 512 pairs for common opponents, 8,192 for priority Both150, 2,048 for priority Both120, and 64 for history. Use variance from the short comparison to check noninferiority power for ordinary conditions, high ping, priority Both120, and history. Increase counts as necessary before main training. Shrink small-sample variance toward 0.5 with a floor of 0.1. The 90% power estimate is conditional on this variance and applies to individual tests.

Reserve final evaluation time using measured throughput. If the estimate exceeds 30 minutes, draw from the reserve, then shorten fine-tuning as needed. If time runs out, record an inconclusive decision in `inconclusive.json`. Partial results cannot promote a candidate. End each training loop early enough for saving and development evaluation. The stop check runs once per update, and the final save may add a few seconds.

Bootstrap seat pairs within each cell and aggregate cells with equal weight. Additional priority matches do not increase their weight in the overall mean. Set `accepted: true` only after passing direct champion matches, a three-point mean gain and positive gains in all four priority Both150 cells, noninferiority on ordinary/high-ping/historical opponents, HP, timeout, and execution checks. If there are no mutual wins, mark HP missing and fail the criterion.

Resume an interrupted final test only with the same manifest and candidate. Replacing the candidate and retrying the same final seeds until one passes would invalidate the evaluation.

## Main outputs

| File | Contents |
|---|---|
| `campaign.json` / `budget.json` | Phase, candidates, cumulative time, and time ledger |
| `opponents.json` | Input history count, deduplicated models, paths, and hashes |
| `memory-comparison.json` | Development results and throughput by memory configuration |
| `evaluation-design.json` | Final match counts and estimated duration |
| Per-phase `latest.pt` / `training.jsonl` | Resume checkpoint and progress |
| Per-phase `coverage.json` | Collection and training token counts and sampling scores by opponent and condition |
| `development/` | Development and confirmation matches per candidate |
| `final/manifest.json` | Frozen candidate, conditions, seeds, match counts, and decision thresholds |
| `final/cell-*.jsonl` | Raw match results with seeds and seats |
| `final/summary.json` | Confidence intervals, promotion criteria, and final decision |
| `replay-*.json` | Replay rerun in Rust and checked for identical results |

Candidate JSON uses the existing Rust GRU format. Once execution checks pass, the model can be used by existing Rust match tools. `accepted` records the evaluation result. Deploying a new bundled champion JSON is a separate operation.

## Implementation verification on November 12, 2026

- Nine Python regression tests, three Rust GRU tests, and one seat-swap test passed.
- The short campaign in `build/recurrent-ppo/nextgen-smoke-20261112` completed alternating main-training resumes, the mid-run exploiter, two fine-tuning branches, final evaluation, and Rust replay verification. It took about 7 minutes 47 seconds. Promotion was disabled for this verification run.
- In `build/recurrent-ppo/nextgen-512-validation`, ran two updates each for sequences 32/0, 32/16, and 64/16 with 512 environments × rollout 256, totaling 786,432 steps. Verified valid training tokens in all eight priority cells and agreement with Rust inference for all three configurations. This run verified operation only. It did not measure relative speed or strength.
- The full 24-hour experiment had yet to run.
