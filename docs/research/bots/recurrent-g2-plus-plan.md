# recurrent-G2+ GPU training plan

Paths under `build/` refer to generated local artifacts. They are relative to the repository root.

Planned on November 21, 2026. Continue training from the bundled recurrent-G2 and evaluate the resulting candidate against its parent under the corrected Minecraft Java Edition 26.1.2 duel rules. The full campaign has a 24-hour budget for training, selection, and final evaluation. Its deliverables are the candidate weights, a training checkpoint, and the evaluation report.

## Starting model and duel settings

| Input | Verified value |
| --- | --- |
| Parent checkpoint | `build/recurrent-ppo/nextgen-24h/main-71001/candidate-0001677.pt` |
| Checkpoint SHA-256 | `892d18b29da3198475b660f8c8ce605fd2bf7fa2d985148bb145ac79e2c4371b` |
| Parent policy SHA-256 | `13b796c29a0732d6ef10084a8ae525f147c564d0f99616b97132f1929cf582b8` for both the adjacent candidate JSON and `models/bundled/recurrent_g2.json` |
| GPU runtime | AMD Radeon 780M Graphics, PyTorch `2.15.0+rocm10.2.0` |

The parent checkpoint and bundled JSON produced matching Python and Rust logits and GRU states over 64 consecutive inputs. A matrix multiplication on the GPU returned finite values. Initialize the new run with the parent's policy, value function, and observation normalization. Start a fresh Adam optimizer. [G2 provenance](../../../models/bundled/recurrent_g2.source.json)

Use `paper_flat_v2` for training and evaluation, including the corrected airborne knockback. Use `sword_pvp_fixed_equipment_v1` for both, so swords and armor remain unbreakable. Rust simulation and rollout inference run on the CPU. PPO updates run on the Radeon through PyTorch's `cuda` device interface. Use 512 environments, eight Rust threads, two Torch threads, and 2,048 tokens per PPO minibatch. [Knockback verification](../../simulator/knockback-parity.md) · [GPU training guide](next-gen-gru-run.md)

## Run from the repository root in PowerShell

The launcher checks the parent checkpoint and policy hashes, builds the Rust training DLL and evaluator in release mode, and uses the GPU Python environment at `build/rl-gpu-bench-venv`. Keep the original checkpoint, its adjacent JSON, and the historical snapshots in `build/recurrent-ppo/full/seed-61002` available throughout the run. [GPU environment setup](next-gen-gru-run.md#gpu-training-on-the-radeon-780m)

Start the 24-hour experiment with this command. The default output directory is `build/recurrent-ppo/recurrent-g2-plus-24h`. Use `-Output <directory>` to choose another empty directory.

```powershell
.\tools\research\recurrent-ppo\run-g2-plus.bat
```

To continue an interrupted run, use the same output directory and append `-Resume`. Keep the trainer and simulator sources from the original launch so the remaining matches use the same rules. The campaign restores its checkpoints and counts elapsed runtime against the original 24-hour budget. Keep one process per output directory.

```powershell
.\tools\research\recurrent-ppo\run-g2-plus.bat -Resume
```

To repeat the workflow verification, run the Rust regression tests, then use `-Quick` with a fresh output directory. The 32-environment smoke campaign exercises GPU updates, checkpoint resume, inference export, and replay generation. Its `final/summary.json` must report `status=smoke_only` and `accepted=false`.

```powershell
.\tools\rust\cargo.bat test --locked -p pvp-rl
.\tools\research\recurrent-ppo\run-g2-plus.bat -Quick -Output build/recurrent-ppo/recurrent-g2-plus-smoke-repeat
```

## Selection, evaluation, and outputs

Use the standard campaign to compare GRU sequence settings, train main seeds 71001 and 71002 in alternating blocks, and evaluate the fine-tuned branches alongside the best main-training candidates. Confirm the top development candidates on separate seeds. Fix final match counts before main training and freeze the selected candidate's SHA-256 before final evaluation. [Campaign schedule](next-gen-gru-plan.md)

Evaluate G2+ and its parent in the same corrected environment. For direct matches, use all 13 network conditions and swap physical seats while keeping the seed and network assignments fixed. Compare both models against the same 18 common opponents and the historical snapshots. Report the direct win rate and the 95% confidence interval for the win-rate difference, priority-opponent gains at 150 ms, results by network condition, remaining HP, and timeout rates.

Apply the existing acceptance criteria. The direct win-rate difference must have a positive lower confidence bound. At 150 ms, the mean gain against the four priority opponents must reach three percentage points, and each opponent's gain must have a positive lower bound. Apply the one-percentage-point noninferiority margin to the other specified comparison groups. Direct and common-opponent timeout rates must each stay below 1%, and all execution checks must pass. [Evaluation implementation](../../../tools/research/recurrent-ppo/paired_evaluate.py)

Remaining HP was the only criterion the parent missed in its original evaluation. For matched games that both models win against a common opponent, require a mean HP difference of at least zero and a 95% lower bound of at least −0.1 HP. Also require a nonnegative HP difference when losses and timeouts count as zero HP.

Retain the selected candidate's JSON, its matching `.pt` checkpoint, and both parent and candidate hashes under `build/recurrent-ppo/recurrent-g2-plus-24h/`. Use `campaign.json` to locate the selected files, `final/manifest.json` for the frozen comparison, and `final/summary.json` for the decision. If the budget expires before evaluation finishes, preserve the checkpoints and partial results and report `inconclusive_budget` from `campaign.json` with the reason in `inconclusive.json`.

## Implementation verification

- All three `pvp-rl` tests and all 11 Python regression tests passed, including the GPU update and optimizer-state resume test.
- The 32-environment smoke campaign completed 577 evaluation cells and 2,282 matches. Both main seeds resumed from their checkpoints. The final result was `smoke_only` with `accepted=false`. The four reproduced replays recorded `paper_flat_v2` and `sword_pvp_fixed_equipment_v1`. Smoke evaluation (`build/recurrent-ppo/recurrent-g2-plus-smoke/final/summary.json`)
- A separate test at the production size completed two GPU updates across 512 environments with 256 steps per rollout, for 262,144 environment steps. All eight priority cells contributed training tokens, and the exported weights matched Rust inference. Production-size test (`build/recurrent-ppo/recurrent-g2-plus-512-check/training-summary.json`)

## Campaign result

The November 22, 2026 campaign finished in 22 hours and 49 minutes. Candidate `main-71002/candidate-0001222` won 98.98% of head-to-head matches against its parent G2 under the corrected rules. Its mean win-rate gain against common opponents was 7.84 percentage points, rising to 44.38 points across the four priority opponents at 150 ms. The campaign evaluated 1,109,904 matches across 577 cells.

The remaining HP criterion failed. In matched games both models won, the mean HP difference was −0.0995 and the 95% confidence interval was [−0.1074, −0.0917] HP. All other criteria passed. The candidate is available as `recurrent-g2-plus` in the Rust simulator and PvPBotRL. [Bundled model provenance](../../../models/bundled/recurrent_g2_plus.source.json)
