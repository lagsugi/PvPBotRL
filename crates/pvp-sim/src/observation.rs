//! Actor-visible data only. Authoritative snapshots live in `sim`, outside this API.
use crate::{
    combat::charge,
    config::SimError,
    math::{Vec3, look_vector},
    state::{Body, LocalPlayer, Pose, Replica},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    Entity,
    Floor,
    Miss,
}
pub fn pick(local: Body, remote: Body, floor: f64) -> Pick {
    let eye = local.eye();
    let direction = look_vector(local.yaw, local.pitch);
    let floor_t = if direction.y < 0.0 {
        (floor - eye.y) / direction.y
    } else {
        f64::INFINITY
    };
    let blocked = (0.0..=4.5).contains(&floor_t);
    let limit = if blocked { floor_t } else { 4.5 };
    let bounds = remote.bounds();
    let hit = if bounds.contains(eye) {
        Some(eye)
    } else {
        bounds.clip(eye, eye + direction * 4.5)
    };
    if let Some(hit) = hit {
        let d = (hit - eye).length_squared();
        if d < 9.0 && (d < limit * limit || d == 0.0) {
            return Pick::Entity;
        }
    }
    if blocked { Pick::Floor } else { Pick::Miss }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opponent_health: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opponent_health_age_ns: Option<u64>,
    pub time_ns: u64,
    pub health: f32,
    pub velocity: Vec3,
    pub attack_charge: f32,
    pub sprinting: bool,
    pub sneaking: bool,
    pub walking: bool,
    pub on_ground: bool,
    pub rising: bool,
    pub falling: bool,
    pub yaw: f32,
    pub pitch: f32,
    pub relative_position: Vec3,
    pub opponent_velocity: Vec3,
    /// Observed interpolation displacement; the motion packet velocity is
    /// separate and may remain zero while an opponent walks.
    pub opponent_displacement: Vec3,
    pub opponent_sprinting: bool,
    pub opponent_sneaking: bool,
    pub opponent_walking: bool,
    pub opponent_on_ground: bool,
    pub opponent_rising: bool,
    pub opponent_falling: bool,
    pub distance: f64,
    pub horizontal_distance: f64,
    pub eye_to_target: f64,
    pub target_under_crosshair: bool,
    pub position_age_ns: u64,
    pub metadata_age_ns: u64,
    pub velocity_age_ns: u64,
    pub health_age_ns: u64,
    pub sword_present: bool,
    pub armor_present: [bool; 4],
}
impl Observation {
    pub(crate) fn new(p: &LocalPlayer, r: &Replica, now: u64, floor: f64) -> Self {
        let relative = r.body.position - p.body.position;
        Self {
            opponent_health: r.health,
            opponent_health_age_ns: r.health_received_ns.map(|t| now.saturating_sub(t)),
            time_ns: now,
            health: p.health,
            velocity: p.body.velocity,
            attack_charge: charge(p.body.attack_ticks, p.equipment.attack_speed()),
            sprinting: p.body.sprinting,
            sneaking: p.body.pose == Pose::Crouching,
            walking: p.body.velocity.horizontal_squared() >= 9e-6,
            on_ground: p.body.on_ground,
            rising: !p.body.on_ground && p.body.velocity.y > 0.0,
            falling: !p.body.on_ground && p.body.velocity.y < 0.0,
            yaw: p.body.yaw,
            pitch: p.body.pitch,
            relative_position: relative,
            opponent_velocity: r.body.velocity,
            opponent_displacement: r.observed_displacement,
            opponent_sprinting: r.body.sprinting,
            opponent_sneaking: r.body.pose == Pose::Crouching,
            opponent_walking: r.observed_displacement.horizontal_squared() >= 9e-6,
            opponent_on_ground: r.body.on_ground,
            opponent_rising: !r.body.on_ground && r.observed_displacement.y > 0.0,
            opponent_falling: !r.body.on_ground && r.observed_displacement.y < 0.0,
            distance: relative.length(),
            horizontal_distance: relative.horizontal_squared().sqrt(),
            eye_to_target: r.body.bounds().distance_squared(p.body.eye()).sqrt(),
            target_under_crosshair: pick(p.body, r.body, floor) == Pick::Entity,
            position_age_ns: now.saturating_sub(r.position_received_ns),
            metadata_age_ns: now.saturating_sub(r.metadata_received_ns),
            velocity_age_ns: now.saturating_sub(r.velocity_received_ns),
            health_age_ns: now.saturating_sub(p.health_received_ns),
            sword_present: p.equipment.sword > 0,
            armor_present: p.equipment.armor.map(|v| v > 0),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Health,
    VelocityX,
    VelocityY,
    VelocityZ,
    AttackCharge,
    Sprinting,
    Sneaking,
    Walking,
    OnGround,
    Rising,
    Falling,
    YawSin,
    YawCos,
    Pitch,
    RelativeX,
    RelativeY,
    RelativeZ,
    OpponentVelocityX,
    OpponentVelocityY,
    OpponentVelocityZ,
    OpponentDisplacementX,
    OpponentDisplacementY,
    OpponentDisplacementZ,
    OpponentSprinting,
    OpponentSneaking,
    OpponentWalking,
    OpponentOnGround,
    OpponentRising,
    OpponentFalling,
    Distance,
    HorizontalDistance,
    EyeToTarget,
    TargetUnderCrosshair,
    PositionAge,
    MetadataAge,
    VelocityAge,
    HealthAge,
    SwordPresent,
    HelmetPresent,
    ChestPresent,
    LegsPresent,
    BootsPresent,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationSchema {
    pub version: u32,
    pub features: Vec<Feature>,
}
impl Default for ObservationSchema {
    fn default() -> Self {
        use Feature::*;
        Self {
            version: 1,
            features: vec![
                Health,
                VelocityX,
                VelocityY,
                VelocityZ,
                AttackCharge,
                Sprinting,
                Sneaking,
                Walking,
                OnGround,
                Rising,
                Falling,
                YawSin,
                YawCos,
                Pitch,
                RelativeX,
                RelativeY,
                RelativeZ,
                OpponentVelocityX,
                OpponentVelocityY,
                OpponentVelocityZ,
                OpponentDisplacementX,
                OpponentDisplacementY,
                OpponentDisplacementZ,
                OpponentSprinting,
                OpponentSneaking,
                OpponentWalking,
                OpponentOnGround,
                OpponentRising,
                OpponentFalling,
                Distance,
                HorizontalDistance,
                EyeToTarget,
                TargetUnderCrosshair,
                PositionAge,
                MetadataAge,
                VelocityAge,
                HealthAge,
                SwordPresent,
                HelmetPresent,
                ChestPresent,
                LegsPresent,
                BootsPresent,
            ],
        }
    }
}
impl ObservationSchema {
    pub fn validate(&self) -> Result<(), SimError> {
        if self.version != 1 || self.features.is_empty() {
            return Err(SimError::InvalidConfig(
                "observation schema requires version 1 and at least one feature".into(),
            ));
        }
        let mut seen = std::collections::HashSet::new();
        if self.features.iter().any(|v| !seen.insert(v)) {
            return Err(SimError::InvalidConfig(
                "duplicate observation feature".into(),
            ));
        }
        Ok(())
    }
    /// Positions in blocks, velocities in blocks/tick, ages in seconds, HP / 20,
    /// pitch / 90. Feature order is explicit and must be saved with the policy.
    pub fn project(&self, o: &Observation) -> Result<Vec<f32>, SimError> {
        self.validate()?;
        Ok(self
            .features
            .iter()
            .map(|f| {
                use Feature::*;
                match f {
                    Health => o.health / 20.0,
                    VelocityX => o.velocity.x as f32,
                    VelocityY => o.velocity.y as f32,
                    VelocityZ => o.velocity.z as f32,
                    AttackCharge => o.attack_charge,
                    Sprinting => o.sprinting as u8 as f32,
                    Sneaking => o.sneaking as u8 as f32,
                    Walking => o.walking as u8 as f32,
                    OnGround => o.on_ground as u8 as f32,
                    Rising => o.rising as u8 as f32,
                    Falling => o.falling as u8 as f32,
                    YawSin => o.yaw.to_radians().sin(),
                    YawCos => o.yaw.to_radians().cos(),
                    Pitch => o.pitch / 90.0,
                    RelativeX => o.relative_position.x as f32,
                    RelativeY => o.relative_position.y as f32,
                    RelativeZ => o.relative_position.z as f32,
                    OpponentVelocityX => o.opponent_velocity.x as f32,
                    OpponentVelocityY => o.opponent_velocity.y as f32,
                    OpponentVelocityZ => o.opponent_velocity.z as f32,
                    OpponentDisplacementX => o.opponent_displacement.x as f32,
                    OpponentDisplacementY => o.opponent_displacement.y as f32,
                    OpponentDisplacementZ => o.opponent_displacement.z as f32,
                    OpponentSprinting => o.opponent_sprinting as u8 as f32,
                    OpponentSneaking => o.opponent_sneaking as u8 as f32,
                    OpponentWalking => o.opponent_walking as u8 as f32,
                    OpponentOnGround => o.opponent_on_ground as u8 as f32,
                    OpponentRising => o.opponent_rising as u8 as f32,
                    OpponentFalling => o.opponent_falling as u8 as f32,
                    Distance => o.distance as f32,
                    HorizontalDistance => o.horizontal_distance as f32,
                    EyeToTarget => o.eye_to_target as f32,
                    TargetUnderCrosshair => o.target_under_crosshair as u8 as f32,
                    PositionAge => o.position_age_ns as f32 / 1e9,
                    MetadataAge => o.metadata_age_ns as f32 / 1e9,
                    VelocityAge => o.velocity_age_ns as f32 / 1e9,
                    HealthAge => o.health_age_ns as f32 / 1e9,
                    SwordPresent => o.sword_present as u8 as f32,
                    HelmetPresent => o.armor_present[0] as u8 as f32,
                    ChestPresent => o.armor_present[1] as u8 as f32,
                    LegsPresent => o.armor_present[2] as u8 as f32,
                    BootsPresent => o.armor_present[3] as u8 as f32,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Spawn;
    #[test]
    fn reach_is_strict_and_uses_aabb() {
        let p = Body::new(
            Spawn {
                position: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            },
            0.0,
        );
        let mut r = p;
        r.position.z = 3.0 + (0.6f32 / 2.0) as f64;
        assert_eq!(pick(p, r, 0.0), Pick::Miss);
        r.position.z -= 0.00001;
        assert_eq!(pick(p, r, 0.0), Pick::Entity);
    }
    #[test]
    fn floor_occludes_and_off_target_is_possible() {
        let mut p = Body::new(
            Spawn {
                position: Vec3::ZERO,
                yaw: 0.0,
                pitch: 90.0,
            },
            0.0,
        );
        let mut r = p;
        r.position.z = 2.0;
        assert_eq!(pick(p, r, 0.0), Pick::Floor);
        p.pitch = 0.0;
        p.yaw = 180.0;
        assert_eq!(pick(p, r, 0.0), Pick::Miss);
    }
}
