# Movement, pose, floor, and numerical compatibility

See M1–M5, C1, C2, and C5 in [source-index.md](source-index.md). Velocity is in **blocks/tick**, position in blocks. Even at 50 ms per tick, the movement formulas use no wall-clock delta multiplier.

## 1. Client-computed movement

`Player.isClientAuthoritative()` returns true. The local player's client simulates physical movement and the server validates and adopts received coordinates. Running LocalPlayer travel again on ServerPlayer each tick would duplicate movement. Keep server `deltaMovement`, received coordinate difference `lastKnownClientMovement`, and current client velocity separate.

Server deltaMovement also changes. In LivingEntity.aiStep reached from ServerPlayer.doTick, when there is no interpolation and canSimulateMovement is false, multiply the entire velocity by double 0.98, then discard tiny components. This interacts with jumps detected from movement packets and attacker slowdown. Keep this server-side update separate from local floor friction and gravity integration.

Disable player contact pushing as requested. Retain the opponent's AABB for attack hit detection.

## 2. WASD input processing

`KeyboardInput.tick` and `LocalPlayer.modifyInput` (M1):

1. Forward axis `forward = W-S`, lateral axis `left = A-D`. Opposing keys on the same axis cancel.
2. Normalize `(left, forward)` using `Vec2.normalized()`.
3. Multiply by 0.98F. The specified environment has no item-use slowdown.
4. When `isMovingSlowly()`, multiply by `SNEAKING_SPEED` (double attribute 0.3 cast to float).
5. Apply the `modifyInputSpeedForSquareMovement` correction.

Conceptual formula for step 5, evaluated in float:

```text
L = length(input)
direction = input / L
r = min(abs(direction.x), abs(direction.y)) / max(abs(direction.x), abs(direction.y))
distanceToSquare = sqrt(1 + r*r)
newLength = min(L * distanceToSquare, 1)
result = direction * newLength
```

Return early for zero input. Normal axial input has length 0.98 and diagonal input approximately 1.0. Sneak gives about 0.294 axially and 0.41578 diagonally. Normalizing all eight directions to equal speed would change these values.

`Entity.getInputVector` also checks squared input length and normalizes only above 1. The movement-direction transform follows (M2). Pitch does not affect horizontal direction.

```text
dx_accel = (left*cos(yaw) - forward*sin(yaw)) * accel
dz_accel = (forward*cos(yaw) + left*sin(yaw)) * accel
```

Yaw 0 faces +Z and yaw 90 faces −X. At yaw 0, A moves toward +X. Looking away from auto-aim changes yaw, so the same WASD input changes world direction.

## 3. Physics on ordinary dry ground

| Quantity | Value |
| --- | --- |
| Floor friction | Default ordinary block value 0.6F, assuming a flat grass floor |
| Floor speedFactor / jumpFactor | 1.0F / 1.0F |
| Base movement speed attribute | `0.1F` widened to double |
| Sprint modifier | ADD_MULTIPLIED_TOTAL of `0.3F` (nominal factor 1.3) |
| Air acceleration | Walking 0.02F, sprinting 0.025999999F |
| Gravity | double 0.08 |
| Vertical damping | 0.98F widened to double |

After LocalPlayer updates input and checks sprint, LivingEntity.aiStep enters travel. Relevant order:

1. Decrement `noJumpDelay` by one if positive.
2. Discard tiny velocities. **For players, set both horizontal components to zero when `vx²+vz² < 9.0E-6`**. The per-axis abs < 0.003 check belongs to nonplayers. Set vertical velocity to zero when `abs(vy) < 0.003`.
3. Apply input to xxa/zza/jumping.
4. Evaluate jump conditions and update velocity when eligible.
5. Use onGround at travel start to set floor factor f to 0.6F on ground or 1.0F in air. Horizontal damping is `f * 0.91F`.
6. Ground acceleration is `getSpeed() * (0.21600002F / (f*f*f))`. In air use the flying speed above. Add yaw-transformed input to current velocity.
7. Move using current velocity, clipping displacement at floor collision. Update position, onGround, fallDistance, and related state. The floor collision handler sets vy to zero on vertical impact.
8. After movement, subtract gravity from velocity and then apply vertical damping. Apply the horizontal damping saved in step 5.

```text
vx_next = vx_after_move * (f * 0.91F)
vy_next = (vy_after_move - 0.08) * double(0.98F)
vz_next = vz_after_move * (f * 0.91F)
```

Even stationary on the ground, vy usually ends travel near −0.0784. The next move clips against the floor. Forcing vy to zero at every grounded tick end also changes vertical knockback.

**Phase 2 correction (2026-11-05):** `LivingEntity.getSpeed()` returns its `speed` field, but `Player.getSpeed()` **overrides it** and directly reads the movement-speed attribute. `getFrictionInfluencedSpeed` dispatches to this override for players. Therefore sprint changes affect player ground acceleration in the same movement tick; the parent-class cache does not introduce a one-tick delay. `Player.aiStep` still updates that cache after travel. The Rust implementation and its sprint-transition regression test follow the override. This corrects the earlier Phase 1 interpretation; it was found by reading source, without running Minecraft.

## 4. Jumping

Normal jump power in `LivingEntity.jumpFromGround` (C2) is 0.42F.

```text
vy = max(double(jump_power_float), current_vy)
if sprinting:
    vx += -sin(yaw_float_radians) * 0.2
    vz +=  cos(yaw_float_radians) * 0.2
```

Jumping does not always add 0.42 to vy. The jump function leaves onGround set until the following move detects takeoff, so the initial travel call uses ground coefficients.

With jump held, execute when grounded and `noJumpDelay==0`, then set noJumpDelay=10. The jumping=false branch resets noJumpDelay to zero. Track the Space press separately from its held state.

Track fallDistance independently through ascent, descent, and landing for critical hits. When a movement packet shows a grounded-to-airborne transition and upward displacement, the server calls jumpFromGround, checks falling from actual displacement, and resets fallDistance on upward movement (N1). Attack and Move arrival order can therefore change crit eligibility within the same tick.

## 5. Sprinting, sneaking, and pose

Ctrl **requests sprint start**. On land, starting requires forward input, enough food, no Blindness, no slowing state, and other conditions. Releasing Ctrl alone need not stop sprint. `shouldStopRunSprinting` tests loss of forward input, significant horizontal collisions, and inability to sprint, among other conditions. Forward double-tap also uses `sprintTriggerTime`. Save the client's sprintWindow in the verification profile.

Fixed full hunger always satisfies the food condition here. Holding Ctrl with only S must not enable backward sprinting.

| Pose | Width | Height | Eye height |
| --- | ---: | ---: | ---: |
| Standing | 0.6F | 1.8F | 1.62F |
| Sneaking | 0.6F | 1.5F | 1.27F |

Use dimensions from `Avatar`. Pose changes the AABB and eye origin for reach as well as movement speed.

LocalPlayer.aiStep calculates crouching from old input before calling `input.tick()`. isMovingSlowly reads that crouching state and other flags. Player.tick calls updatePlayerPose near its end. Keep the new Shift key value, movement-slowdown crouching, Pose, and transmitted input separate. Use differential traces to check appearance and AABB on the Shift transition tick.

The server applies PlayerInput shift to `setShiftKeyDown` and updates Pose during Player processing. The opposing RemotePlayer uses the synchronized pose without independently estimating it (N5).

## 6. Floor behavior retained without a block world

Chunks, terrain generation, block arrays, and lighting can be omitted. Keep **plane collision, floor friction, landing, foot support, and downward ray hits on the floor**. A specialized infinite plane at `y=floor_y` suffices.

An infinite plane needs no stepping up, sneak edge stopping, or obstacle avoidance. There are no other blocks to attack or break. A downward click can produce a floor BLOCK hit, distinct from MISS. Make the floor indestructible in the Minecraft reference too.

Retain fall damage. The 26.1.2 calculation is:

```text
raw_fall = floor((fallDistance + 1e-6 - safe_fall_distance) * modifier * fall_damage_multiplier)
Normally safe_fall_distance=3, modifier=1, fall_damage_multiplier=1
Process damage when raw_fall > 0
```

Do not use the older `ceil(fallDistance-3)` formula. The fall tag bypasses armor while the specified Protection still applies. Fall also belongs to no_knockback and thus avoids ordinary sword knockback. Invulnerability can interact across damage types. An ordinary jump alone deals no damage, but retain this path until all supported initial heights, network phases, and knockback histories establish it is unreachable. C2, D1.

## 7. Numerical contract for Rust

- Coordinates, deltaMovement, AABB, and fallDistance generally use f64. HP, damage, input, yaw/pitch, and many coefficients use f32. Preserve intermediate Java casts such as `fallDistance -= (float)ya`.
- `0.6F`, `0.91F`, `0.98F`, and `0.42F` differ from double 0.6, 0.91, 0.98, and 0.42. Preserve the original expression types.
- `Mth.sin/cos` uses a 65,536-entry float table. In 26.1.2 it uses double arguments and a cast to long. Compare against the Java output table before substituting standard-library sin/cos (M5).
- Java Math.round differs from Rust round at exact negative halves. Implement explicit compatible rounding for VecDeltaCodec and LpVec3 quantization.
- Avoid reassociation and FMA. Verify trace agreement with the sequential implementation before introducing SIMD or SoA.
- Isolate RNG per match. Distinguish the official RNG algorithm, initial seed, and draw order from a reproducible training seed.

First require matching boolean decisions for grounded state, reach, strong hits, and invulnerability. A different branch fails conformance even when position error is within tolerance.
