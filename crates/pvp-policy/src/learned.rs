// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

//! Observation-only linear policy. The trainer and the shipped bot share this action decoder.
use pvp_sim::{Action, Keys, Observation};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub const FEATURES: usize = 10;
pub const OUTPUTS: usize = 4;
pub type Weights = [[f64; FEATURES]; OUTPUTS];

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinearPolicy {
    pub weights: Weights,
}

fn base_features(o: &Observation) -> [f64; FEATURES] {
    // Project horizontal motion onto the line of approach so both seats share a policy.
    let d = o.horizontal_distance.max(1e-6);
    let closing = (o.velocity.x * o.relative_position.x + o.velocity.z * o.relative_position.z) / d;
    [
        1.0,
        ((d - 3.0) / 3.0).clamp(-1.0, 2.0),
        (1.0 - f64::from(o.attack_charge)) * 2.0,
        f64::from(o.rising),
        f64::from(o.falling),
        f64::from(o.sprinting),
        o.relative_position.y.clamp(-1.5, 1.5),
        f64::from(o.opponent_on_ground),
        (3.0 * closing).clamp(-1.5, 1.5),
        (3.0 - o.eye_to_target).clamp(-1.0, 1.0),
    ]
}

fn decode<const N: usize>(
    weights: &[[f64; N]; OUTPUTS],
    features: [f64; N],
    o: &Observation,
) -> Action {
    let [advance, strafe, jump, swing] =
        weights.map(|row| row.iter().zip(features).map(|(w, x)| w * x).sum::<f64>());
    // Approach from outside the engagement area. Inside, learn all movement decisions.
    let far = o.horizontal_distance > 8.0;
    Action {
        keys: Keys {
            forward: far || advance > 0.2,
            backward: !far && advance < -0.2,
            left: !far && strafe > 0.0,
            jump: o.on_ground && jump > 0.0,
            ..Keys::default()
        },
        click: o.eye_to_target < 3.0 && swing > 0.0,
        ..Action::default()
    }
}

impl LinearPolicy {
    pub fn act(&self, o: &Observation) -> Action {
        decode(&self.weights, base_features(o), o)
    }
}

pub fn trained() -> &'static LinearPolicy {
    static POLICY: OnceLock<LinearPolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        serde_json::from_str(include_str!("../../../models/bundled/duel_rl.json"))
            .expect("embedded Duel-RL weights")
    })
}

pub const LEAGUE_FEATURES: usize = FEATURES + 6;

pub fn league_trained() -> &'static LeaguePolicy {
    static POLICY: OnceLock<LeaguePolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        serde_json::from_str(include_str!("../../../models/bundled/league_rl.json"))
            .expect("embedded League-RL weights")
    })
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaguePolicy {
    pub weights: [[f64; LEAGUE_FEATURES]; OUTPUTS],
}

impl LeaguePolicy {
    pub fn from_duel_rl() -> Self {
        let mut weights = [[0.0; LEAGUE_FEATURES]; OUTPUTS];
        for (row, original) in weights.iter_mut().zip(&trained().weights) {
            row[..FEATURES].copy_from_slice(original);
        }
        Self { weights }
    }

    pub fn act(&self, o: &Observation) -> Action {
        decode(&self.weights, league_features(o), o)
    }
}

fn league_features(o: &Observation) -> [f64; LEAGUE_FEATURES] {
    let r = o.relative_position;
    let v = o.opponent_displacement;
    let scale = 3.0 / o.horizontal_distance.max(1e-6);
    let mut x = [0.0; LEAGUE_FEATURES];
    x[..FEATURES].copy_from_slice(&base_features(o));
    x[FEATURES..].copy_from_slice(&[
        ((r.x * v.x + r.z * v.z) * scale).clamp(-1.5, 1.5),
        ((r.x * v.z - r.z * v.x) * scale).abs().min(1.5),
        (o.position_age_ns as f64 / 100_000_000.0).min(2.0),
        f64::from(o.opponent_sprinting),
        (3.0 * o.velocity.y).clamp(-1.5, 1.5),
        f64::from(o.attack_charge >= 1.0),
    ]);
    x
}

pub const G5_FEATURES: usize = LEAGUE_FEATURES + 12;

pub fn g5_trained() -> &'static G5Policy {
    static POLICY: OnceLock<G5Policy> = OnceLock::new();
    POLICY.get_or_init(|| {
        serde_json::from_str(include_str!("../../../models/bundled/league_g5.json"))
            .expect("embedded G5 weights")
    })
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct G5Policy {
    pub weights: [[f64; G5_FEATURES]; OUTPUTS],
}

/// Private to one seat in one episode. All updates come from that client's observations and clicks.
#[derive(Default)]
pub struct G5Memory {
    previous: Option<(f32, f64)>,
    hurt: f64,
    recent_swing: f64,
    approach: f64,
    opponent_air: f64,
}
impl G5Policy {
    pub fn warm_start(base: &LeaguePolicy) -> Self {
        let mut weights = [[0.0; G5_FEATURES]; OUTPUTS];
        for (row, original) in weights.iter_mut().zip(&base.weights) {
            row[..LEAGUE_FEATURES].copy_from_slice(original);
        }
        Self { weights }
    }
    pub fn act(&self, memory: &mut G5Memory, o: &Observation) -> Action {
        let action = decode(&self.weights, memory.features(o), o);
        memory.remember(o, action);
        action
    }
}

impl G5Memory {
    fn features(&mut self, o: &Observation) -> [f64; G5_FEATURES] {
        let old = league_features(o);
        let charge = f64::from(o.attack_charge);
        let (hp, d) = self.previous.unwrap_or((o.health, o.horizontal_distance));
        self.hurt = if o.health < hp { 1.0 } else { 0.82 * self.hurt };
        let closing = ((d - o.horizontal_distance) * 2.0).clamp(-1.5, 1.5);
        self.approach += 0.25 * (closing - self.approach);
        self.opponent_air += 0.20 * (f64::from(!o.opponent_on_ground) - self.opponent_air);
        let mut x = [0.0; G5_FEATURES];
        x[..LEAGUE_FEATURES].copy_from_slice(&old);
        x[LEAGUE_FEATURES..].copy_from_slice(&[
            self.hurt,
            self.recent_swing,
            self.approach,
            self.opponent_air,
            closing,
            old[1] * charge,
            old[4] * charge,
            old[10] * charge,
            old[1] * self.approach,
            old[6] * charge,
            old[4] * old[9],
            old[5] * old[4],
        ]);
        x
    }

    fn remember(&mut self, o: &Observation, action: Action) {
        self.previous = Some((o.health, o.horizontal_distance));
        self.recent_swing = if action.click {
            1.0
        } else {
            self.recent_swing * 0.72
        };
    }
}

pub const G6_FEATURES: usize = G5_FEATURES + 8;

pub fn g6_trained() -> &'static G6Policy {
    static POLICY: OnceLock<G6Policy> = OnceLock::new();
    POLICY.get_or_init(|| {
        serde_json::from_str(include_str!("../../../models/bundled/league_g6.json"))
            .expect("embedded G6 weights")
    })
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct G6Policy {
    #[serde(
        serialize_with = "serialize_g6_weights",
        deserialize_with = "deserialize_g6_weights"
    )]
    pub weights: [[f64; G6_FEATURES]; OUTPUTS],
}

fn serialize_g6_weights<S: serde::Serializer>(
    weights: &[[f64; G6_FEATURES]; OUTPUTS],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    weights
        .each_ref()
        .map(|row| row.as_slice())
        .serialize(serializer)
}

fn deserialize_g6_weights<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<[[f64; G6_FEATURES]; OUTPUTS], D::Error> {
    let rows = <[Vec<f64>; OUTPUTS]>::deserialize(deserializer)?;
    let mut weights = [[0.0; G6_FEATURES]; OUTPUTS];
    for (target, row) in weights.iter_mut().zip(rows) {
        *target = row.try_into().map_err(|row: Vec<f64>| {
            serde::de::Error::invalid_length(row.len(), &"36 weights per output")
        })?;
    }
    Ok(weights)
}

impl G6Policy {
    pub fn warm_start(base: &G5Policy) -> Self {
        let mut weights = [[0.0; G6_FEATURES]; OUTPUTS];
        for (row, original) in weights.iter_mut().zip(&base.weights) {
            row[..G5_FEATURES].copy_from_slice(original);
        }
        Self { weights }
    }

    pub fn act(&self, memory: &mut G5Memory, o: &Observation) -> Action {
        let mut x = [0.0; G6_FEATURES];
        x[..G5_FEATURES].copy_from_slice(&memory.features(o));
        let charge = f64::from(o.attack_charge);
        let descent = f64::from(o.falling && !o.sprinting);
        let reach = x[9];
        let charged_reach = x[15] * reach;
        x[G5_FEATURES..].copy_from_slice(&[
            f64::from(o.on_ground),
            descent,
            descent * charge,
            descent * reach,
            f64::from(o.rising) * charge,
            f64::from(o.falling && o.sprinting) * charge,
            f64::from(o.target_under_crosshair),
            charged_reach,
        ]);
        let action = decode(&self.weights, x, o);
        memory.remember(o, action);
        action
    }
}
