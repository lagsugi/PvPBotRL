use crate::{
    action::{AppliedAction as Action, AppliedKeys as Keys},
    config::Spawn,
    math::{Aabb, Vec3, wrap_degrees},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pose {
    #[default]
    Standing,
    Crouching,
}
impl Pose {
    pub fn height(self) -> f64 {
        match self {
            Self::Standing => 1.8f32 as f64,
            Self::Crouching => 1.5f32 as f64,
        }
    }
    pub fn eye_height(self) -> f64 {
        match self {
            Self::Standing => 1.62f32 as f64,
            Self::Crouching => 1.27f32 as f64,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Equipment {
    /// Remaining durability in head, chest, legs, feet order; zero means absent.
    pub armor: [u16; 4],
    pub sword: u16,
}
impl Default for Equipment {
    fn default() -> Self {
        Self {
            armor: [363, 528, 495, 429],
            sword: 1561,
        }
    }
}
impl Equipment {
    pub fn armor_value(self) -> f32 {
        self.armor
            .iter()
            .zip([3.0, 8.0, 6.0, 3.0])
            .filter_map(|(&n, v)| (n > 0).then_some(v))
            .sum()
    }
    pub fn toughness(self) -> f32 {
        self.armor.iter().filter(|&&n| n > 0).count() as f32 * 2.0
    }
    pub fn protection(self) -> f32 {
        self.armor.iter().filter(|&&n| n > 0).count() as f32 * 4.0
    }
    pub fn attack_speed(self) -> f64 {
        if self.sword > 0 {
            4.0 + (-2.4f32) as f64
        } else {
            4.0
        }
    }
    pub fn attack_damage(self) -> f32 {
        if self.sword > 0 { 7.0 } else { 1.0 }
    }
    pub fn hurt_armor(&mut self, raw: f32) {
        if raw > 0.0 {
            let loss = (raw / 4.0).max(1.0) as u16;
            for piece in &mut self.armor {
                *piece = piece.saturating_sub(loss);
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Body {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub pose: Pose,
    pub on_ground: bool,
    pub fall_distance: f64,
    pub sprinting: bool,
    pub attack_ticks: u32,
    pub cached_speed: f32,
}
impl Body {
    pub fn new(spawn: Spawn, floor: f64) -> Self {
        Self {
            position: spawn.position,
            velocity: Vec3::ZERO,
            yaw: wrap_degrees(spawn.yaw),
            pitch: spawn.pitch.clamp(-90.0, 90.0),
            pose: Pose::Standing,
            on_ground: spawn.position.y == floor,
            fall_distance: 0.0,
            sprinting: false,
            attack_ticks: 20,
            cached_speed: 0.1,
        }
    }
    pub fn bounds(self) -> Aabb {
        let half = (0.6f32 / 2.0) as f64;
        Aabb {
            min: Vec3::new(
                self.position.x - half,
                self.position.y,
                self.position.z - half,
            ),
            max: Vec3::new(
                self.position.x + half,
                self.position.y + self.pose.height(),
                self.position.z + half,
            ),
        }
    }
    pub fn eye(self) -> Vec3 {
        self.position + Vec3::new(0.0, self.pose.eye_height(), 0.0)
    }
    pub fn critical(self) -> bool {
        self.fall_distance > 0.0 && !self.on_ground && !self.sprinting
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerPlayer {
    pub body: Body,
    pub health: f32,
    pub equipment: Equipment,
    /// Cached modifiers are refreshed by the server's equipment-update phase.
    pub armor_attribute: f32,
    pub toughness_attribute: f32,
    pub attack_damage: f32,
    pub attack_speed: f64,
    pub last_hurt: f32,
    pub invulnerable_ticks: u8,
    pub hurt_ticks: u8,
    pub hurt_marked: bool,
    pub needs_sync: bool,
    pub metadata_dirty: bool,
    pub known_movement: Vec3,
    pub received_movement: bool,
    pub input: Keys,
    pub loaded: bool,
    pub first_good: Vec3,
    pub last_good: Vec3,
    pub move_packets: u32,
    pub teleport_id: u32,
    pub awaiting_teleport: Option<u32>,
    pub last_sword_present: bool,
}
impl ServerPlayer {
    pub fn new(spawn: Spawn, floor: f64) -> Self {
        Self {
            body: Body::new(spawn, floor),
            health: 20.0,
            equipment: Equipment::default(),
            armor_attribute: 20.0,
            toughness_attribute: 8.0,
            attack_damage: 7.0,
            attack_speed: 4.0 + (-2.4f32) as f64,
            last_hurt: 0.0,
            invulnerable_ticks: 0,
            hurt_ticks: 0,
            hurt_marked: false,
            needs_sync: false,
            metadata_dirty: false,
            known_movement: Vec3::ZERO,
            received_movement: false,
            input: Keys::default(),
            loaded: true,
            first_good: spawn.position,
            last_good: spawn.position,
            move_packets: 0,
            teleport_id: 0,
            awaiting_teleport: None,
            last_sword_present: true,
        }
    }
    pub fn refresh_attributes(&mut self) {
        self.armor_attribute = self.equipment.armor_value();
        self.toughness_attribute = self.equipment.toughness();
        self.attack_damage = self.equipment.attack_damage();
        self.attack_speed = self.equipment.attack_speed();
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalPlayer {
    pub body: Body,
    pub health: f32,
    pub equipment: Equipment,
    pub input: Keys,
    pub crouching: bool,
    pub jump_delay: u8,
    pub sprint_trigger: u8,
    pub miss_time: u8,
    pub control: Action,
    pub pending_clicks: u32,
    pub sent_position: Vec3,
    pub sent_yaw: f32,
    pub sent_pitch: f32,
    pub sent_ground: bool,
    pub sent_sprint: bool,
    pub sent_input: Keys,
    pub position_reminder: u8,
    pub health_received_ns: u64,
    pub destroying_floor: bool,
    pub last_sword_present: bool,
}
impl LocalPlayer {
    pub fn new(spawn: Spawn, floor: f64) -> Self {
        let mut body = Body::new(spawn, floor);
        if body.on_ground {
            body.velocity.y = -0.08 * (0.98f32 as f64);
        }
        Self {
            body,
            health: 20.0,
            equipment: Equipment::default(),
            input: Keys::default(),
            crouching: false,
            jump_delay: 0,
            sprint_trigger: 0,
            miss_time: 0,
            control: Action::default(),
            pending_clicks: 0,
            sent_position: body.position,
            sent_yaw: body.yaw,
            sent_pitch: body.pitch,
            sent_ground: body.on_ground,
            sent_sprint: false,
            sent_input: Keys::default(),
            position_reminder: 0,
            health_received_ns: 0,
            destroying_floor: false,
            last_sword_present: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Replica {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_received_ns: Option<u64>,
    pub body: Body,
    pub position_base: Vec3,
    pub target_position: Vec3,
    pub target_yaw: f32,
    pub target_pitch: f32,
    pub position_steps: u8,
    pub target_velocity: Vec3,
    pub velocity_steps: u8,
    pub position_received_ns: u64,
    pub metadata_received_ns: u64,
    pub velocity_received_ns: u64,
    /// Observable displacement over the last remote interpolation tick.
    pub observed_displacement: Vec3,
}
impl Replica {
    pub fn new(spawn: Spawn, floor: f64) -> Self {
        let body = Body::new(spawn, floor);
        Self {
            health: None,
            health_received_ns: None,
            body,
            position_base: body.position,
            target_position: body.position,
            target_yaw: body.yaw,
            target_pitch: body.pitch,
            position_steps: 0,
            target_velocity: Vec3::ZERO,
            velocity_steps: 0,
            position_received_ns: 0,
            metadata_received_ns: 0,
            velocity_received_ns: 0,
            observed_displacement: Vec3::ZERO,
        }
    }
    pub fn interpolate_to(&mut self, pos: Vec3, yaw: f32, pitch: f32) {
        if self.position_steps == 0
            || self.target_position != pos
            || self.target_yaw != yaw
            || self.target_pitch != pitch
        {
            self.target_position = pos;
            self.target_yaw = yaw;
            self.target_pitch = pitch;
            self.position_steps = 3;
        }
    }
    pub fn tick(&mut self) {
        let old_position = self.body.position;
        if self.position_steps > 0 {
            let a = 1.0 / self.position_steps as f64;
            self.body.position =
                self.body.position + (self.target_position - self.body.position) * a;
            self.body.yaw = ((self.body.yaw as f64
                + crate::math::wrap_degrees_f64(self.target_yaw as f64 - self.body.yaw as f64) * a)
                as f32)
                % 360.0;
            self.body.pitch = ((self.body.pitch as f64
                + (self.target_pitch as f64 - self.body.pitch as f64) * a)
                as f32)
                % 360.0;
            self.position_steps -= 1;
        }
        if self.velocity_steps > 0 {
            let delta = self.target_velocity - self.body.velocity;
            let n = self.velocity_steps as f64;
            self.body.velocity =
                self.body.velocity + Vec3::new(delta.x / n, delta.y / n, delta.z / n);
            self.velocity_steps -= 1;
        }
        self.observed_displacement = self.body.position - old_position;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tracker {
    pub tick: u64,
    pub position_base: Vec3,
    pub yaw: i8,
    pub pitch: i8,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub pose: Pose,
    pub sprinting: bool,
    pub teleport_delay: u16,
    pub sent_health: f32,
    pub sent_equipment: Equipment,
}
impl Tracker {
    pub fn new(p: ServerPlayer) -> Self {
        Self {
            tick: 0,
            position_base: p.body.position,
            yaw: crate::math::pack_angle(p.body.yaw),
            pitch: crate::math::pack_angle(p.body.pitch),
            velocity: p.body.velocity,
            on_ground: p.body.on_ground,
            pose: p.body.pose,
            sprinting: p.body.sprinting,
            teleport_delay: 0,
            sent_health: p.health,
            sent_equipment: p.equipment,
        }
    }
}
