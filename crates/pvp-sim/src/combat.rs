use crate::{
    math::{Vec3, cos, radians, sin},
    rng::JavaRandom,
    state::ServerPlayer,
};
use serde::{Deserialize, Serialize};

pub fn charge(ticks: u32, speed: f64) -> f32 {
    let delay = (1.0 / speed * 20.0) as f32;
    ((ticks as f32 + 0.5) / delay).clamp(0.0, 1.0)
}
pub fn attack_damage(base: f32, q: f32, critical: bool) -> f32 {
    let damage = base * (0.2 + q * q * 0.8);
    if critical { damage * 1.5 } else { damage }
}
pub fn after_armor(d: f32, armor: f32, toughness: f32) -> f32 {
    let t = 2.0 + toughness / 4.0;
    let a = (armor - d / t).clamp(armor * 0.2, 20.0);
    d * (1.0 - a / 25.0)
}
pub fn after_protection(d: f32, protection: f32) -> f32 {
    d * (1.0 - protection.clamp(0.0, 20.0) / 25.0)
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageResult {
    pub accepted: bool,
    pub full_hit: bool,
    pub raw: f32,
    pub applied_raw: f32,
    pub after_armor: f32,
    pub final_damage: f32,
    pub health_lost: f32,
}
impl DamageResult {
    fn rejected(raw: f32) -> Self {
        Self {
            accepted: false,
            full_hit: false,
            raw,
            applied_raw: 0.0,
            after_armor: 0.0,
            final_damage: 0.0,
            health_lost: 0.0,
        }
    }
}
pub fn hurt(victim: &mut ServerPlayer, raw: f32, fall: bool, unbreakable: bool) -> DamageResult {
    if victim.health <= 0.0 || !victim.loaded || raw <= 0.0 {
        return DamageResult::rejected(raw);
    }
    let protected = victim.invulnerable_ticks > 10;
    if protected && raw <= victim.last_hurt {
        return DamageResult::rejected(raw);
    }
    let d = if protected {
        raw - victim.last_hurt
    } else {
        raw
    };
    victim.last_hurt = raw;
    if !protected {
        victim.invulnerable_ticks = 20;
        victim.hurt_ticks = 10;
        victim.hurt_marked = true;
    }
    // Durability changes before armor/protection. Attributes remain cached until
    // equipment detection, while Protection reads the surviving stacks immediately.
    if !fall && !unbreakable {
        victim.equipment.hurt_armor(d);
    }
    let armor = if fall {
        d
    } else {
        after_armor(d, victim.armor_attribute, victim.toughness_attribute)
    };
    let final_damage = after_protection(armor, victim.equipment.protection());
    let old = victim.health;
    victim.health = (victim.health - final_damage).clamp(0.0, 20.0);
    victim.metadata_dirty |= victim.health != old;
    if victim.health <= 0.0 {
        victim.loaded = false;
    }
    DamageResult {
        accepted: true,
        full_hit: !protected,
        raw,
        applied_raw: d,
        after_armor: armor,
        final_damage,
        health_lost: old - victim.health,
    }
}
pub fn knockback(
    velocity: &mut Vec3,
    on_ground: bool,
    power: f64,
    mut x: f64,
    mut z: f64,
    rng: &mut JavaRandom,
) {
    if power <= 0.0 {
        return;
    }
    while x * x + z * z < (1e-5f32 as f64) {
        x = (rng.next_double() - rng.next_double()) * 0.01;
        z = (rng.next_double() - rng.next_double()) * 0.01;
    }
    let direction = Vec3::new(x, 0.0, z).normalize() * power;
    *velocity = Vec3::new(
        velocity.x / 2.0 - direction.x,
        if on_ground {
            (velocity.y / 2.0 + power).min(0.4)
        } else {
            velocity.y
        },
        velocity.z / 2.0 - direction.z,
    );
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttackResult {
    pub charge: f32,
    pub critical: bool,
    pub sprint_hit: bool,
    #[serde(default)]
    pub sweep: bool,
    pub damage: DamageResult,
    pub motion: Option<Vec3>,
}
pub fn attack(
    attacker: &mut ServerPlayer,
    victim: &mut ServerPlayer,
    rng: &mut JavaRandom,
    unbreakable: bool,
) -> AttackResult {
    let q = charge(attacker.body.attack_ticks, attacker.attack_speed);
    let strong = q > 0.9;
    let critical = strong && attacker.body.critical();
    let sprint_hit = strong && attacker.body.sprinting;
    // Player.isSweepAttack queries the server's last reported displacement
    // and the current speed attribute before knockback or sword durability changes.
    let sweep_limit = f64::from(crate::physics::movement_speed(attacker.body.sprinting)) * 2.5;
    let sweep = strong
        && !critical
        && !sprint_hit
        && attacker.body.on_ground
        && attacker.equipment.sword > 0
        && attacker.known_movement.horizontal_squared() < sweep_limit * sweep_limit;
    let raw = attack_damage(attacker.attack_damage, q, critical);
    attacker.body.attack_ticks = 0;
    let old = victim.body.velocity;
    let result = hurt(victim, raw, false, unbreakable);
    let mut motion = None;
    if result.accepted {
        if result.full_hit {
            knockback(
                &mut victim.body.velocity,
                victim.body.on_ground,
                0.4f32 as f64,
                attacker.body.position.x - victim.body.position.x,
                attacker.body.position.z - victim.body.position.z,
                rng,
            );
            victim.needs_sync = true;
        }
        if sprint_hit {
            victim.needs_sync = true;
            let angle = radians(attacker.body.yaw) as f64;
            knockback(
                &mut victim.body.velocity,
                victim.body.on_ground,
                0.5,
                sin(angle) as f64,
                -cos(angle) as f64,
                rng,
            );
            victim.needs_sync = true;
            attacker.body.velocity = attacker.body.velocity.horizontal_scale(0.6);
            attacker.body.sprinting = false;
            attacker.metadata_dirty = true;
        }
        if victim.hurt_marked {
            motion = Some(victim.body.velocity);
            victim.hurt_marked = false;
            victim.body.velocity = old;
        }
        if !unbreakable {
            attacker.equipment.sword = attacker.equipment.sword.saturating_sub(1);
        }
    }
    AttackResult {
        charge: q,
        critical,
        sprint_hit,
        sweep,
        damage: result,
        motion,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Spawn;
    fn p(z: f64) -> ServerPlayer {
        ServerPlayer::new(
            Spawn {
                position: Vec3::new(0.0, 0.0, z),
                yaw: 0.0,
                pitch: 0.0,
            },
            0.0,
        )
    }
    #[test]
    fn phase1_damage_float_bits() {
        for (n, bits) in [
            (0, 0x3dde6555),
            (1, 0x3dea7e1b),
            (5, 0x3e4de6af),
            (10, 0x3efa0e6a),
            (11, 0x3f13e962),
            (12, 0x3f2e2eb0),
            (13, 0x3f2e2eb1),
        ] {
            let d = attack_damage(7.0, charge(n, 4.0 + (-2.4f32) as f64), false);
            assert_eq!(
                after_protection(after_armor(d, 20.0, 8.0), 16.0).to_bits(),
                bits,
                "n={n}"
            );
        }
    }
    #[test]
    fn invulnerability_uses_raw_difference() {
        let mut v = p(1.0);
        hurt(&mut v, 7.0, false, true);
        let r = hurt(&mut v, 10.5, false, true);
        assert_eq!(r.applied_raw, 3.5);
        assert!(!r.full_hit);
        assert!((r.final_damage - 0.2961).abs() < 1e-6);
        assert_eq!(v.invulnerable_ticks, 20);
        assert!(!hurt(&mut v, 10.5, false, true).accepted);
        v.invulnerable_ticks = 10;
        assert!(hurt(&mut v, 7.0, false, true).full_hit);
    }
    #[test]
    fn sprint_knockback_is_sequential_and_server_velocity_restored() {
        let mut a = p(0.0);
        let mut b = p(2.0);
        a.body.sprinting = true;
        let r = attack(&mut a, &mut b, &mut JavaRandom::new(1), true);
        let motion = r.motion.unwrap();
        assert!((motion.z - 0.7).abs() < 1e-7);
        assert_eq!(b.body.velocity, Vec3::ZERO);
        assert!(!a.body.sprinting);
        assert!(!r.critical);
    }
    #[test]
    fn airborne_knockback_preserves_vertical_velocity() {
        let mut v = Vec3::new(0.2, -0.3, 0.0);
        knockback(&mut v, false, 0.4, 1.0, 0.0, &mut JavaRandom::new(1));
        assert_eq!(v.y, -0.3);
        assert!((v.x + 0.3).abs() < 1e-12);
    }
    #[test]
    fn rejected_hit_consumes_attacker_charge() {
        let mut a = p(0.0);
        let mut b = p(2.0);
        hurt(&mut b, 20.0, false, true);
        let r = attack(&mut a, &mut b, &mut JavaRandom::new(1), true);
        assert!(!r.damage.accepted);
        assert_eq!(a.body.attack_ticks, 0);
        assert_eq!(a.equipment.sword, 1561);
    }
    #[test]
    fn fixed_equipment_preserves_sword_and_protection_on_a_hit() {
        let mut attacker = p(0.0);
        let mut victim = p(2.0);
        attacker.equipment.sword = 1;
        victim.equipment.armor = [1; 4];
        let result = attack(&mut attacker, &mut victim, &mut JavaRandom::new(1), true);
        assert!(result.damage.accepted);
        assert_eq!(attacker.equipment.sword, 1);
        assert_eq!(victim.equipment.armor, [1; 4]);
        assert_eq!(
            result.damage.final_damage,
            after_protection(after_armor(7.0, 20.0, 8.0), 16.0)
        );
    }
    #[test]
    fn armor_break_removes_protection_then_cached_attributes_refresh() {
        let mut b = p(2.0);
        b.equipment.armor[0] = 1;
        let r = hurt(&mut b, 7.0, false, false);
        assert_eq!(b.equipment.protection(), 12.0);
        assert_eq!(b.armor_attribute, 20.0);
        assert_eq!(
            r.final_damage,
            after_protection(after_armor(7.0, 20.0, 8.0), 12.0)
        );
        b.refresh_attributes();
        assert_eq!(b.armor_attribute, 17.0);
    }
    #[test]
    fn overlap_randomness_is_reproducible() {
        let mut a = Vec3::ZERO;
        let mut b = a;
        knockback(&mut a, true, 0.4, 0.0, 0.0, &mut JavaRandom::new(42));
        knockback(&mut b, true, 0.4, 0.0, 0.0, &mut JavaRandom::new(42));
        assert_eq!(a, b);
        assert!(a.finite());
    }
    #[test]
    fn falling_critical_and_sprinting_exclusion() {
        let mut a = p(0.0);
        let mut b = p(2.0);
        a.body.on_ground = false;
        a.body.fall_distance = 0.5;
        let result = attack(&mut a, &mut b, &mut JavaRandom::new(42), true);
        assert!(result.critical);
        assert_eq!(result.damage.raw, 10.5);
        assert_eq!(result.damage.final_damage.to_bits(), 1.1529001f32.to_bits());
        a.body.attack_ticks = 20;
        a.body.sprinting = true;
        let result = attack(&mut a, &mut p(2.0), &mut JavaRandom::new(42), true);
        assert!(!result.critical);
        assert!(result.sprint_hit);
        assert_eq!(result.damage.raw, 7.0);
    }
    #[test]
    fn protected_incremental_hit_has_only_sprint_knockback_and_no_direct_motion() {
        let mut a = p(0.0);
        let mut b = p(2.0);
        a.body.sprinting = true;
        hurt(&mut b, 1.0, false, true);
        b.hurt_marked = false;
        let result = attack(&mut a, &mut b, &mut JavaRandom::new(42), true);
        assert!(result.damage.accepted);
        assert!(!result.damage.full_hit);
        assert_eq!(result.damage.applied_raw, 6.0);
        assert!(result.motion.is_none());
        assert!((b.body.velocity.z - 0.5).abs() < 1e-10);
        assert!(b.needs_sync);
    }
}
