# Phase 2 verification record

Date: 2026-11-05 (Asia/Tokyo). Toolchain: Rust 1.99.1, `x86_64-pc-windows-gnu`. Machine and full measurements are recorded in [`validation-results.json`](validation-results.json).

**Only Rust verification was executed.** No Minecraft client/server, Gradle task, or Java probe was run. Existing Phase 1 numeric observations were reused as fixed expectations. These results do not establish Minecraft equivalence; the remaining boundaries are documented in [implementation.md](implementation.md).

## Checks executed

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| Debug tests, all targets | 48 passed: 39 unit tests + 9 integration tests |
| Release tests, all targets | Same 48 passed |
| Release binary build | Passed |
| CLI duel, seed 42, sample configuration | Seat 0 won; 619 decisions; death at 30.9 virtual seconds |
| JSON action replay | Same winner, health and positions |
| NDJSON trace replay | Byte-identical SHA-256; no trace records dropped |
| Vector environment example | 16 lanes, 42 default features, explicit resets completed |
| Whole-match batch, 1 and 4 worker threads | Both completed 1,000 matches, zero truncations, wins [513, 487] |

Trace SHA-256 for both the original and replay:

```text
4ed7beca92d528891127330256d848b3eca5ddd65ff7343f680385864aba3f54
```

Generated CLI artifacts are local and ignored by Git: `build/phase2/duel.json`, `build/phase2/duel.ndjson`, and `build/phase2/replay.ndjson`. The summary JSON in this documentation directory is retained. The integration replay test additionally compares the full diagnostic snapshot and trace in memory after JSON serialization/deserialization with asymmetric FPS, phases, and delays.

## Test coverage

- Damage float-bit fixtures, charge boundaries, armor and Protection, incremental raw damage during immunity, critical hits, sprint exclusion, and break ordering.
- Ground/air knockback, sequential sprint knockback, protected incremental sprint hits, quantized motion, server velocity restoration, and overlap RNG reproducibility.
- Opposing keys, diagonal/sneak input, jump/landing, ground velocity, fall-damage rounding, sprint persistence, and same-tick sprint acceleration through the `Player.getSpeed()` override.
- Strict client reach, AABB endpoint behavior, deliberate off-target views, floor occlusion, and Java rounding conventions.
- Network FIFO/cross-connection arrival ordering, LpVec3 fixtures and malformed payloads, and relative coordinate quantization.
- Delayed attack application, delayed own HP, client miss versus server cooldown, attacks preceding new sprint commands, death ordering, metadata-driven tracker updates, and truncation.
- Actor observations remaining unchanged when hidden server state changes; remote walking inferred from interpolation; custom feature ordering and validation.
- Full checkpoint cloning, reset isolation, bounded traces, atomic invalid-action rejection, worker-count determinism, and full JSON action replay.
- Timing matrix at 20/37/60/144 FPS, zero through 230 ms one-way delay, and nonzero tick phases while exercising all keys and view modes.
- Floor interaction release timing, falling from height with armor bypass and Protection, and baseline completion by death.

The speed-override regression covers a correction discovered during Phase 2 source review. The previous Phase 1 explanation treated the inherited speed cache as the player's movement source; the actual `Player.getSpeed()` override reads the current attribute. The corrected source reasoning is recorded in [the Phase 1 movement document](../research/minecraft/movement.md).

## Throughput sample

Command: `pvp-sim batch --matches 1000 --threads N --seed 42`, release build, default profile, no trace, scripted baseline. Each match ended after 619 decisions. Compilation and pool/environment construction are outside the CLI's timed region; simulation, observation construction, and baseline decisions are inside it. Neural-network inference and training are absent.

| Workers | Wall seconds | Matches/second | Decisions/second |
| --- | ---: | ---: | ---: |
| 1 | 1.0151 | 985 | 609,788 |
| 4 | 0.2917 | 3,428 | 2,121,927 |

These are individual measurements on the recorded machine, not a stable benchmark distribution or a promised speedup over Minecraft. Different policies, traces, equipment failures, timing profiles and inference costs change throughput. The CLI's `virtual_seconds_per_wall_second` uses nominal decision count multiplied by 0.05; a terminal decision may process less than its full interval.

## Deferred work

Minecraft packet/state differential tests, cross-platform Java/Rust numerical identity, real inventory and movement-correction edge cases, and trained-policy evaluation are intentionally unverified. The current suite validates the Rust implementation's internal contracts and recorded scalar expectations. Future Minecraft comparisons should use the Phase 1 case matrix and the explicit scheduling/input profile in the Phase 2 implementation contract.
