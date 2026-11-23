# Tick order, hit detection, and network delay

References N1–N9, C1, C7, M1, and M2 in [source-index.md](source-index.md).

## 1. Server order

In 26.1.2, `MinecraftServer.processPacketsAndTick` **drains the PacketProcessor queue before calling tickServer**. Preserve this boundary when scheduling packet receipt. Ordinary gameplay packets from the network thread enter this queue via `PacketUtils.ensureRunningOnSameThread`.

Combat-relevant steps on the target dedicated server at a steady 20 TPS:

```text
processPacketsAndTick
  PacketProcessor.processQueuedPackets
    Process Move / PlayerInput / PlayerCommand / Attack / Swing ... in receive queue order
  tickServer
    tickChildren
      ServerLevel.tick
        chunkSource.tick -> ChunkMap.tick -> ServerEntity.sendChanges
        entityTickList -> ServerPlayer.tick
          Decrement invulnerableTime
      tickConnection -> Connection.tick -> ServerGamePacketListenerImpl.tick
        tickPlayer -> ServerPlayer.doTick -> Player.tick
          Update LivingEntity and related state
          attackStrengthTicker++
          Update Pose
      PlayerList.tick and related calls
      Resume outbound flushing
```

This excerpt covers combat-related calls. `ServerPlayer.tick` and `ServerPlayer.doTick` are distinct paths. The latter runs super.tick. Treating both as ordinary player ticks would advance the player twice.

Consequences:

- An incoming Attack reads timers before that tick's increments or decrements.
- Tracking updates precede the world's player tick and the connection's doTick. A pose change cannot be advanced into an earlier tracker send in the same tick.
- The queue drains until empty. On a real server, packets arriving during processing can join that pass. Define boundary behavior for a zero-processing-time virtual model and record actual processing start/end times on the reference.
- Preserve packet order within a connection. Ping alone cannot determine tie order across connections.
- If a player dies first, `ServerPlayer.die` calls `markClientUnloadedAfterDeath()` and a later Attack is rejected by `hasClientLoaded()`. Respect this order in double-hit scenarios.

## 2. Client order

`Minecraft.runTick` processes the receive queue every render frame and then runs the logical ticks due in that frame. Receive cadence can differ from logical tick cadence. Rendering may be omitted, but the receive frames still need virtual timestamps.

Combat-relevant order in an ordinary logical client tick:

```text
Minecraft.tick
  pick(1.0F)                       // Select from the view ray and replicated opponent AABB
  gameMode.tick
  handleKeybinds -> startAttack
    Send Attack packet
    Local player.attack(remote)    // Local prediction
    Reset client attack timer
    Swing
  Decrement missTime
  ClientLevel.tickEntities
    LocalPlayer.tick
      super.tick -> update input, movement, timers, and pose
      Send PlayerInput if changed
      sendPosition
        Send START/STOP_SPRINTING if changed
        Send PosRot / Pos / Rot / StatusOnly
    RemotePlayer.tick              // Interpolate opponent replica. Verify iteration order too
  Send ClientTickEnd at the end of the logical tick
```

In this ordinary path, Attack precedes new movement/sprint packets from the same tick. Distinguish yaw set by immediate auto-aim from the last yaw received by the server.

Minecraft also picks during rendering, but the click path uses pick(1.0F) from the beginning of the logical tick. Fix the hooks where the bot changes aim and clicks. Apply frame mouse inputs and key inputs at their respective hooks. The proposed reference client has fixed settings, no GUI, and auto-jump disabled.

## 3. Sword target selection and server acceptance

### Client

The specified sword has no custom AttackRange, as also verified by the numerical probe. LocalPlayer.raycastHitResult proceeds to ordinary picking.

- Start at the player's eye, 1.62F standing or 1.27F sneaking.
- blockInteractionRange=4.5 and entityInteractionRange=3.0. Initially cast the view ray out to 4.5.
- A block hit such as the floor shortens the search distance.
- `ProjectileUtil.getEntityHitResult` intersects the line segment with opponent AABBs and chooses the closest target.
- inflate(1.0) expands the search AABB for candidate selection only. The target hitbox keeps its actual dimensions.
- Player getPickRadius is zero. Add no 0.1 or 0.3 margin here.
- Finally require `closerThan(..., 3.0)` between eye and intersection, i.e. **squared distance < 9**. Exactly 3 is a MISS.
- Preserve the branch for an eye inside the target AABB and boundaries for edge intersections and contains.

`AABB.contains` includes each axis minimum and excludes its maximum. In clip, axis direction uses ±1e-7 and face intersection has a 1e-7 allowance. Segment parameter `0 < s < current minimum s` is strict. Check these endpoint and parallel tests even when using a general ray/AABB library.

Measure eye to intersection. At width 0.6F, two grounded players can be slightly more than 3 apart center-to-center and still hit if the near face is closer than 3. Elevation and sneak change the eye/AABB relationship.

### Server

`handleAttack(ServerboundAttackPacket)` performs the following checks relevant to this environment.

1. Client loaded and player outside Spectator mode.
2. Target ID exists and lies inside the world border.
3. `isWithinAttackRange(mainHand, targetAABB, 3.0)`.
4. Target valid, non-piercing attack, enabled item, and minimum charge satisfied.
5. `player.attack(target)`.

The sword's default AttackRange has min=0, max=3, and margin=0. With extraBuffer=3, the shortest eye-to-AABB distance can be **0 through 6 inclusive**. Preserve the different comparison operators on client and server.

This path uses the position at server receipt. It has no client-style ray recomputation, historical rewind, or ping-based reach extension. The tolerance accommodates motion in transit after legitimate client selection below 3. Passing an opponent ID directly to attack at 6 would violate the requested mouse/click controls.

## 4. Required traffic and cadence

| Packet/state | Meaning and handling |
| --- | --- |
| ServerboundAttackPacket | Target ID only. Omits yaw, pitch, hit point, and client time |
| ServerboundSwingPacket | Arm swing. Leaves server cooldown running |
| ServerboundPlayerInputPacket | Changes in WASD, jump, shift, and sprint input. Server stores input and updates shift |
| ServerboundPlayerCommandPacket | START_SPRINTING / STOP_SPRINTING changes server sprint state |
| ServerboundMovePlayerPacket | PosRot / Pos / Rot / StatusOnly. Includes double position, float rotation, onGround, etc. as applicable |
| ServerboundClientTickEndPacket | End of logical tick. If no movement arrived, zero server knownMovement, then reset the receipt flag |
| ClientboundSetEntityMotionPacket | Velocity from knockback and other sources. Replaces LocalPlayer velocity and interpolates RemotePlayer velocity |
| ClientboundMoveEntityPacket | Tracked opponent's relative position, rotation, and grounded state |
| ClientboundEntityPositionSyncPacket | Synchronizes position, rotation, etc. on changes such as ground transitions |
| ClientboundSetEntityDataPacket | Synchronized metadata including pose and sprint |
| ClientboundSetHealthPacket and related traffic | Own HP and death notification. Damage events and opponent metadata are other observation candidates |

State transmission is conditional. LocalPlayer.sendPosition uses:

```text
position_changed = squared_distance_from_last_sent > (2e-4)^2
                   OR positionReminder >= 20
rotation_changed = deltaYaw != 0 OR deltaPitch != 0
```

These conditions and changes in grounding/horizontal collision determine packet type. Input and sprint are sent only on change.

Omitting ClientTickEnd leaves lastKnownClientMovement stale when movement packets are skipped. A position difference should not persist as current velocity. This value also differs from ordinary server deltaMovement.

Server movement acceptance checks `firstGood` / `lastGood` coordinates, movement packet count, abnormal speed, floor collision, invalid values, and other conditions. It can send position corrections. Many rejection paths should be rare with a legitimate client on flat ground, but verify with traces including knockback and latency before omitting them.

## 5. Opponent updates and interpolation

EntityType.PLAYER has an updateInterval of **two ticks**. ServerEntity.sendChanges can trigger on the two-tick cycle, needsSync, dirty metadata, and other conditions. Intervals can therefore vary.

- Relative position deltas are differences of `round(position*4096)` values stored as shorts. When decoding a zero axis delta, retain that baseline component unchanged.
- The normal position-change threshold is squared displacement `>= 7.6293945E-6F`. There is also a 60-tick condition.
- Ground transitions, out-of-range deltas, teleportDelay, and similar conditions trigger full position synchronization.
- Rotation uses byte encoding `floor(angle*256F/360F)`, nominally 360/256 degrees per step.
- LivingEntity's InterpolationHandler defaults to **three steps** for received opponent position. Advance toward the target using remaining steps as denominator and replace the target when a new update arrives.
- RemotePlayer velocity interpolation uses `updateInterval+1=3` steps, with state separate from position interpolation.

Auto-aim targets the **opponent AABB visible to that client**. Reading the latest server position leaks information even at 20 ms ping. Estimate opponent velocity from received motion or interpolated position differences.

## 6. Velocity packet quantization

In 26.1.2, ClientboundSetEntityMotionPacket uses `Vec3.LP_STREAM_CODEC` → `LpVec3` (N6).

Conceptual encoding of nonzero values:

```text
m = max(abs(vx), abs(vy), abs(vz))
if m < 3.051944088384301e-5: one-byte zero
s = ceil(m)
q_axis = Math.round((v_axis/s * 0.5 + 0.5) * 32766)
decoded_axis = (min(q_axis & 32767, 32766) * 2 / 32766 - 1) * s
```

Store 15 bits per axis plus scale and continuation flags. Ordinary small velocities use six bytes, with an extra VarInt when needed. Sanitization maps NaN to zero and caps absolute magnitude. Check bit layout and byte order against N6.

For example, 0.4 round-trips to about 0.3999877922 and 0.7 to about 0.6999938961. **Deliver the quantized knockback velocity to the client.** [Official codec results](evidence/probe-results.json)

## 7. Proposed virtual-time contract for 20 ms ping

Interpret ping as round-trip time: **20 ms RTT, 10 ms upstream, 10 ms downstream**. This is a proposed interpretation of the requested profile. Minecraft does not assign 10 ms to all connections. A one-way delay of 20 ms is a separate 40 ms RTT profile.

Track the following times and ordering:

- Server tick boundaries at 50 ms intervals.
- Logical tick and receive-frame boundaries for each client.
- Packet creation, send flush, arrival, enqueue, and application times.
- Per-connection sequence and stable ordering of simultaneous events.

Start with steady 20 TPS, fixed 10 ms each way, and no jitter or loss. Use 60 Hz as one comparison frame rate and make FPS and phase configurable. Represent fractional frame times with integer nanoseconds plus a remainder or equivalent to avoid accumulating rounding error. Integrate physics at 20 Hz.

**Proposed example.** A sends Attack at 5 ms. It arrives at 15 ms and applies at the next server boundary at 50 ms. An immediately sent knockback velocity reaches B at 60 ms. B processes it at the next virtual frame, and movement appears at B's subsequent physics tick. The time from click to victim movement can thus exceed 20 ms.

Advance directly to the next virtual event without real sleeps, sockets, or rendering. Internal events can carry only relevant payload fields. Preserve codec-induced changes, send conditions, and application order.

Source alone cannot fix server/client phases, FPS, click hook, or simultaneous arrival order. Replay the same arrivals on both sides for verification. Training can vary phases and seats by seed while retaining the order of packets that have already arrived.
