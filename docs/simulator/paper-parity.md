# Paper parity verification: implementation and measurements on November 18, 2026

Paths under `build/` refer to generated local artifacts. They are relative to the repository root.

**Full-match parity across all three systems has yet to pass.** Fixed movement and state updates driven by identical received packets passed under the conditions below.

This report records historical measurements from several test builds. The measurements come from the original development history, including commit `40c8a29`. For individual trials, use the recorded runtime and configuration hashes in the evidence.

The fixed observation ages and snapshot observations below describe earlier builds. The current Paper bot receives and interpolates the opponent's native tracker packets, with ages measured from application of those updates. [Current observation implementation](../../paper/rlbot/src/main/java/dev/sugi/pvpbot/rl/OpponentReplica.java)

The durability results below cover the finite-durability configuration. Use the named profile to test unbreakable equipment separately. [Fixed-equipment profile](README.md#fixed-equipment-tournament-and-training-profile)

The November 21 follow-up measured outgoing motion on Paper at takeoff, during ascent, and during descent, and switched new Rust training and evaluation episodes to `paper_flat_v2`. [Knockback investigation and follow-up tests](knockback-parity.md)

### Follow-up on November 20, 2026: fixed-equipment verification

An isolated Paper server ran one charged hit in a player-versus-player duel and one in a player-versus-bot duel with `sword_pvp_fixed_equipment_v1`. Across both probes, the swords and all four armor pieces retained their starting durability. The damaged player's HP stayed at approximately 19.3196 for 68 and 69 samples respectively (about 3.4 seconds). The bot round journal recorded the profile. The harness ended each capture after sampling. Local evidence is under `build/parity/fixed-equipment-00404ad7/`, including `summary.json` and `runtime-sha256.json`.

These probes checked wear and short-term HP behavior. Deployment on the regular server, full-match parity, and model strength under these rules remain separate verification steps. For research, complete the moving-target tests first, repeat them across the directional delay matrix below, then compare existing models with the same rules, seeds, opponents, and swapped seats. Record match duration and timeouts alongside wins to measure disengagement under the no-regeneration rule.

## Sword PvP rules and implementation

The target is 1v1 sword PvP on a flat floor. Each player starts with 20 HP, a diamond sword, and full Protection IV diamond armor. The intended rules fix equipment throughout the match and exclude wear and breakage. Hunger changes, natural regeneration, and contact pushing are disabled. Shared controls are forward/back/strafe, sprint, jump, aim, and attack. The policy cannot choose sneak. Reach, charge, critical hits, knockback, receipt of opponent information, and attack application all affect decisions transferred to live play.

Compare the saved Rust environment and Paper deployment against these rules. The kit and arena below describe the earlier test host. For current standalone captures, use the [Paper parity runner](../../tools/parity/README.md#reproduce) and record starting positions, match limits, and network conditions.

| Item | Verified behavior | Comparison with intended rules |
| --- | --- | --- |
| Equipment durability | Rust defaults and four saved next-generation GRU replays use `unbreakable: false`. The Paper kit validator used for these tests rejected unbreakable items. | This conflicts with the fixed-equipment intent. Retain historical finite-durability breakage tests. The effect of breakage on existing learned models is unmeasured. [Rust defaults](../../crates/pvp-sim/src/config.rs) · Example recording (`build/recurrent-ppo/nextgen-24h/replay-0.json`) · [Paper kit validation](../../paper/rlbot/src/main/java/dev/sugi/pvpbot/rl/RlOpponent.java) |
| Sneak | Rust's general controls and observations include sneak, but Recurrent PPO/G2 cannot output it. Four saved next-generation GRU replays contained zero sneak inputs. | The policy action space matches intent. Equivalent enforcement for human Paper players remains unverified. [Policy action conversion](../../crates/pvp-policy/src/recurrent.rs#L51) |
| Hunger and natural regeneration | Omitted or disabled in the Rust duel profile. The earlier test ladder used `hunger: false` and `regen: false`. | Record the effective rules in the host arena. [Rust duel profile](README.md#supported-match-profile) |
| Starting position and network | The training runner derives matches from seed and network condition. One recording starts at about 26.8 blocks, while the earlier `rl-flat` arena placed fighters 20 blocks apart. | Record the training distribution and fixed deployment settings. [Training runner](../../crates/pvp-rl/src/lib.rs#L171) |

## Known differences and open verification items

For each item, use evidence from inputs, actions, hits, damage received, and outcomes to determine whether actions learned under the sword PvP rules have the same effects on Paper. Later sections also retain implementation diagnostics collected before the rules were consolidated.

### Priority 1. Attacking moving targets

**Verified.** Against a stationary target, the ordinary client selected the target at an initial separation of 3.29 blocks and missed at 3.31. In an isolated test with 51 ms upstream, Paper's normal handler accepted a single bot attack selected from a received position. Post-hit position differences remain under mixed inputs. Target selection tests (`build/parity/combat-pick-01`) · Bot acceptance log (`build/parity/combat-replica-attack-51up/opponent_attack_bot/attack-uplink-difference.json`) · Long mixed-input comparison (`build/parity/combat-mixed-long-flat-01/suite-difference.json`)

**Open.** Repeat fixed-input comparisons as opponents approach or retreat across the reach boundary, strafe through the crosshair, move relative to each other, or change spacing after a hit. Click just before and after entering and leaving reach. Test immediate retries after a miss and follow-up attacks after knockback. Swap seats and repeat five times per seat, fixing both players' inputs and target click times before each trial.

**Record and pass criteria.** At click time record self-prediction, last received target position, aim, selected target ID or miss, and attack packet send/arrival/application ticks. At the Paper decision record both server positions, acceptance/rejection, charge, damage, and applied post-hit velocity. Map target IDs to seats within each trial. Require agreement at the same processing stage on selection, packet presence, acceptance, attack order, and application tick, with position, velocity, damage, and charge errors at most `1e-6`. Report whether the first divergence occurs during target selection or server acceptance.

### Priority 2. Agreement across network conditions

**Verified.** In 14 nominally matched delay conditions with one attack, all agreed on the first eight post-hit movement packets, and four agreed on the attack acceptance tick. Representative cases covered 30 and 120 FPS. The production bot used a fixed 10 ms age for opponent position, velocity, and state information. Fixing these three ages at 10 ms in sequential replay of 78 measured decisions changed 15 PPO actions and 36 G2 actions. Comparisons by delay (`build/parity/combat-matched-network/suite-difference.json`) · 30 FPS (`build/parity/combat-fps30-boundary/suite-difference.json`) · 120 FPS (`build/parity/combat-fps120-boundary/suite-difference.json`) · Fixed-model sensitivity analysis (`build/parity/combat-policy-replica-51/policy_bot_0up_51down_ParityA_0/training-impact.json`)

**Open.** Run Priority 1 fixed inputs at 60 FPS and RTTs of 0, 1, 20, 49, 50, 51, 99, 100, 101, 120, 150, and 300 ms. At RTT 100 ms, test upstream/downstream splits of 10/90, 50/50, and 90/10 ms to examine observation and attack application effects. For representative conditions test 30/120 FPS, both sides of tick boundaries, and reversed connection and seat order. Agreement under identical nominal delays remains undetermined for observation age, selection, acceptance, and post-hit position.

**Record and pass criteria.** For each direction record configured delay, measured receipt/forward/application times, client frame, server tick, and the received information and ages used by the policy. Record nominal-condition differences. When arrivals vary, replay the measured order in Rust and compare selection, acceptance, and state at the same processing stage. Retain both nominal results and measured-order replay. Avoid post hoc position or tick shifts. Mark conditions pending or failed for missing data, unmeasured fields, or discrepancies that change results.

### Items remaining after these tests

| Established difference or finding | Open verification |
| --- | --- |
| Opponent information age and post-miss charge can change a fixed model's actions | Compare before/after observation and attack-path corrections with identical weights, opponents, and initial conditions. Measure hit rate, follow-ups, incoming hits, and win rate. Fixed-observation sensitivity tests established action changes only. |
| The 100-seed movement-only test and single 6,000-tick mixed-input case have different coverage | Repeat mixed matches under Priority 2 conditions and increase seeds to test accumulation of observation and combat-outcome differences. |
| Rust and Java inference agreed on identical observations | Effects of environment differences on learned performance and tactics remain unmeasured. Estimating the win-rate impact requires matches under common evaluation conditions. |

## Additional measurements and verdicts

| Subject | Measured result | Evidence |
| --- | --- | --- |
| Fixed movement | 12 cases × two directions × five runs. All 240 comparisons of ordinary client against Rust and against bot passed | `build/parity/motion-fixed-5/summary.json` |
| Long movement run | 6,000-tick stationary case. Ordinary client vs. Rust and vs. bot passed | `build/parity/motion-long-6000/summary.json` |
| 6,000 ticks of mixed inputs | Feeding measured ordinary-client processing order to Rust reproduced state, damage, and decoded velocity across 51,522 events | `build/parity/combat-mixed-long-flat-01/mixed_000_player/rust-replay/difference.json` |
| Random-input movement | Measured 100 seeds × 600 ticks. 96 passed. Four diverged at sneak/sprint transitions | `build/parity/coverage.json` |
| Received sprint state | Seed 13 diverged on remeasurement. Replaying 600 ticks with measured sprint flags and movement speed gave zero error in every field for Rust and bot | `build/parity/motion-random-sprint-trace`, `build/parity/motion-sprint-bot-feedback-013-flat` |
| Ordinary-client delay conditions | Captured one attack in each of both matches at 12 requested RTTs and two asymmetric settings. Damage and HP differences from Rust stayed within `1e-6` in all 14. Full matches failed under every condition | `build/parity/combat-network-matrix/suite-difference.json` |
| Rust replay with measured Paper processing order | Fully charged attack on a stationary target. Maximum coordinate and velocity error zero. Measured attacks, decoded knockback, durability, and timers matched | `build/parity/combat-agent-02/rust-verified/difference.json` |
| Client-side opponent replica | 120 FPS cap, 10 ms up/90 ms down. Received packets and update order reproduced position, velocity, angles, and displacement over 91 events | `build/parity/combat-observations-120/rust-replica/difference.json` |
| Inference from live logs | Fixed Recurrent PPO and G2, 166 decisions each. Model SHA-256 and chosen actions matched. Maximum feature error about `1.01e-20`, maximum GRU and logits error zero | `build/parity/combat-live-ppo`, `build/parity/combat-live-g2` |
| Bot policy input from opponent replica | At 51 ms downstream, position, velocity, displacement, pose flags, HP, and age since receipt matched the replica in all 79 samples. Rust PPO matched all 78 decisions on the same observations | `build/parity/combat-policy-replica-51` |
| Bot attack selection and Paper acceptance | At 51 ms upstream, selected a single attack from received target position. Attack then Swing traversed normal Paper handlers. Two ticks after sending, raw damage was 7 and final damage `0.6803998947143555` | `build/parity/combat-replica-attack-51up` |
| Reach and aim | Reselection from measured self/opponent state matched a target at 3.29 blocks, a miss at 3.31, and a target during descent | `build/parity/combat-pick-01` |
| Server replay of critical hits | Remeasured including movement acceptance. For five runs per seat (ten ordinary-client trials), state, damage, and decoded velocity matched Rust | `build/parity/combat-critical-rechecked-5/rust-replay/summary.json` |
| Hit at coincident positions | Five runs per seat in ordinary-client and bot matches, 20 trials total. Rust used measured random values and processing order for the ten ordinary-client trials and matched state, damage, and decoded velocity | `build/parity/combat-overlap-fixed-5/rust-replay/summary.json` |
| Fall damage | Replayed 334 events from the airborne state recorded after setup teleport. State and fall damage matched | `build/parity/combat-fall-replay/rust-replay/summary.json` |

Delay-test values were requested on the TCP relay for two ordinary clients. The comparison bot's velocity queue stayed at 0 ms downstream and its policy delay at RTT 20 ms. For all 14 conditions the first eight post-hit movement packets agreed between client send values and Paper decoding. The first coordinate change in server tick-end state appeared one to eight ticks after the hit. Even the 0 ms relay trial incurred measurable real delay.

The additional `combat-matched-network` applied the same delay values to the ordinary-client TCP relay and the experimental stationary-target bot's upstream/downstream queues. In all 14 conditions the first eight post-hit movement packets matched in coordinates and grounded state, including application by Paper on the bot side. Attack acceptance tick matched in four conditions. Rust replay with the ordinary client's measured Paper processing order passed measured state, damage, and decoded knockback in all 14. The TCP relay records deadlines and actual forwarding times in `relay.ndjson`.

A diagnostic path also received opponent state from the ordinary server tracker. At 51 ms downstream it received opponent positions for critical and sprint cases and interpolated them in an independent replica. The production bot's position and metadata ages were fixed at 10 ms per decision. On an ordinary client capped at 120 FPS, elapsed time since the last position update in the observation window ranged from about 44 to 3,018 ms. Unifying the main network and observation paths remained an open acceptance condition.

`--replica-observation` is an isolated-match experiment constructing bot observation and aim from received opponent information. At 51 ms downstream, opponent position age ranged from 0 to 2,300 ms of logical time. The bot's own state still came from its existing Citizens entity. The existing policy snapshot delay also remained, and update phases had yet to be aligned with the ordinary client.

With `--attack-uplink-ms` also enabled, the bot selects its attack target from received hitboxes and delivers attack and swing to Paper through a separate upstream queue. At 51 ms upstream, the single attack applied after 100 ms logical time. The measured interval from send to handler completion was about 106.5 ms. This path is for isolated diagnostics.

On remeasurement of seed 13, the ordinary client applied the server's shared flags and movement speed attribute before moving. Immediately preceding self-prediction had sprint enabled, but receipt cleared it and set speed to `0.10000000149011612`. Omitting receipt in the minimal case produces a difference of `0.0017639999189078548` blocks on each horizontal axis. `tests/fixtures/parity/motion/sprint_feedback` contains raw events and replay input for that tick. The other three seeds reproduced the difference at 51 ms downstream. Applying both measured fields immediately before movement in Rust matched all 600 ticks. The original network conditions still passed 96/100.

With Packet Handler diagnostics enabled, Netty identifies packets from ordinary clients and correlates each with the main-thread return from processing that same packet in Paper. Separate logs cover client application and sending, server receipt, handler completion, and tick end. Rust replay uses the measured order directly.

Before the change, Rust damped stationary Y velocity to `-0.07683200149536133` on the first tick. Paper reported `-0.0784000015258789`. With `server_rules: paper_flat_v1`, flat-ground collision and gravity affect velocity as in Paper while coordinates retain the last accepted movement value. Existing training and recordings select `legacy`. Select the verification configuration explicitly.

`paper_flat_v2` also routes self-directed sprint flags and movement speed updates through the downstream queue. A 51 ms regression test verified that state stayed unchanged until the deadline and applied on a receive frame. Whether Paper emits the updates at the same measured times remains unverified.

The initial 600-tick mixed-input run found an attack-charge timer of zero in Paper and 14 in Rust after a miss. The pinned Paper build's ordinary swing handler calls `ServerPlayer.swing(InteractionHand)` and resets the timer. The fix added this behavior to Rust's `paper_flat_v2` and made the bot call the same overload. The revised version records `ServerboundSwingPacket` arrival and charge at handler completion.

Replayed identical measured packets from an ordinary-client match before and after the fix. Before the change, the charge timer immediately after Swing was 13, versus zero in Paper. Afterward, state, damage, and decoded velocity matched across 5,276 events. Self-prediction replay using received data from each ordinary client also passed 600 ticks each, with maximum error `6.49e-22`.

Measured critical hits in human and bot matches both had raw damage 10.5 and final damage `1.152900218963623`. The preliminary click occurred early and dealt normal damage 7.0, so a separate retest clicked after descent. Rust replay of the measured crit met damage tolerance but still differed on acceptance state of the first descending movement packet before the attack.

The following initial results use the configurations and implementation versions current at the time of each test.

## Initial measurements

Paper 26.1.2 build 74, Citizens 2.0.44 build 4269, ZonePractice Pro 8.2.0, and ordinary Minecraft 26.1.2 clients ran separately from the live server.

- Before the fix, diagonal movement differed by roughly 0.001838 blocks on each horizontal axis at the first tick and up to about 0.242899 blocks at 80 ticks. The ordinary client and Rust agreed in this case.
- Citizens lacked a tiny-velocity cutoff when stopping. In one case the bot moved about 0.001705 blocks on the tick when the ordinary client stopped.
- Corrected input at float precision, used the previous tick's sneak state, and corrected sprint transitions and tiny velocities.
- Afterward, 12 cases × two directions gave 24 scenarios, each comparing the ordinary client with Rust and the bot. **All 48 comparisons passed.** Each condition ran once. Five repeats and actual seat-swapped matches were still outstanding.
- Java/Rust comparisons of features, GRU memory, logits, and actions passed for Recurrent PPO and G2 across 100 seeds × two seats, 400 episodes and 127,148 decisions. Equivalence of live measured observations requires separate evidence.

## Isolating a single attack and received velocity

Started two ordinary clients and delivered one fully charged normal attack to a stationary opponent in an actual ZonePractice match, then attacked a stationary Citizens bot under the same setup. Measurement completed `combat-01` and `combat-03`. Repetition, seat swapping, and coverage of other attack types were still pending.

| Measurement | Result |
| --- | --- |
| Final damage | `0.6803998947143555` for human and bot, within `1e-6` of Rust's attack calculation `0.6803999543190002` |
| Post-attack HP | `19.319599151611328` for human, bot, and Rust attack calculation |
| Live knockback | Failed. The bot moved earlier and its final Z position differed by about `0.00006072147` blocks |
| 35-tick replay from received state | Ordinary client vs. Rust and vs. Citizens both passed with zero numerical difference in recorded fields |

The ordinary client applied velocity Y=`0.36080083012879194`, Z=`0.39998779222364655`. The live bot's first displacement was Y=`0.3608000051975182`, Z=`0.4000000059604645`. The three physics updates agreed when given identical post-receive initial state and zero input. This narrowed the investigation to velocity quantization, delivery, and application timing. The live bot path still needed repair and retesting.

In `combat-03` the human trial accepted the attack on tick 11 and the bot trial on tick 10. File-based start signaling was unsynchronized with packet arrival, so this test cannot establish a fixed delay. The comparison applies no retrospective tick offset.

A separate Fabric stream recorded state after ordinary `handleSetEntityMotion` application and after `LocalPlayer.tick`. These are main-thread application records. Arrival at Netty has a different timestamp. Paper's `PlayerVelocityEvent` can differ from received velocity, so retain actual packets as evidence. Human Bukkit velocity and self-predicted velocity are also separate.

`extract_impulse.py` extracts 35 consecutive physics updates immediately after receipt, retaining the original receive event, sequence numbers, and hashes. This is a component test independent of full attack verification. Save wall time and tick number separately. Paper's scheduler uses ticks. [Official documentation](https://docs.papermc.io/paper/dev/scheduler/)

## November 18, 2026: experimental velocity packet path

Bytecode inspection showed that the pinned Citizens `EntityHumanNPC.hurtServer` clears `hurtMarked` after damage handling and restores it in a separate task. This bypasses the ordinary Paper player attack path that sends a velocity packet and restores the victim's pre-hit server velocity.

Added `experimental-motion-transport-v1` for isolated diagnostics. It bypasses only the relevant Citizens override, calls pinned `ServerPlayer.hurtServer`, and receives the self-directed velocity packet produced by Paper. It encodes and decodes through the normal `LpVec3` codec, then applies before physics from a downstream queue with a deadline. Both `pvp.parity.root` and `pvp.parity.motionTransport=true` are required. Ordinary startup uses the existing path.

| Measurement | 0 ms downstream | 51 ms downstream |
| --- | --- | --- |
| Post-attack stopping position vs. ordinary client | Zero on all axes | Zero on all axes |
| Received velocity vs. ordinary client | Zero on all axes | Zero on all axes |
| LpVec3 encoded value, Paper vs. Rust | Exact match at `f9ffb3315c59` | Same |
| Application tick for this attack | Same tick as send | Two ticks after send |
| Delay from logical deadline to application | 0 ms | 49 ms |
| Full-match parity | Failed | Failed |

At 51 ms downstream, the physics update applied 100 ms after sending in logical time. The measured epoch-time delta between send and application was 92 ms. Post-receive physics ran in phase with the Paper tick. Ordinary-client frame phases and upstream movement packets were still unmodeled. This comparison did not impose 51 ms of network delay on the ordinary client.

The `combat-transport-01` and `combat-transport-51ms` logs also showed velocity before physics returning to pre-hit `[0, -0.0784000015258789, 0]` and then being replaced with the decoded velocity. Timing still differed from ordinary-client coordinate application on the server. Even at 51 ms, tick 13 differed in position and grounded state. No post hoc fixed-tick adjustment was applied.

For a single fully charged attack on a stationary target, the experimental path resolved the stopping-position difference caused by missing quantization. Moving hits still needed full separation of server and self-predicted velocity, upstream traffic, observations, and target selection. The model and production adapter continued using their existing paths.

Additional checks comprised Java build/test including three queue-boundary and send-order tests, 20 Python tests, and two Rust comparisons against measured bytes. The optional Java inference corpus was unspecified and skipped, so it is excluded from the pass count.

## Evidence

Local `build/parity/baseline/manifest.json` holds hashes and copies of pre-change JARs, settings, and models. `motion-04` is the pre-fix diagonal movement, `motion-05` records the stopping difference, and `motion-07` contains the 48 post-fix comparisons. Each scenario retains initial state, input sequence, logs from all three systems, and differences.

`motion-01` through `motion-03` failed during startup or measurement preparation. Parts of `motion-05` contacted an old wall and fell outside scope. `motion-06` stopped because of a Windows sharing conflict on the instruction file. None contributed to the overall pass evidence.

`combat-02` stopped after a race read a partially written JSON file. After switching to atomic publication of complete files, `combat-03` ran to completion. `impulse-bot-01` replays Citizens from the same received state. The version-controlled `tests/fixtures/parity/` contains the first combat divergence, raw receiver logs, and a replayable movement case.

Additional verification passed 18 Python tests, Java build/test, Fabric build, and five saved Rust replay cases. The Rust attack example tested damage calculation alone. It did not replay a complete Duel from the measured initial state. The combat comparator withholds an overall pass when durability or timers remain unverified even if measured fields agree.

See the [comparison tools guide](../../tools/parity/README.md) for reproduction commands, log specifications, limitations, and outstanding work. Existing training APIs, model weights, and recording formats were preserved. Match logs identify the bot behavior including the swing fix as `citizens-player-input-v3` and include a configuration hash.
