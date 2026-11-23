# Rust port contract, training interface, and verification plan

Source analysis identifies the required state. The type names, action representation, and initial settings below are **proposed project designs**, distinct from the Minecraft internal API.

## 1. Minimum architecture

```text
Duel
  rules / simulation_clock / event_queue / per_match_rng
  Server
    ServerPlayerState[2]
    packet_processing_order / tracker_state
  Client[2]
    LocalPlayerState
    RemotePlayerReplica
    input_state / attack_clicks / receive_frame_clock
  NetworkLink[2]
    upstream / downstream / sequence / virtual_delay
```

A match holds server-side state for two players plus two clients, each with a local player and a replica of the opponent. Build observations for training from client state. Reserve authoritative server state for ground truth, reward, and verification.

| State group | Examples to retain | Purpose |
| --- | --- | --- |
| Server player | position, yaw/pitch, AABB/Pose, onGround, fallDistance, deltaMovement, lastKnownClientMovement | Reach, critical hits, knockback, and movement validation read different values |
| Server combat | health, attackStrengthTicker, lastHurt, invulnerableTime, hurtMarked, needsSync, sprinting, gear durability | Damage branches, velocity send and restore |
| Local player | position, velocity, yaw/pitch, Pose, crouching, onGround, fallDistance, cached speed, jumping, noJumpDelay | Physics and input order |
| Local control | Current and previous input, sprintTriggerTime, wasSprinting, lastSentInput, missTime, click events | Transient Ctrl, Shift, and click states |
| Local send state | Last sent position, rotation, ground and collision flags, positionReminder | Packet omission conditions |
| Remote replica | Received position baseline, displayed and target positions, remaining interpolation steps, velocity interpolation, metadata, ground state | Legal observations and aim target |
| Server tracker | Position codec baseline, sent rotation and velocity, tickCount, teleportDelay, wasOnGround, receivedMovementThisTick | Two-tick cadence, exceptional sends, ClientTickEnd handling |
| Scheduler | Virtual time, tick number, frame phases, send/arrival queues, per-connection sequence | Deterministic replay |

Keep internal state separate from the subset visible to the bot.

## 2. Fixed conditions and provisional starting profile

| Condition | Treatment |
| --- | --- |
| Vanilla 26.1.2 equivalent | Pin this conformance target. Other 1.9+ versions can differ |
| Survival, 1v1, 20 HP each | PvP enabled, no friendly-fire restriction, both clients fully loaded |
| Specified diamond sword and full Protection IV | Use fixed equipment slots without generic inventory management |
| No natural healing or hunger loss | Omit exhaustion and fix food at a value that permits sprinting |
| No pushing | Disable player pushing on both clients and server. Keep attack AABBs |
| Flat ground | Grass-like friction 0.6F, floor at 0, no obstacles or ledges. Retain floor collision and raycasts |
| 20 ms ping | Initially 20 ms RTT = 10 up + 10 down, without jitter or loss |
| Starting positions and facing | Proposed distance 6 on the same floor, facing each other. This is a project choice |
| Initial velocity and timers | Prepare stationary players with full post-equip charge, zero invulnerability, and full HP |
| Durability | Start with new gear and finite durability. If equipment must stay fixed, explicitly enable unbreakable on both sides |
| Client settings | Auto-jump off, no GUI, fixed mouse/click injection points. Save FPS, sprintWindow, and key-toggle settings |
| Time limit | Configurable for training batches. Death remains the victory condition |

Return `truncated` for timeout and `terminated` for termination by death. The requested rules do not award timeout wins by HP difference. Bound world-border exposure using match duration and maximum travel, and document the infinite plane as an explicit approximation.

## 3. Action space

Translate the requested controls into inputs the game can execute.

| Action | Recommended meaning |
| --- | --- |
| View mode | `AimAtObservedTarget` or `FreeLook` |
| AimAtObservedTarget | Calculate deterministic yaw/pitch from the client's opponent AABB |
| FreeLook | Output target yaw/pitch or deltas, allowing the player to look away |
| Attack | Click event separate from button hold, expanded into press/release if needed |
| W/A/S/D | Independent key states, with opposing keys cancelling on the same axis as in Minecraft |
| Ctrl / Space / Shift | Held key states processed through game logic before sprinting, jumping, or Pose changes |

Aiming exclusively at the head or center can miss when the near face of the same AABB is reachable. The initial proposal picks a reachable interior point from the eye to avoid ambiguities exactly on a face. Verify the actual raycast hits. Expose a center-aim alternative as an aim-module setting independent of the learned policy.

After rotating the view, always use the client's normal raycast to select a target. Direct opponent IDs would allow attacks while facing away or within the server's six-block tolerance beyond legitimate client reach.

Start with decisions at 20 Hz and schedule input for the next applicable client processing stage. Record the exact key, view, and click hooks in the verification profile. Keep physics ticks independent of any future change in decision frequency.

## 4. Observation space

Define named features with a schema version and availability mask before fixing a vector length. Proposed observations depend on what the client can actually access.

| Candidate | Representation |
| --- | --- |
| Own HP | Last received own HP. Exclude damage the client has yet to receive |
| Own velocity | LocalPlayer velocity, in blocks/tick |
| Attack cooldown | Derived from client attackStrengthTicker, distinct from server cooldown |
| Own sprint/sneak/onGround | Local state. Expose key state separately from effective state where needed |
| Own walking/rising/falling | Derive using explicit definitions from input, horizontal speed, grounding, vy, etc. |
| Relative opponent position | Offset from own eye or feet to interpolated opponent position/AABB |
| Opponent velocity | Received velocity or an estimate from successive observations. Label the estimate accordingly |
| Opponent sprint/sneak/onGround | Latest received metadata/position packets, with age |
| Opponent walking/jumping/falling | Estimate from replica motion and received grounded flag. Opponent Space input is hidden |
| Distance between players | Distinguish center distance, horizontal distance, minimum eye-to-AABB distance, and other definitions |

Free look also makes own yaw/pitch or view-relative bearing (sin/cos), previous action, and current raycast hit eligibility useful candidates. Positions alone leave the meaning of movement actions ambiguous when facing direction is hidden.

Consider time since receipt, estimates of remaining interpolation steps, and recent history. Delays make a single observation insufficient to reconstruct complete server state. Compare history stacking and recurrent models in a later phase.

Keep opponent server cooldown, unreceived positions and HP, and internal RNG out of actor inputs. Evaluate any full-state research policy under a separate oracle mode. If the critic or reward uses server information, isolate it from the actor.

## 5. Reward and parallelism boundaries

Keep reward outside physics and networking. Avoid hidden changes to friction, speed, reach, or damage to discourage running away.

An initial candidate combines a primary win/loss reward with small auxiliary terms for net damage and lack of progress. Rewarding distance every tick can incentivize repeated approaches and retreats. A safe retreat in response to a feint should remain viable. Reward weights and comparisons of PPO or other algorithms fall outside Phase 1.

**Parallelize Rust across matches.** Each Duel owns its state, RNG, and event queue. Process attacks sequentially within a match. Exclude shared RNGs, wall clocks, and mutable global state that make outcomes depend on thread count. Verify identical results with the same seed and input sequence before and after parallelization.

Separate single-match `reset / step / observe / trace` from batched operations at the FFI/training boundary. Consider SoA, SIMD, batch inference, and reduced allocation after verifying correctness. The analysis alone establishes no speedup factor.

## 6. Verification performed

[Phase1Probe.java](../../../tools/research/minecraft/Phase1Probe.java) loads Minecraft classes and bundled libraries extracted from the SHA-1-verified official server bundle. It initializes registries and default components without starting a game server or world.

**33 checks passed.** They covered:

- Diamond sword attack attributes, durability, and absence of custom reach or minimum charge.
- Full diamond armor with armor=20, toughness=8, knockback resistance=0, and each piece's durability.
- CombatRules mitigation for normal hits, critical hits, and raw damage differences.
- Boundaries at n=10–13 in a Java charge formula reconstructed from source.
- Official LpVec3 round trips for zero, ordinary knockback, small velocity, and extended scale.
- Official AABB ray intersections, the Vec3.closerThan boundary at 3.0, and VecDeltaCodec quantization at 4096.

These static numerical and data checks cover selected functions. Full Player.attack execution, live packet timing, complete movement, RNG, and parallel performance require other tests. See scope in the [results JSON](evidence/probe-results.json).

## 7. Differential verification for the next phase

Use the existing Fabric project as an **instrumentation harness** to collect event traces from official 26.1.2 clients and server. Record inputs and arrival order and replay that sequence in Rust. Self-play win rates cannot establish simulator correctness.

| ID | Case | Comparison or boundary |
| --- | --- | --- |
| D01 | Stationary, fully charged normal hit | Raw, post-armor, post-Protection, HP, timers, transmitted knockback |
| D02 | Attack at n=0,1,5,10,11,12,13 | q, strong flag, float bits, cooldown reset order |
| D03 | Attack on ascent, at apex, in descent, and at landing | fallDistance, onGround, crit. Sprint prevents a crit even while falling |
| D04 | Weaker/equal/stronger attack at invulnerableTime=11,10,9 | Rejection, difference, full damage, knockback, timer reset |
| D05 | Hits 7 then 10.5 within protected interval | Raw difference 3.5 gives about 0.2961 after mitigation |
| K01 | Hit while grounded/airborne with nonzero velocity | Halving, vertical cap, position direction vs. yaw direction |
| K02 | Strong/weak/rejected sprint hit | Extra knockback, own 0.6 slowdown, sprint cancellation, differences from local prediction |
| K03 | Incremental hit during invulnerability plus sprint | No normal knockback, extra knockback and hurtMarked/needsSync paths |
| K04 | Coincident positions and tiny offsets | Random-direction threshold, RNG consumption, reproducibility |
| M01 | W/A/S/D, opposing keys, diagonals | Normalization, square correction, yaw transform, steady-state speed |
| M02 | Ctrl transitions, W-tapping, attacking while holding Ctrl | Current movement-speed attribute via `Player.getSpeed()`; same-tick acceleration and start/stop packet order. See the Phase 2 correction in `movement.md`. |
| M03 | Hold/release/press Space again | noJumpDelay, takeoff friction, initial jump speed |
| M04 | Toggle Shift | Old/new input, crouching, Pose, AABB, eye height |
| M05 | Tiny velocity, landing, stationary on floor | Horizontal norm threshold, residual negative vy, fallDistance reset |
| M06 | Fall from near height 4 | floor plus 1e-6 formula, armor bypass, Protection |
| H01 | Eye-to-intersection 2.999999/3/3.000001 | Client reach boundary, AABB width vs. center distance |
| H02 | Server eye-to-AABB just below/above 6 | Inclusive upper bound of server AttackRange |
| H03 | Sneaking, airborne, overlapping fully, looking at floor | Containment, ray misses, floor occlusion, float dimensions |
| N01 | Move then Attack and Attack then Move | Last accepted position, crit, yaw, sprint |
| N02 | RTT 0/20/40, just before/at/after boundary | Server acceptance tick, knockback onset, observation delay |
| N03 | Mutual attacks on same tick, one lethal | Global queue order, hasClientLoaded rejection after death |
| N04 | updateInterval boundary and ground-state change | Relative/full position sync, metadata send, three-step interpolation |
| N06 | ClientTickEnd without movement packet | knownMovement resets to zero separately from ServerPlayer's internal 0.98 velocity decay |
| N05 | MISS, BLOCK, ENTITY, held/repeated clicks | missTime, diverging server/client cooldowns |
| E01 | Armor at durability 1 when hit, weapon at durability 1 | Timing of attribute and Protection changes at breakage |
| P01 | Same seed/input on one and multiple threads | Identical events, termination reason, and final state |

Record match seed, client settings, virtual/measured time, side, frame/tick, connection/sequence, event type, position, velocity, view, AABB, Pose, onGround, fallDistance, sprint, cached speed, attackStrengthTicker, invulnerableTime, lastHurt, raw/mitigated damage and HP, hurtMarked/needsSync, payload before sending and after codec decoding, random values, and termination reason.

First require exact agreement on event order, packet types, branches, and discrete states. Then compare float bits for identical numerical functions and error over long movement traces. Derive tolerances from reference quantization and arithmetic differences. An arbitrary tolerance such as 1e-3 can hide a divergent branch. Stop at the first branch mismatch before differences compound into a different outcome.

## 8. Open questions and Phase 1 completion

Source analysis identified the main combat, movement, and network paths. Resolve the following as implementation or verification settings.

| Item | Status or next step |
| --- | --- |
| Live server type | Vanilla 26.1.2 is the reference. Analyze the exact version and plugins separately for a Paper or other target |
| Ping definition | Initial proposal is 20 ms RTT. Store upstream and downstream separately in the profile |
| FPS, tick phase, simultaneous arrival order | Make configurable and record in reference traces. Ping alone cannot determine them |
| Input injection point and sprintWindow | Pin reference client settings and hooks for the port |
| Gear durability | Initially finite. Explicitly select unbreakable for permanently fixed attributes |
| Omitting position corrections | Decide after testing legitimate controls with knockback and delay |
| RNG agreement | Choose between injecting overlap-knockback draws and reproducing the RNG consumption sequence |
| Performance targets | After measuring an accurate single match, set matches/s, steps/s, and parallel scaling targets |

Phase 1 produced a version-pinned port specification with verifiable evidence. Phase 2 can begin, but **live agreement at every boundary remains unproven**. Start by collecting reference traces and implementing Rust conformance for pure CombatRules, codecs, and flat-ground physics.
