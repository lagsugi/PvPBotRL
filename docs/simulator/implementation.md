# Implementation contract and validation boundaries

## Source basis

The implementation is derived from the official 26.1.2 source extraction and source index recorded in [Phase 1](../research/minecraft/source-index.md). The existing [provenance](../research/minecraft/evidence/provenance.json) identifies those artifacts. This phase read the extracted source; it did not execute Minecraft or regenerate Java reference results.

| Rust behavior | Main source anchors |
| --- | --- |
| Charge, critical hits, sprint knockback, velocity restoration | `Player.attack`, `Player.causeExtraKnockback` |
| Hurt immunity, damage reduction, normal knockback | `LivingEntity.hurtServer`, `actuallyHurt`, `knockback`, `CombatRules` |
| Equipment break ordering | `ItemStack.applyDamage`, `LivingEntity.detectEquipmentUpdates`, `onEquippedItemBroken` |
| Movement, input shaping, sprint and sneak | `LocalPlayer.aiStep`, `modifyInput`, `LivingEntity.aiStep`, `travelInAir`, `Entity.move` |
| Fall damage | `Entity.checkFallDamage`, `LivingEntity.calculateFallDamage` |
| Client input and attack ordering | `Minecraft.tick`, `startAttack`, `continueAttack`, `LocalPlayer.sendPosition` |
| Server packet gate and player timers | `MinecraftServer.processPacketsAndTick`, `ServerPlayer.tick`, `ServerPlayer.doTick` |
| Attack range and movement receipt | `ServerGamePacketListenerImpl.handleAttack`, `handleMovePlayer`, `handleClientTickEnd` |
| Tracking and interpolation | `ServerEntity.sendChanges`, `ClientPacketListener`, `RemotePlayer.aiStep`, `InterpolationHandler` |
| Numeric codecs | `LpVec3`, `VecDeltaCodec`, `Mth.packDegrees`, `AABB.clip` |

## Numeric rules

Position and velocity are `f64`. Health, damage, input shaping, charge, and many source constants remain `f32`; widening happens where the source widens. Rust does not deliberately fuse arithmetic using `mul_add` or enable fast-math.

Source review corrected one Phase 1 interpretation: `Player.getSpeed()` overrides the parent implementation and reads the current attribute directly. Ground acceleration therefore responds to sprint changes in the same movement tick. The parent `speed` field is retained for diagnostics but does not drive player movement. See the English correction in [movement.md](../research/minecraft/movement.md) and the sprint-transition regression test.

The diamond sword's attack speed is `4.0 + (-2.4f32 as f64)`, not the exact decimal 1.6. Charge uses `(attack_ticks + 0.5f32) / delay`. At counter 12 it is slightly below 1; at 13 it clamps to 1. A strong hit requires charge > 0.9. Critical damage requires a strong hit and falling off the ground without sprinting in this profile.

With intact armor, base attack damage is 7, armor is 20, toughness is 8, and Protection is 16. Armor reduction precedes Protection. During the protected half of hurt immunity, stronger raw damage applies only its raw difference, then reduces that difference. Full ordinary knockback and extra sprint knockback are sequential. An accepted server melee hit can send victim motion and restore the victim's previous server velocity; the client applies the quantized motion later.

Durability is reduced before protection lookup. Cached armor/toughness modifiers are refreshed in the equipment update phase; protection reads surviving pieces immediately. Sword break changes the held-item identity, damage and attack speed, and resets the attack timer during the player update. This boundary remains a priority for later differential validation.

Motion uses the 26.1.2 `LpVec3` layout, including 15-bit axes, scale continuation, VarInt, zero threshold, saturation, and NaN handling. Relative coordinates use Java-style rounding on the 4096 grid; a zero delta preserves the receiver's unquantized axis. Rotations use packed signed bytes. Received remote motion and position interpolate separately over three logical ticks.

## Time and event order

All clocks use integer nanoseconds. Client frame `n` is scheduled at `frame_phase + floor(n * 1e9 / fps)`, preventing per-frame rounding drift. There are no simulation sleeps, sockets, graphics, or wall-clock physics. Wall time is used only for CLI throughput reporting.

Each `step` applies the requested controls for one decision interval and processes events strictly before its ending boundary. Events at exactly the boundary belong to the next call. At a shared timestamp, the server gate runs before client frames. Client frame ties use `first_client` if set, otherwise the low bit of the episode seed. This is a reproducible scheduling convention; RTT alone cannot determine a real machine's tie order.

The server drains packets with arrival time <= the gate time, sorted across connections by arrival and then global send sequence. FIFO is maintained within each fixed-latency connection. It then runs tracker updates, world hurt-timer decrements, and connection/player updates. Packet processing consumes zero simulated CPU time. The model has no packet arrival inserted halfway through a server tick.

At a client frame, arrived downstream packets are applied before any due local logical tick. That tick applies the view action, picks against the current remote AABB, handles clicks, predicts local attack effects, updates movement, sends changed input/sprint/position, advances the remote replica, and sends `ClientTickEnd`. Local-before-remote entity tick order is fixed. Thus an attack can precede the new sprint or movement command of the same logical tick, and use the server's previous yaw for authoritative sprint knockback.

Packets are scheduled for transmission at their send phase with the configured one-way latency. This is the zero-duration equivalent of the flush contract, not a simulation of real Netty buffers, socket scheduling, or OS threads. Initial frames and ticks have zero phase unless configured otherwise. Reset starts from already loaded and synchronized players with full charge; login, spawn settling and chunk loading are omitted.

The client ray uses the eye, the real 0.6F-wide opponent AABB, floor occlusion, and strict hit distance < 3. The server independently checks its current eye-to-target-AABB distance <= 6 for the default sword's buffered acceptance. There is no historical rewind, ping-dependent range formula, or action that bypasses normal client picking with a target ID.

## Observation boundary

Each player owns one local state and one delayed opponent replica. Opponent position, metadata and velocity arrive through separate packet paths. Automatic aim uses that replica. The normal actor API cannot mutate the internal server. Diagnostic snapshots are copies, and only explicit debug access exposes authoritative fields.

The environment stops immediately on the first confirmed server death, including a death from falling. It does not finish draining later attacks or wait for a death notification to arrive at the client. Terminal observations can therefore still show pre-notification HP. Simultaneous attacks are not combined into an artificial draw. An episode limit produces `Truncated`, with no winner.

## Deliberate reductions and remaining gaps

| Area | Current contract and remaining work |
| --- | --- |
| Terrain | One immutable collision plane. No voxel shape traversal, ledges, walls, fluids, ladders, block effects, or stepping. It also blocks downward rays. |
| Floor clicks | Tracks starting/continuing/stopping an unbreakable-floor interaction and the local timer reset on stopping. No digging packets, block acknowledgements, or break progress are simulated. |
| Movement acceptance | Handles finite coordinates, speed budget, floor penetration, correction and acknowledgement. General vanilla collision rejection, post-impulse grace, world-border behavior, teleport retry and anti-flight disconnect machinery are not fully ported. Ordinary supported traffic is the target; adversarial packet injection is not an action. |
| Corrections | Uses an absolute position/rotation correction with zero local velocity. Full relative teleport flags and every correction subtype are outside this profile. |
| Client loop | Fixed input hook and local-before-remote order. No real mouse sensitivity, render interpolation for aiming, variable frame times, GUI, auto-jump, pauses or overload. The chosen hook/order must match the eventual validation client. |
| Inventory synchronization | Fixed equipment and durability messages replace a general slot/attribute synchronization protocol. Inventory/attribute ordering around a break must be checked against live traces before claiming exactness. |
| Metadata | Models pose, sprint, ground and own health used by current observations. Hurt animation, sounds, particles and other potential future observations are omitted. |
| Tracker dirtiness | Tracks health, shift, pose and sprint changes plus impulse flags, including a flag changed and restored within one gate. Dirty fields from omitted systems are not represented. Tracker phase agreement remains a differential-validation target. |
| Randomness | Java-style 48-bit RNG makes overlap knockback reproducible within the simulator. Seed assignment and consumption by omitted vanilla systems are not claimed to match a running server. |
| Trigonometry | Uses the Mth table shape and indexing, generated with host Rust `f64::sin`. Bit identity with Java's table on all platforms remains unverified. |
| Long-distance state | Normal arena combat is the target. Very large remote teleports, re-pairing, dimension changes and arbitrary disconnections are not modeled. |
| Network | Fixed, lossless, FIFO links with independently configurable delays and phases. No jitter/loss/retransmission simulation or latency estimator. |
| Rules | No regeneration, hunger loss, contact pushing, shields, other weapons, consumables or extra entities. Sweeps have no additional target in this 1v1 profile. |
| Training | APIs and baseline are present. No learned policy, optimizer, self-play curriculum or transfer-quality claim. |

These boundaries are not evidence of Minecraft equivalence. Rust regression tests establish internal behavior and determinism for the supported profile. A future phase must compare packet-phase traces, movement, health, timers, inventory and impulses against Minecraft with the same seedable inputs, phases and effective arrival order. No such comparison is run by `tools/rust/check.bat` or the Rust CLI.

## Parallel execution and reproducibility

Every duel owns its clocks, queues, RNG and mutable state. Only the read-only sine table is shared. `Batch` owns a persistent Rayon pool; complete-match tasks avoid a per-decision barrier for throughput runs. Vector stepping instead returns one result per lane and is intended for batched policy inference.

Match seeds are derived from the master seed and lane index with a fixed SplitMix64 mixing function. Work scheduling does not choose seeds or modify state. Worker-count equivalence is tested within the same build. Portable replay stores version, configuration, seed and actions; byte-identical cross-platform replay is not promised until the numeric boundaries above are resolved. A serialized snapshot is diagnostic output, not a supported state-import format. `Duel::clone` is the supported exact in-process checkpoint.

Tracing is opt-in and bounded. The oldest records are dropped if capacity is exhausted; the drop count is exposed. The CLI drains trace records after every decision. Traces contain send/arrival times, application times and sequences, attack results and correction events. Simulation throughput should be measured with tracing disabled and reported separately from inference and logging costs.
