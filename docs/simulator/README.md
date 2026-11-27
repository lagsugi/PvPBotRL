# Phase 2: Rust Sword PvP simulator

The workspace now contains a virtual-time 1v1 environment library in [`crates/pvp-sim`](../../crates/pvp-sim). It implements the combat profile researched in [Phase 1](../research/minecraft/README.md), a client/server state model, parallel environments, configurable observations, and action replay. The `pvp-cli` package provides the `pvp-sim` executable.

**Minecraft equivalence has not been established. No Minecraft process, Java probe, or Gradle task was run during Phase 2.** Rust tests use source-derived expectations and the numeric results already saved in Phase 1. The implementation and its explicit boundaries are described in [implementation.md](implementation.md); recorded checks are in [validation.md](validation.md).

## Run on this Windows workspace

A workspace-local Rust toolchain is available under `.tools/` (ignored by Git). The wrapper uses it when present and otherwise uses Cargo from `PATH`. It does not modify the user's persistent PATH. A normal Rust installation also works; this workspace was tested with Rust 1.99.1, Windows GNU.

```powershell
# Format, Clippy, and Rust tests. No Minecraft is started.
.\tools\rust\check.bat

# Run a scripted baseline duel and save its actions and packet trace.
.\tools\rust\cargo.bat run --release --locked -- duel --seed 42 --record build/phase2/duel.json --trace build/phase2/duel.ndjson

# Replay the recorded actions, including their original configuration and seed.
.\tools\rust\cargo.bat run --release --locked -- replay --input build/phase2/duel.json

# Run independent complete matches in a persistent worker pool.
.\tools\rust\cargo.bat run --release --locked -- batch --matches 1000 --threads 4 --seed 42

# Demonstrate vector stepping, observation projection, and explicit lane resets.
.\tools\rust\cargo.bat run --release --locked -p pvp-sim --example vector_env
```

On other platforms, use `cargo` in place of `.\tools\rust\cargo.bat`. No Java dependency is needed for the Rust environment. `Cargo.lock` fixes dependency versions. After dependencies are cached, Cargo accepts `--offline`. Use `--` to forward flags to tools, for example `clippy --workspace --all-targets -- -D warnings`.

The CLI prints JSON. `--trace` writes NDJSON and `--record` writes a versioned JSON action replay. Output files at explicitly supplied paths are overwritten. Use a separate file for the trace. The CLI rejects a trace that refers to the same file as the replay input or the duel recording. Generated output belongs under the ignored `build/phase2/` directory. Use `pvp-sim --help` for all options. A replay may intentionally end before death.

## Supported match profile

| Setting | Default |
| --- | --- |
| Combat target | Minecraft Java Edition 26.1.2 source behavior |
| Players | Two, survival movement, 20 HP each |
| Equipment | Diamond sword; four diamond armor pieces with Protection IV |
| Durability | Fixed for the sword and all four armor pieces |
| Floor | Infinite, immutable horizontal plane at Y = 0; friction 0.6 |
| Initial locations | (0, 0, -3) and (0, 0, 3), facing each other |
| Movement and server rate | 20 ticks/second |
| Client receive frames | 60 frames/second per client |
| Network | Fixed 10 ms upstream + 10 ms downstream per client: RTT 20 ms |
| Recovery, hunger, contact pushing | Disabled/omitted |
| Initialization | Fully loaded, synchronized clients; full attack charge |
| Decision step | 50 ms of virtual time |
| Episode limit | 6,000 decisions (300 virtual seconds) |
| Victory | First server-confirmed death ends the episode |
| Time limit | Truncation, with no health-based winner |
| Reward | +1 winner / -1 loser; optional damage and time terms |

The floor still participates in vertical collision, fall damage, and ray occlusion. There is no block grid, destruction, terrain generation, or player contact pushing. Equipment is represented by fixed modifiers and durability counters.

Configuration is JSON with strict field names and defaults for omitted top-level fields. See [`configs/duel.json`](../../configs/duel.json). `--config` is available for `duel` and `batch`. `--max-steps` overrides its episode limit. FPS, one-way delays, tick/frame phases, seat priority, floor height/friction, spawn states, rewards, and trace capacity are configurable.

## Fixed-equipment tournament and training profile

Use `sword_pvp_fixed_equipment_v1` for new Sword PvP matches, training, and evaluation.

| Rule | Behavior |
| --- | --- |
| Armor | Both players wear full Protection IV diamond armor. Keep armor durability fixed throughout the match. |
| Weapon | Both players use an unbreakable diamond sword. |
| Natural regeneration | Disabled for both players. Keep existing match time limits and outcome rules. |

Finite durability makes the rules change mid-match when a piece breaks. Losing armor changes mitigation, and losing the sword changes attack damage and timing. Wear also depends on earlier exchanges, while the recurrent policy sees equipment presence without the remaining durability counters. Fixing equipment keeps a long match under the same rules as a short one and makes seed-paired evaluations easier to interpret. Disabling natural regeneration removes the health recovered by disengaging. Track timeouts and match duration separately.

CLI duels and batches apply this profile to new matches, including when a configuration file supplies other match settings. For a standalone preset, use `--config configs/sword_pvp_fixed_equipment_v1.json`. Replays store the effective profile. Historical recordings replay with their saved rules.

New GRU campaigns, competitive evaluations, and replay generation use fixed equipment by default. Resuming a historical campaign follows its saved rules. Use a fresh output directory for a new campaign. [Next-generation GRU guide](../research/bots/next-gen-gru-run.md)

On Paper, the match host equips both fighters with the five unbreakable items. PvPBotRL checks both loadouts and requires natural regeneration to be off in the arena world. [Paper setup](../../paper/RLBOT_SETUP.md#fixed-equipment-profile)

Before adopting the profile, verify in an isolated match that the sword and armor show no wear after accepted hits and HP does not regenerate while idle after damage. Use the same seeds and swapped seats to evaluate existing models under the new rules. Identify historical rules from saved configurations and model provenance. [Historical measurements](paper-parity.md)

## Learning interface

```rust
use pvp_sim::{Action, Config, Duel, Feature, ObservationSchema};

let mut env = Duel::new(Config::default(), 42)?;
let schema = ObservationSchema {
    version: 1,
    features: vec![Feature::Health, Feature::AttackCharge, Feature::RelativeZ],
};
let inputs = schema.project(&env.observe(0)?)?;
let transition = env.step([Action::default(), Action::default()])?;
// Feed inputs to a policy; store transition before any reset.
# Ok::<(), pvp_sim::SimError>(())
```

`Duel::reset(seed)` reinitializes all state and queues. `Duel::clone()` checkpoints the complete in-memory environment, including pending packets, RNG, and clocks. `Batch::step` advances independent lanes in parallel and preserves lane order. It validates all actions before mutating any lane; terminal lanes require explicit `reset_at`. See the executable [vector environment example](../../crates/pvp-sim/examples/vector_env.rs).

Actions have six independent movement keys: forward, backward, left, right, jump, and sneak. Both seats hold the sprint key automatically. The client evaluates sprint eligibility each tick. Releasing forward for a tick and pressing it again lets a player reset sprint after a sprint hit. Opposite movement keys cancel. `click` queues one attack press and `attack_held` controls held-button interactions. These rules apply to both single and batched environments. The default spawn separation is 20 blocks.

New human transition files (metadata format 2) omit Ctrl from the policy decisions. The version 1 replay format stores physical keys including Ctrl. Historical recordings replay with their recorded spawn coordinates and individual sprint inputs.

The view action supports `track_target`, `keep`, absolute `angles`, and relative `turn`. Tracking computes an aim point inside the **observed opponent AABB**, before the next local pick. It never reads current server coordinates. A policy can deliberately turn away and strafe. Yaw uses Minecraft conventions: 0 faces +Z, +90 faces -X; positive pitch looks down. Pitch is clamped to [-90, 90].

Observations contain local HP, movement, charge and equipment presence; observed opponent relative position, motion and pose; distances, crosshair hit eligibility, and receive ages. They exclude opponent HP, opponent attack charge, current server state, and in-flight packets. `opponent_velocity` follows received motion packets; because walking does not necessarily generate those velocities, `opponent_displacement` separately measures position interpolation over the last client tick. Opponent walking/rising/falling indicators use this observed displacement.

`ObservationSchema` selects and orders named features. The default contains 42 values. Save the schema with a trained policy; do not assume a fixed input layout. HP is divided by 20, pitch by 90, ages are seconds, position is blocks, and motion is blocks/tick. Other booleans are 0 or 1; yaw uses sine and cosine. Positions and velocities are not otherwise clipped. `snapshot()` is a privileged diagnostic interface and must not be passed to actors.

The environment is partially observable. Episode termination and training rewards use authoritative server events. Normal observations remain delayed, even in a terminal transition. Both `duel` and `batch` default to the original baseline. Select a matchup with `--bot-a` and `--bot-b`, for example `duel --bot-a league-g6 --bot-b league-g5`. The available styles are baseline, pressure, orbit, counter, critical, critical-plus (Critical+), duel-rl, league-rl (G4), league-g5 and league-g6. See the [League-G6 results at 0–150 ms](../research/bots/league-g6.md), the [League-G5 and G4 results](../research/bots/league-rl.md), the [Critical+ tuning results](../research/bots/critical-plus.md) and the [Duel-RL training and evaluation](../research/bots/duel-rl.md).

Run `cargo run --release -p pvp-rl --example bot_league` for a round robin. Each distinct pair plays four fixed-start matches at seed 42, covering both seat assignments and both same-time client processing orders. The table reports wins, timeouts, accepted sprint hits and critical hits. These controlled matchups measure the included scripts against one another.

Optional reward shaping is:

```text
reward[p] = damage_weight * (opponent_HP_lost - own_HP_lost)
            - time_penalty
            + terminal_win_or_loss
```

It introduces no arena wall or artificial combat force. Reward coefficients are configurable; the defaults do not establish that a learned policy will avoid stalling. That behavior needs training and evaluation. A timeout is exposed separately from death so a trainer can choose its truncation/bootstrap policy.

## Paper Citizens Bot vs Bot

Use `configs/paper_citizens_practice.json` to reproduce the normal PvPBotRL paired NPC path on the flat Practice opening with fixed equipment. Run from the repository root in PowerShell.

```powershell
.\tools\rust\cargo.bat run --release --locked -- duel --config configs/paper_citizens_practice.json --bot-a g3-prototype --bot-b recurrent-g2-plus --record build/paper-citizens/duel.json
.\tools\rust\cargo.bat run --release --locked -- replay --input build/paper-citizens/duel.json
```

The preset starts at 0 ms modeled RTT. For Paper's default 20 ms, pass `--ping-profile fixed --ping-a 20 --ping-b 20`. Change the bot order to swap physical seats. `batch` uses the same driver and seed handling as `duel`.

Select this mode in Rust with `Config::duel_driver = DuelDriver::PaperCitizens` and call `Duel::step_with_policy` for closed-loop decisions. The lead seat alternates each server tick. A 0 ms attack can immediately change the second bot's own velocity and HP. Movement and tracker updates follow both decisions. Subsequent decisions consume the received tracker updates, including interpolation, metadata and motion packet quantization. The next decision gives actors their own current NPC state and received opponent information. FPS and virtual client phases belong to the client/server driver. `Duel::step` and `Batch::step` accept fixed inputs for recorded or scripted runs without mid-tick controller calls.

The stored replay includes its driver. A config or replay with no driver field uses `client_server`, the driver used by the original training and evaluation. The Paper preset covers flat-floor Bot vs Bot physics. The open-floor model excludes arena walls and other entities.

The regression fixture from Paper 26.1.2 build 74 and Citizens build 4270 checks every chosen action and accepted attack for G3 Prototype vs G2+ at symmetric 0 and 2 ms in both seats. All four action sequences and all 180 accepted hits match their Paper counterparts, including the hit tick, attacker and critical classification. At 20 ms the fixture checks the winner, terminal tick and HP in both seats. Final HP error was at most `0.000002 HP`. Paper reports a zero-based terminal tick, so Rust's `decision_steps` is one greater. [Paper fixture](../../tests/fixtures/parity/g3-paper-citizens.json) · [Regression test](../../crates/pvp-policy/tests/paper_citizens.rs)

## Module map

| Module | Responsibility |
| --- | --- |
| `math`, `rng` | Minecraft-style floating-point helpers, AABB clipping, reproducible randomness |
| `state`, `config` | Local, server, remote replica and tracker state; validated settings |
| `physics` | Input shaping, sprint, sneak, jumping, gravity and plane collision |
| `combat` | Charge, critical hits, armor, Protection, hurt immunity and knockback |
| `network` | Packet queues, motion packing and position/rotation quantization |
| `sim` | Virtual clocks, phase ordering, packet handlers and episode lifecycle |
| `observation`, `action` | Actor-visible data, feature schemas and controls |
| `batch` | Persistent Rayon worker pool, lane stepping and whole-match execution |
| `trace` | Bounded diagnostics and portable action replay |
| `policy`, CLI | Baseline demonstrations and throughput measurements |
