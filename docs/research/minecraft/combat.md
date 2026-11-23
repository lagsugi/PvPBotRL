# Combat, equipment, and damage

Reference codes correspond to [source-index.md](source-index.md). Unless otherwise indicated, details were verified in source. Decimal formulas illustrate behavior. Preserve Java float/double boundaries in the implementation.

## 1. Effective values for the fixed loadout

| Parameter | Value under the specified conditions | Source |
| --- | --- | --- |
| Initial/maximum HP | 20.0, `float` | LivingEntity, Attributes, and initial conditions |
| Attack damage | 7.0 = player 1 + sword base 3 + diamond material 3 | C1, C4 |
| Attack speed | Nominally 1.6 attacks/second = 4 − 2.4 | C1, C4, C5 |
| Armor | 20 = helmet 3 + chestplate 8 + leggings 6 + boots 3 | C4 |
| Armor toughness | 8 = 2 per piece | C4 |
| Total Protection | 16 = IV × four pieces | C6 |
| Knockback resistance | 0 (diamond armor adds none) | C4, C5 |
| Weapon knockback attribute/enchantment | 0 | C2, C5 |
| Sweeping damage ratio | Applies only to secondary targets, absent in 1v1 | C1 |

Rust can represent these effective values and equipment conditions in state/configuration without a general item registry or enchantment evaluator.

## 2. Attack cooldown

`Player.getCurrentItemAttackStrengthDelay`, `getAttackStrengthScale`, `baseDamageScaleFactor` and `attack` (C1).

```text
T = float((1.0 / attack_speed_double) * 20.0)
q = clamp_f32((attackStrengthTicker + 0.5F) / T, 0F, 1F)
raw = 7F * (0.2F + q*q*0.8F)
strong = q > 0.9F
```

`attackStrengthTicker` is an integer. It increments once after `super.tick()` in `Player.tick`. Attack processing reads q and the damage coefficient before `onAttack()` resets the timer to zero. Reaching that point consumes the attacker's timer even when victim invulnerability rejects damage. If preliminary server checks such as target or range reject the attack before calling `Player.attack`, the timer follows a different path.

**Float precision matters.** The sword speed modifier widens `-2.4F` to double. The attribute is therefore `1.5999999046325684` and float T is `12.500000953674316`. Preserve this value instead of rounding to a nominal 12.5 ticks. `n=11` yields strong, `n=12` gives q close to but strictly below 1, and `n=13` clamps it to 1. The damage difference is small but matters for bitwise checks. [Numerical results](evidence/probe-results.json)

The game does not automatically attack every 12 ticks. The sword has no minimum attack charge, so an early click can produce a weaker attack. Track clicks separately from cooldown.

### Misses and left clicks

On MISS, `Minecraft.startAttack` resets the client cooldown and sets `missTime=10` in Survival. `continueAttack(false)` clears missTime, so a miss need not block attacks for a full 500 ms. Model the actual press/release sequence and call order.

Sword attacks against entities start from click events in `keyAttack.consumeClick()`. Holding the left button has different semantics from attacking with the sword every tick. Under these conditions `continueAttack` primarily handles block breaking.

A MISS sends Swing alone. Server `handleAnimate` only calls `player.swing()` and leaves the attack timer unchanged. Track the client's observed cooldown separately from the server timer. C7, N1.

## 3. Critical hits and sprint attacks

Critical-hit conditions (C1):

```text
q > 0.9F
fallDistance > 0
!onGround
!onClimbable
!inWater
!isMobilityRestricted
!isPassenger
target is LivingEntity
!isSprinting
```

When eligible, multiply base damage by 1.5F. The specified sword has no extra enchantment damage. Use the server's last accepted grounded state and fallDistance. `vy < 0` alone is insufficient. A rising player cannot crit even with jump held. Sneaking alone does not disqualify a critical hit.

A sprint attack requires `isSprinting && q > 0.9F` and is mutually exclusive with a critical hit. If damage succeeds, apply extra knockback of 0.5F. When extra knockback is positive, multiply the attacker's horizontal velocity by 0.6 and clear sprint. Weak sprint attacks and wholly rejected hits do not uniformly trigger this behavior.

The client also executes `player.attack(remoteEntity)` immediately after sending Attack. Since `RemotePlayer.hurtClient` returns true, this local prediction can slow the attacker and cancel sprint independently of server damage acceptance (C7, N5). Holding Ctrl can requalify the player for sprint in later input processing of the same client tick. Reproducing W-tapping and sprint recovery requires more than one sprint flag.

## 4. Damage order and invulnerability

The call path is `ServerPlayer.hurtServer → Player.hurtServer → LivingEntity.hurtServer → Player.actuallyHurt` (C1, C2, N2). It targets a living opponent after checks such as PvP permission, game mode, and load completion.

Track at least `health: f32`, `lastHurt: f32`, `invulnerableTime: i32`, and `hurtTime` separately. The ten-tick visual effect and 20-count damage filter serve different purposes.

| Condition | Result |
| --- | --- |
| `invulnerableTime > 10` and `raw <= lastHurt` | Return false. HP and normal knockback stay unchanged |
| `invulnerableTime > 10` and `raw > lastHurt` | Pass `raw-lastHurt` to mitigation, update lastHurt to raw, and leave the timer running without resetting it to 20 |
| `invulnerableTime <= 10` | Pass full raw damage to mitigation, set lastHurt=raw, invulnerableTime=20, and hurtTime=10 |

Here raw is damage **before** armor and Protection. A difference hit skips the normal hit-knockback block, but returns true to its caller. A qualifying sprint can therefore still apply extra knockback during invulnerability.

ServerPlayer's invulnerableTime decreases in `ServerPlayer.tick`. LivingEntity has a branch to avoid decrementing it twice for ServerPlayer. Packet processing precedes this decrement, making boundary order significant. [Tick specification](network-and-ticks.md)

## 5. Mitigation and HP

`CombatRules`, `Player.actuallyHurt`, `EnchantmentHelper` (C3, C1, C6). With no absorption hearts, potions, or shield:

```text
d = raw damage that passed the invulnerability check
t = 2F + 8F / 4F = 4F
armor_eff = clamp_f32(20F - d/t, 20F*0.2F, 20F)
d_armor = d * (1F - armor_eff/25F)
d_final = d_armor * (1F - clamp_f32(16F, 0F, 20F)/25F)
health = clamp_f32(health - d_final, 0F, max_health)
```

In the specified normal range this simplifies conceptually to `d_final = d × (0.2 + d/100) × 0.36`. Retain the original order of operations in the conforming Rust implementation because reordering changes float rounding.

| Situation | Damage d passed to mitigation | Approximate HP loss |
| --- | ---: | ---: |
| Fully charged normal hit | 7 | 0.6804 |
| Fully charged critical hit | 10.5 | 1.1529 |
| raw=10.5 during the protected interval after raw=7 | 3.5 | 0.2961 |

Using `1.1529 - 0.6804 = 0.4725` for the last row gives the wrong result. Protection IV × four leaves 36% of the damage after armor mitigation. Keep fractional HP without rounding to integers or half-HP increments for the heart display.

The table was checked with a limited probe of the official CombatRules class on the path without weapon enchantment changes to armor effectiveness. The probe covers neither all Player state transitions nor the complete equipment enchantment aggregation.

Determine death when server HP reaches zero or below. The same attack call may still apply knockback and other effects after HP decreases, so finish processing the event before recording the result. Two attacks arriving simultaneously are processed sequentially from the queue. Preserve that order when deciding survival.

## 6. Knockback

For a normal hit, derive horizontal direction from the players' position difference. Conceptual formula for `LivingEntity.knockback` (C2):

```text
k = power * (1 - knockback_resistance)
u = normalize((attacker_x - victim_x, attacker_z - victim_z))
vx' = vx/2 - u.x*k
vz' = vz/2 - u.z*k
vy' = onGround ? min(0.4, vy/2 + k) : vy
```

Normal-hit power is **0.4F widened to double**. If squared direction length is below `1.0E-5F`, resample `(nextDouble-nextDouble)*0.01` for each axis. With pushing disabled, the players can overlap in 1v1, making this branch relevant. Include seed and RNG consumption in reproduction conditions.

Sprint knockback uses a separate call with power=0.5F and direction `(sin(yaw), -cos(yaw))` from the attacker's yaw. Normal knockback follows position difference. The two directions can differ.

With aligned directions, both calls applied, and zero initial velocity, the conceptual horizontal magnitude is `0.4/2 + 0.5 = 0.7`. A simple sum would give 0.9. A nonzero initial horizontal component is quartered before adding the directional terms. Each airborne knockback call preserves vy. Even on the ground, a negative initial vy can make the normal vertical component differ from 0.4.

**ServerPlayer-specific send and restore** (C1 `causeExtraKnockback`):

1. Save the victim's pre-attack `deltaMovement`.
2. Apply normal and extra knockback.
3. Send `ClientboundSetEntityMotionPacket` to a ServerPlayer with `hurtMarked` set.
4. Set `hurtMarked=false` and restore the victim's server deltaMovement to its saved value.
5. The victim's client **replaces** its velocity with the received value.

Moving every player server-side after adding knockback changes knockback onset, risks double application, and changes the velocity read by the next attack. For difference hits that leave hurtMarked clear, also trace the ordinary tracking update path (N4).

## 7. Omissible effects and equipment durability

Sweeping requires strong, non-critical, no sprint knockback, grounded, known horizontal movement below `(getSpeed()*2.5)^2`, and the sword tag. The secondary-target loop excludes attacker and primary target. In 1v1 with no other entities it adds no HP loss or knockback, so secondary search and effects can be omitted. The primary target receives no extra point of sweep damage.

Attack sounds, particles, statistics, advancements, experience, drops, and item models can be omitted. Skipping their RNG draws can change knockback draws when players overlap. Record or inject random values for trace verification. A common seed alone may be insufficient.

**Durability can affect combat.** Armor durability loss is `int(max(1F, d/4F))`, and the weapon has Weapon(1). New diamond equipment has durability 363 (helmet), 528 (chestplate), 495 (leggings), 429 (boots), and 1561 (sword), per C4. Broken armor changes total armor, toughness, and Protection. Since wear occurs during mitigation, also verify the hit on which a piece breaks.

Omitting the item system still allows finite durability. The initial design stores **durability counters in fixed equipment slots**. To maintain full Protection IV throughout an experiment, explicitly configure unbreakable equipment on both simulator and reference sides. Claims of vanilla agreement must account for wear.
