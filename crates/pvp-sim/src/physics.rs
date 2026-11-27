use crate::{
    action::AppliedKeys as Keys,
    math::{Vec3, cos, radians, sin},
    state::{Body, LocalPlayer, Pose},
};

pub fn movement_speed(sprinting: bool) -> f32 {
    let base = 0.1f32 as f64;
    (if sprinting {
        base * (1.0 + 0.3f32 as f64)
    } else {
        base
    }) as f32
}
pub fn input_vector(keys: Keys, slow: bool) -> (f32, f32) {
    let (mut x, mut z) = keys.axes();
    let n = (x * x + z * z).sqrt();
    if n < 1e-4 {
        return (0.0, 0.0);
    }
    x /= n;
    z /= n;
    x *= 0.98;
    z *= 0.98;
    if slow {
        x *= 0.3;
        z *= 0.3;
    }
    let len = (x * x + z * z).sqrt();
    if len <= 0.0 {
        return (0.0, 0.0);
    }
    let dx = x * (1.0 / len);
    let dz = z * (1.0 / len);
    let (ax, az) = (dx.abs(), dz.abs());
    let tan = if az > ax { ax / az } else { az / ax };
    let scale = (len * (1.0 + tan * tan).sqrt()).min(1.0);
    (dx * scale, dz * scale)
}
pub fn trim_velocity(v: &mut Vec3) {
    if v.horizontal_squared() < 9e-6 {
        v.x = 0.0;
        v.z = 0.0;
    }
    if v.y.abs() < 0.003 {
        v.y = 0.0;
    }
}
pub fn jump(body: &mut Body) {
    body.velocity.y = body.velocity.y.max(0.42f32 as f64);
    if body.sprinting {
        let angle = radians(body.yaw) as f64;
        body.velocity.x += -(sin(angle) as f64) * 0.2;
        body.velocity.z += cos(angle) as f64 * 0.2;
    }
}

/// ServerPlayer travels during its connection tick and snaps back to the latest
/// accepted packet's coordinates. The velocity from that travel is retained.
pub(crate) fn tick_server_velocity(body: &mut Body, floor: f64, friction: f32) {
    trim_velocity(&mut body.velocity);
    let drag = if body.on_ground {
        friction * 0.91
    } else {
        0.91
    };
    body.on_ground = body.velocity.y < 0.0 && body.position.y + body.velocity.y < floor;
    let y = if body.on_ground { 0.0 } else { body.velocity.y };
    body.velocity = Vec3::new(
        body.velocity.x * f64::from(drag),
        (y - 0.08) * f64::from(0.98f32),
        body.velocity.z * f64::from(drag),
    );
}
/// Advances one local movement tick on an immutable horizontal floor.
pub fn tick_local(
    player: &mut LocalPlayer,
    keys: Keys,
    floor: f64,
    friction: f32,
    sprint_window: u8,
) {
    tick_local_with_feedback(player, keys, floor, friction, sprint_window, None);
}

#[derive(Clone, Copy, serde::Deserialize)]
pub struct MovementFeedback {
    pub sprinting: bool,
    pub movement_speed: f32,
}

pub fn tick_local_with_feedback(
    player: &mut LocalPlayer,
    keys: Keys,
    floor: f64,
    friction: f32,
    sprint_window: u8,
    feedback: Option<MovementFeedback>,
) {
    if let Some(received) = feedback {
        player.body.sprinting = received.sprinting;
    }
    player.sprint_trigger = player.sprint_trigger.saturating_sub(1);
    let old_forward = player.input.forward_impulse();
    let old_shift = player.input.sneak;
    // Crouching used for speed is computed before polling the new keys.
    player.crouching = old_shift;
    player.input = keys;
    if old_shift || keys.backward {
        player.sprint_trigger = 0;
    }
    if !player.body.sprinting && keys.forward_impulse() && !player.crouching {
        if !old_forward {
            if player.sprint_trigger > 0 {
                player.body.sprinting = true;
            } else {
                player.sprint_trigger = sprint_window;
            }
        }
        if keys.sprint {
            player.body.sprinting = true;
        }
    }
    if player.body.sprinting && !keys.forward_impulse() {
        player.body.sprinting = false;
    }
    let speed = feedback
        .filter(|received| received.sprinting == player.body.sprinting)
        .map_or_else(
            || movement_speed(player.body.sprinting),
            |received| received.movement_speed,
        );
    travel(player, keys, floor, friction, speed);
}

pub(crate) fn travel(player: &mut LocalPlayer, keys: Keys, floor: f64, friction: f32, speed: f32) {
    player.jump_delay = player.jump_delay.saturating_sub(1);
    trim_velocity(&mut player.body.velocity);
    if keys.jump {
        if player.body.on_ground && player.jump_delay == 0 {
            jump(&mut player.body);
            player.jump_delay = 10;
        }
    } else {
        player.jump_delay = 0;
    }
    let ground_before = player.body.on_ground;
    let f = if ground_before { friction } else { 1.0 };
    let drag = f * 0.91;
    let acceleration = if ground_before {
        // Player overrides LivingEntity.getSpeed(): read the current attribute,
        // not the speed field refreshed after travel by Player.aiStep.
        speed * (0.21600002 / (f * f * f))
    } else if player.body.sprinting {
        0.025999999
    } else {
        0.02
    };
    let (x, z) = input_vector(keys, player.crouching);
    let mut input = Vec3::new(x as f64, 0.0, z as f64);
    if input.length_squared() >= 1e-7 {
        if input.length_squared() > 1.0 {
            input = input.normalize();
        }
        input = input * acceleration as f64;
        let angle = radians(player.body.yaw) as f64;
        let s = sin(angle) as f64;
        let c = cos(angle) as f64;
        player.body.velocity = player.body.velocity
            + Vec3::new(input.x * c - input.z * s, 0.0, input.z * c + input.x * s);
    }
    let desired = player.body.velocity;
    let mut delta = desired;
    if desired.y < 0.0 && player.body.position.y + desired.y < floor {
        delta.y = floor - player.body.position.y;
    }
    // Entity.move skips tiny displacement unless collision made no meaningful change.
    let len = delta.length_squared();
    if len > 1e-7 || desired.length_squared() - len < 1e-7 {
        player.body.position = player.body.position + delta;
    }
    player.body.on_ground = desired.y != delta.y && desired.y < 0.0;
    if delta.y < 0.0 {
        player.body.fall_distance -= (delta.y as f32) as f64;
    }
    if player.body.on_ground {
        player.body.fall_distance = 0.0;
    }
    if desired.y != delta.y {
        player.body.velocity.y = 0.0;
    }
    player.body.velocity = Vec3::new(
        player.body.velocity.x * drag as f64,
        (player.body.velocity.y - 0.08) * (0.98f32 as f64),
        player.body.velocity.z * drag as f64,
    );
    player.body.cached_speed = speed;
    player.body.attack_ticks = player.body.attack_ticks.saturating_add(1);
    let sword = player.equipment.sword > 0;
    if sword != player.last_sword_present {
        player.body.attack_ticks = 0;
        player.last_sword_present = sword;
    }
    player.body.pose = if keys.sneak {
        Pose::Crouching
    } else {
        Pose::Standing
    };
}
pub fn fall_damage(distance: f64) -> f32 {
    ((distance + 1e-6 - 3.0).floor().max(0.0)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Spawn, state::LocalPlayer};
    fn local() -> LocalPlayer {
        LocalPlayer::new(
            Spawn {
                position: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            },
            0.0,
        )
    }
    #[test]
    fn diagonal_and_sneak_are_not_same_speed() {
        let keys = Keys {
            forward: true,
            left: true,
            ..Keys::default()
        };
        let (x, z) = input_vector(keys, false);
        assert!(((x * x + z * z).sqrt() - 1.0).abs() < 1e-6);
        let (x, z) = input_vector(keys, true);
        assert!(((x * x + z * z).sqrt() - 0.4157788).abs() < 1e-6);
    }
    #[test]
    fn opposing_keys_cancel() {
        assert_eq!(
            input_vector(
                Keys {
                    forward: true,
                    backward: true,
                    left: true,
                    right: true,
                    ..Keys::default()
                },
                false
            ),
            (0.0, 0.0)
        );
    }
    #[test]
    fn stationary_floor_keeps_negative_vertical_velocity() {
        let mut p = local();
        for _ in 0..40 {
            tick_local(&mut p, Keys::default(), 0.0, 0.6, 7);
        }
        assert_eq!(p.body.position, Vec3::ZERO);
        assert!(p.body.on_ground);
        assert_eq!(p.body.velocity.y, -0.08 * (0.98f32 as f64));
    }
    #[test]
    fn jump_rises_and_lands() {
        let mut p = local();
        tick_local(
            &mut p,
            Keys {
                jump: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        assert_eq!(p.body.position.y, 0.42f32 as f64);
        assert!(!p.body.on_ground);
        let mut peak = p.body.position.y;
        for _ in 0..30 {
            tick_local(&mut p, Keys::default(), 0.0, 0.6, 7);
            peak = peak.max(p.body.position.y);
        }
        assert!((1.2..1.3).contains(&peak));
        assert!(p.body.on_ground);
        assert_eq!(p.body.position.y, 0.0);
    }
    #[test]
    fn horizontal_threshold_uses_norm() {
        let mut v = Vec3::new(0.0025, 0.002, 0.0025);
        trim_velocity(&mut v);
        assert_eq!(v.x, 0.0025);
        assert_eq!(v.y, 0.0);
    }
    #[test]
    fn sprint_requires_forward_but_not_held_ctrl_to_continue() {
        let mut p = local();
        tick_local(
            &mut p,
            Keys {
                sprint: true,
                backward: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        assert!(!p.body.sprinting);
        tick_local(
            &mut p,
            Keys {
                sprint: true,
                forward: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        assert!(p.body.sprinting);
        tick_local(
            &mut p,
            Keys {
                forward: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        assert!(p.body.sprinting);
    }
    #[test]
    fn fall_uses_floor_not_ceil() {
        assert_eq!(fall_damage(3.99), 0.0);
        assert_eq!(fall_damage(4.0), 1.0);
    }
    #[test]
    fn player_sprint_attribute_changes_acceleration_in_the_same_tick() {
        let mut walking = local();
        let mut sprinting = local();
        tick_local(
            &mut walking,
            Keys {
                forward: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        tick_local(
            &mut sprinting,
            Keys {
                forward: true,
                sprint: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        assert!((sprinting.body.position.z / walking.body.position.z - 1.3).abs() < 1e-6);
        // Deliberately poison the parent-class cache: the Player override must
        // not read it when resolving ground acceleration.
        let mut cached = local();
        cached.body.cached_speed = 123.0;
        tick_local(
            &mut cached,
            Keys {
                forward: true,
                ..Keys::default()
            },
            0.0,
            0.6,
            7,
        );
        assert_eq!(cached.body.position, walking.body.position);
    }
}
