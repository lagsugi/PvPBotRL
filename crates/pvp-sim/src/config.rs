use crate::math::Vec3;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const TICK_NS: u64 = 50_000_000;
pub const SECOND_NS: u64 = 1_000_000_000;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClientTiming {
    pub upstream_ns: u64,
    pub downstream_ns: u64,
    pub fps: u32,
    pub frame_phase_ns: u64,
    pub tick_phase_ns: u64,
}
impl Default for ClientTiming {
    fn default() -> Self {
        Self {
            upstream_ns: 10_000_000,
            downstream_ns: 10_000_000,
            fps: 60,
            frame_phase_ns: 0,
            tick_phase_ns: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spawn {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RewardConfig {
    pub win: f32,
    pub damage: f32,
    pub time_penalty: f32,
}
impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            win: 1.0,
            damage: 0.0,
            time_penalty: 0.0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerRules {
    #[default]
    Legacy,
    PaperSwing,
    PaperFlatV1,
    PaperFlatV2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuelDriver {
    #[default]
    ClientServer,
    /// Paired Paper NPCs, with one decision opportunity per server tick.
    PaperCitizens,
}

impl DuelDriver {
    fn is_client_server(&self) -> bool {
        *self == Self::ClientServer
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchProfile {
    SwordPvpFixedEquipmentV1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub expose_opponent_health: bool,
    pub ping_profile: Option<crate::ping::PingProfile>,
    pub clients: [ClientTiming; 2],
    pub spawns: [Spawn; 2],
    pub initial_health: [f32; 2],
    pub floor_y: f64,
    pub floor_friction: f32,
    pub max_steps: u64,
    pub server_phase_ns: u64,
    /// Existing serialized configurations without this field retain their dynamics.
    #[serde(default)]
    pub server_rules: ServerRules,
    /// Paper Citizens owns both movement and attack on the server tick clock.
    /// Client FPS and phases and the packet replay's server rules apply to ClientServer.
    #[serde(skip_serializing_if = "DuelDriver::is_client_server")]
    pub duel_driver: DuelDriver,
    pub sprint_window_ticks: u8,
    pub unbreakable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_profile: Option<MatchProfile>,
    /// First seat at time zero. None uses the seed for ClientServer and seat A
    /// for PaperCitizens. PaperCitizens alternates the lead seat each tick.
    pub first_client: Option<usize>,
    pub reward: RewardConfig,
    /// Diagnostic logs are opt-in, and bounded to prevent training memory growth.
    pub trace_capacity: usize,
    /// Expensive phase snapshots, only for differential validation; never actor input.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub trace_states: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            expose_opponent_health: false,
            ping_profile: None,
            clients: std::array::from_fn(|_| ClientTiming::default()),
            spawns: [
                Spawn {
                    position: Vec3::new(0.0, 0.0, -10.0),
                    yaw: 0.0,
                    pitch: 0.0,
                },
                Spawn {
                    position: Vec3::new(0.0, 0.0, 10.0),
                    yaw: 180.0,
                    pitch: 0.0,
                },
            ],
            floor_y: 0.0,
            initial_health: [20.; 2],
            floor_friction: 0.6,
            max_steps: 6000,
            server_phase_ns: 0,
            server_rules: ServerRules::Legacy,
            duel_driver: DuelDriver::ClientServer,
            sprint_window_ticks: 7,
            unbreakable: false,
            match_profile: None,
            first_client: None,
            reward: RewardConfig::default(),
            trace_capacity: 0,
            trace_states: false,
        }
    }
}
impl Config {
    pub fn with_fixed_equipment(mut self) -> Self {
        self.unbreakable = true;
        self.match_profile = Some(MatchProfile::SwordPvpFixedEquipmentV1);
        self
    }

    pub fn validate(&self) -> Result<(), SimError> {
        if self.match_profile == Some(MatchProfile::SwordPvpFixedEquipmentV1) && !self.unbreakable {
            return Err(SimError::InvalidConfig(
                "sword_pvp_fixed_equipment_v1 requires unbreakable equipment".into(),
            ));
        }
        if let Some(profile) = &self.ping_profile {
            profile.validate()?;
        }
        let bad = |s: &str| Err(SimError::InvalidConfig(s.into()));
        if self
            .initial_health
            .iter()
            .any(|&hp| !hp.is_finite() || hp <= 0. || hp > 20.)
        {
            return bad("initial_health must be finite and in (0, 20]");
        }
        if self.max_steps == 0 || self.max_steps > 10_000_000 {
            return bad("max_steps must be in 1..=10000000");
        }
        if !self.floor_y.is_finite() || self.floor_y.abs() > 1_000_000.0 {
            return bad("invalid floor_y");
        }
        if !self.floor_friction.is_finite() || !(0.1..=1.0).contains(&self.floor_friction) {
            return bad("floor_friction must be in 0.1..=1.0");
        }
        if self.server_phase_ns >= TICK_NS {
            return bad("server_phase_ns must be less than one tick");
        }
        if self.first_client.is_some_and(|v| v > 1) {
            return bad("first_client must be 0, 1, or null");
        }
        if [
            self.reward.win,
            self.reward.damage,
            self.reward.time_penalty,
        ]
        .iter()
        .any(|v| !v.is_finite() || v.abs() > 1e6)
        {
            return bad("invalid reward coefficients");
        }
        for (spawn, timing) in self.spawns.iter().zip(&self.clients) {
            if !spawn.position.finite()
                || spawn.position.x.abs() > 1e6
                || spawn.position.z.abs() > 1e6
                || spawn.position.y < self.floor_y
                || spawn.position.y > self.floor_y + 1000.0
                || !spawn.yaw.is_finite()
                || !spawn.pitch.is_finite()
            {
                return bad("invalid spawn");
            }
            if !(20..=1000).contains(&timing.fps)
                || timing.frame_phase_ns >= SECOND_NS / timing.fps as u64
                || timing.tick_phase_ns >= TICK_NS
            {
                return bad("invalid client frame rate or phase");
            }
            if timing.upstream_ns > 10 * SECOND_NS || timing.downstream_ns > 10 * SECOND_NS {
                return bad("one-way latency must not exceed 10 seconds");
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimError {
    InvalidConfig(String),
    InvalidAction(String),
    InvalidPlayer(usize),
    EpisodeFinished,
    InvalidBatch(String),
    Replay(String),
}
impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(s) => write!(f, "invalid config: {s}"),
            Self::InvalidAction(s) => write!(f, "invalid action: {s}"),
            Self::InvalidPlayer(p) => write!(f, "invalid player index: {p}"),
            Self::EpisodeFinished => write!(f, "episode finished; reset before stepping"),
            Self::InvalidBatch(s) => write!(f, "invalid batch: {s}"),
            Self::Replay(s) => write!(f, "invalid replay: {s}"),
        }
    }
}
impl std::error::Error for SimError {}
