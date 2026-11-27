// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

use pvp_sim::{
    Action, Config, Duel, Keys, Observation, ObservationSchema, SimError, math::Vec3,
    state::LocalPlayer,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const WIDTH: usize = 64;
pub const OUTPUTS: usize = 10;
pub const MAX_INPUTS: usize = 48;
pub const HEADS: [usize; 4] = [3, 3, 2, 2];
pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    #[default]
    Full,
    Legacy,
    NoOpponentHp,
    NoAction,
    G3,
}

impl Input {
    pub fn size(self) -> usize {
        match self {
            Self::Full => 44,
            Self::Legacy | Self::G3 => 48,
            Self::NoOpponentHp => 42,
            Self::NoAction => 40,
        }
    }
}

pub struct Features {
    pub values: [f32; MAX_INPUTS],
    pub allow_jump: bool,
    pub allow_click: bool,
}

impl Features {
    pub fn action(&self, choices: [u8; 4]) -> Action {
        decode_action(choices, self.allow_jump, self.allow_click)
    }

    pub fn mask(&self) -> [u8; 2] {
        [u8::from(self.allow_jump), u8::from(self.allow_click)]
    }
}

pub fn decode_action(choices: [u8; 4], allow_jump: bool, allow_click: bool) -> Action {
    Action {
        keys: Keys {
            backward: choices[0] == 0,
            forward: choices[0] == 2,
            right: choices[1] == 0,
            left: choices[1] == 2,
            jump: allow_jump && choices[2] == 1,
            ..Keys::default()
        },
        click: allow_click && choices[3] == 1,
        ..Action::default()
    }
}

/// Encodes the supplied observation with local client state and match timing.
pub fn observe(
    duel: &Duel,
    seat: usize,
    observation: &Observation,
    previous: Action,
    input: Input,
) -> Result<Features, SimError> {
    encode(
        observation,
        duel.local_player(seat)?,
        duel.config(),
        duel.decision_step(),
        previous,
        input,
    )
}

fn encode(
    o: &Observation,
    local: &LocalPlayer,
    config: &Config,
    step: u64,
    previous: Action,
    input: Input,
) -> Result<Features, SimError> {
    encode_sample(
        o,
        Context {
            jump_delay: local.jump_delay,
            miss_time: local.miss_time,
            fall_distance: local.body.fall_distance,
            height_above_floor: local.body.position.y - config.floor_y,
            remaining_fraction: config.max_steps.saturating_sub(step) as f32
                / config.max_steps as f32,
        },
        previous,
        input,
    )
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Context {
    pub jump_delay: u8,
    pub miss_time: u8,
    pub fall_distance: f64,
    pub height_above_floor: f64,
    pub remaining_fraction: f32,
}

/// Encode an observation and the per-decision context recorded by another adapter.
pub fn encode_sample(
    o: &Observation,
    context: Context,
    previous: Action,
    input: Input,
) -> Result<Features, SimError> {
    let hp = if input == Input::NoOpponentHp {
        None
    } else {
        Some(
            o.opponent_health
                .zip(o.opponent_health_age_ns)
                .ok_or_else(|| {
                    SimError::InvalidConfig(
                        "recurrent policy requires delivered opponent HP and age".into(),
                    )
                })?,
        )
    };
    let mut values = [0.; MAX_INPUTS];
    let mut i = 0;
    let mut push = |value| {
        values[i] = value;
        i += 1;
    };
    let age = |ns: u64| ((ns as f64 / 1e9).min(10.).ln_1p() / 11_f64.ln()) as f32;
    if input == Input::Legacy {
        use std::sync::OnceLock;
        static SCHEMA: OnceLock<ObservationSchema> = OnceLock::new();
        for x in SCHEMA.get_or_init(ObservationSchema::default).project(o)? {
            push(x);
        }
        let (health, received) = hp.unwrap();
        push(health / 20.);
        push(age(received));
    } else {
        let forward = pvp_sim::math::look_vector(o.yaw, 0.);
        let project = |v: Vec3| {
            [
                (v.x * forward.x + v.z * forward.z) as f32,
                (v.x * forward.z - v.z * forward.x) as f32,
                v.y as f32,
            ]
        };
        push(o.health / 20.);
        if let Some((health, _)) = hp {
            push(health / 20.);
        }
        push(o.attack_charge);
        for v in project(o.relative_position) {
            push(v / 8.);
        }
        for v in [
            o.horizontal_distance as f32 / 8.,
            o.eye_to_target as f32 / 8.,
            f32::from(o.target_under_crosshair),
            o.pitch / 90.,
        ] {
            push(v);
        }
        for vector in [o.velocity, o.opponent_velocity, o.opponent_displacement] {
            for v in project(vector) {
                push(v);
            }
        }
        for flag in [
            o.sprinting,
            o.sneaking,
            o.on_ground,
            o.opponent_sprinting,
            o.opponent_sneaking,
            o.opponent_on_ground,
        ] {
            push(f32::from(flag));
        }
        for ns in [
            o.position_age_ns,
            o.metadata_age_ns,
            o.velocity_age_ns,
            o.health_age_ns,
        ] {
            push(age(ns));
        }
        if let Some((_, received)) = hp {
            push(age(received));
        }
        for flag in [o.sword_present].into_iter().chain(o.armor_present) {
            push(f32::from(flag));
        }
        push(f32::from(context.jump_delay) / 10.);
        push(f32::from(context.miss_time) / 10.);
        push((context.fall_distance.clamp(0., 64.).ln_1p() / 65_f64.ln()) as f32);
        push((context.height_above_floor / 8.) as f32);
        push(context.remaining_fraction);
    }
    if input != Input::NoAction {
        push(f32::from(previous.keys.forward) - f32::from(previous.keys.backward));
        push(f32::from(previous.keys.left) - f32::from(previous.keys.right));
        push(f32::from(previous.keys.jump));
        push(f32::from(previous.click));
    }
    if input == Input::G3 {
        // Signed rates along/across the observed line of separation (blocks/tick).
        let distance = o.horizontal_distance.max(1e-6);
        let dx = o.relative_position.x / distance;
        let dz = o.relative_position.z / distance;
        let vx = o.velocity.x - o.opponent_velocity.x;
        let vz = o.velocity.z - o.opponent_velocity.z;
        let closing = vx * dx + vz * dz;
        let margin = 3.0 - o.eye_to_target;
        push(margin as f32);
        push(closing as f32);
        push((vx * dz - vz * dx) as f32);
        push((margin + 3.0 * closing) as f32);
    }
    debug_assert_eq!(i, input.size());
    for value in &mut values[..i] {
        if !value.is_finite() {
            return Err(SimError::InvalidConfig("non-finite recurrent input".into()));
        }
        if input != Input::Legacy {
            *value = value.clamp(-8., 8.);
        }
    }
    Ok(Features {
        values,
        allow_jump: o.on_ground,
        allow_click: o.eye_to_target < 3.,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Linear {
    pub weight: Vec<Vec<f32>>,
    pub bias: Vec<f32>,
}

impl Linear {
    fn valid(&self, rows: usize, cols: usize) -> bool {
        self.weight.len() == rows
            && self.bias.len() == rows
            && self.bias.iter().all(|v| v.is_finite())
            && self
                .weight
                .iter()
                .all(|r| r.len() == cols && r.iter().all(|v| v.is_finite()))
    }

    fn apply(&self, x: &[f32], out: &mut [f32]) {
        for (dst, (row, bias)) in out.iter_mut().zip(self.weight.iter().zip(&self.bias)) {
            *dst = *bias + row.iter().zip(x).map(|(w, x)| w * x).sum::<f32>();
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub version: u32,
    pub input: Input,
    pub encoder: Linear,
    pub input_gate: Linear,
    pub hidden_gate: Option<Linear>,
    pub actor: Linear,
    pub mean: Vec<f32>,
    pub scale: Vec<f32>,
}

impl Model {
    pub(crate) fn bundled() -> &'static Self {
        use std::sync::OnceLock;

        static MODEL: OnceLock<Model> = OnceLock::new();
        MODEL.get_or_init(|| {
            let model: Self =
                serde_json::from_str(include_str!("../../../models/bundled/recurrent_ppo.json"))
                    .expect("valid bundled Recurrent-PPO JSON");
            model
                .validate()
                .expect("valid bundled Recurrent-PPO weights");
            model
        })
    }

    pub(crate) fn bundled_g2() -> &'static Self {
        static MODEL: std::sync::OnceLock<Model> = std::sync::OnceLock::new();
        MODEL.get_or_init(|| {
            let model: Self =
                serde_json::from_str(include_str!("../../../models/bundled/recurrent_g2.json"))
                    .expect("valid recurrent-G2 JSON");
            model.validate().expect("valid recurrent-G2 weights");
            model
        })
    }

    pub(crate) fn bundled_g2_plus() -> &'static Self {
        static MODEL: std::sync::OnceLock<Model> = std::sync::OnceLock::new();
        MODEL.get_or_init(|| {
            let model: Self = serde_json::from_str(include_str!(
                "../../../models/bundled/recurrent_g2_plus.json"
            ))
            .expect("valid recurrent-g2-plus JSON");
            model.validate().expect("valid recurrent-g2-plus weights");
            model
        })
    }

    pub(crate) fn bundled_g3_prototype() -> &'static Self {
        static MODEL: std::sync::OnceLock<Model> = std::sync::OnceLock::new();
        MODEL.get_or_init(|| {
            let model: Self =
                serde_json::from_str(include_str!("../../../models/bundled/g3_prototype.json"))
                    .expect("valid g3-prototype JSON");
            model.validate().expect("valid g3-prototype weights");
            model
        })
    }

    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let model: Self =
            serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
        model.validate()?;
        Ok(model)
    }

    pub fn validate(&self) -> Result<(), SimError> {
        let n = self.input.size();
        let rows = if self.hidden_gate.is_some() {
            3 * WIDTH
        } else {
            WIDTH
        };
        if self.version != VERSION
            || !self.encoder.valid(WIDTH, n)
            || !self.input_gate.valid(rows, WIDTH)
            || self
                .hidden_gate
                .as_ref()
                .is_some_and(|g| !g.valid(rows, WIDTH))
            || !self.actor.valid(OUTPUTS, WIDTH)
            || self.mean.len() != n
            || self.scale.len() != n
            || !self.mean.iter().all(|v| v.is_finite())
            || !self.scale.iter().all(|v| v.is_finite() && *v > 0.)
        {
            return Err(SimError::InvalidConfig(
                "invalid recurrent model version, shape or coefficients".into(),
            ));
        }
        Ok(())
    }

    /// Gate order and candidate update follow torch.nn.GRU.
    pub fn logits(&self, features: &Features, memory: &mut [f32; WIDTH]) -> [f32; OUTPUTS] {
        let mut x = [0.; MAX_INPUTS];
        let n = self.input.size();
        for (i, value) in x[..n].iter_mut().enumerate() {
            *value = ((features.values[i] - self.mean[i]) * self.scale[i]).clamp(-8., 8.);
        }
        let mut encoded = [0.; WIDTH];
        self.encoder.apply(&x[..n], &mut encoded);
        encoded.iter_mut().for_each(|x| *x = x.tanh());
        let mut i = [0.; 3 * WIDTH];
        self.input_gate.apply(&encoded, &mut i);
        if let Some(gate) = &self.hidden_gate {
            let sigmoid = |x: f32| 1. / (1. + (-x).exp());
            let mut h = [0.; 3 * WIDTH];
            gate.apply(memory, &mut h);
            for (j, old) in memory.iter_mut().enumerate() {
                let reset = sigmoid(i[j] + h[j]);
                let update = sigmoid(i[WIDTH + j] + h[WIDTH + j]);
                let candidate = (i[2 * WIDTH + j] + reset * h[2 * WIDTH + j]).tanh();
                *old = (1. - update) * candidate + update * *old;
            }
        } else {
            for (old, x) in memory.iter_mut().zip(i) {
                *old = x.tanh();
            }
        }
        let mut output = [0.; OUTPUTS];
        self.actor.apply(memory, &mut output);
        output
    }
}

#[derive(Default)]
pub struct Controller {
    hidden: Option<Box<[f32; WIDTH]>>,
    previous: Action,
}

impl Controller {
    pub fn act(
        &mut self,
        model: &Model,
        duel: &Duel,
        seat: usize,
        observation: &Observation,
    ) -> Result<Action, SimError> {
        let features = observe(duel, seat, observation, self.previous, model.input)?;
        let hidden = self.hidden.get_or_insert_with(|| Box::new([0.; WIDTH]));
        let logits = model.logits(&features, hidden);
        self.previous = features.action(greedy_choices(&logits));
        Ok(self.previous)
    }
}

pub fn greedy_choices(logits: &[f32; OUTPUTS]) -> [u8; 4] {
    let mut offset = 0;
    HEADS.map(|width| {
        let values = &logits[offset..offset + width];
        offset += width;
        let mut best = 0;
        for j in 1..width {
            if values[j] > values[best] {
                best = j;
            }
        }
        best as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Bot, Style};

    #[test]
    fn recurrent_ppo_matches_shared_policy_fixture() {
        #[derive(Deserialize)]
        struct Decision {
            features: Vec<f32>,
            logits: [f32; OUTPUTS],
            action: Action,
        }

        let fixture = include_str!("../../../tests/fixtures/policy/recurrent-ppo-rust.ndjson");
        let mut config = Config {
            expose_opponent_health: true,
            ..Config::default()
        };
        config.spawns[0].position = Vec3::new(-0.35, 0., -1.7);
        config.spawns[1].position = Vec3::new(0.35, 0., 1.7);
        config.spawns[1].yaw = -168.0;
        let mut duel = Duel::new(config, 42).unwrap();
        let model = Model::bundled();
        let mut controller = Controller::default();
        let mut g6 = Bot::new(Style::LeagueG6);
        let mut previous = Action::default();
        let mut hidden = [0.; WIDTH];

        for (frame, line) in fixture.lines().enumerate() {
            let expected: Decision = serde_json::from_str(line).unwrap();
            let [opponent, observation] = duel.observations();
            let features = observe(&duel, 1, &observation, previous, Input::Full).unwrap();
            assert_eq!(expected.features.len(), Input::Full.size());
            for (index, (actual, expected)) in
                features.values.iter().zip(expected.features).enumerate()
            {
                assert!(
                    (actual - expected).abs() <= 1e-6,
                    "frame {frame}, feature {index}: {actual} != {expected}"
                );
            }
            for (index, (actual, expected)) in model
                .logits(&features, &mut hidden)
                .into_iter()
                .zip(expected.logits)
                .enumerate()
            {
                assert!(
                    (actual - expected).abs() <= 1e-4,
                    "frame {frame}, logit {index}: {actual} != {expected}"
                );
            }
            let action = controller.act(model, &duel, 1, &observation).unwrap();
            assert_eq!(action, expected.action, "frame {frame}");
            duel.step([g6.act(&opponent), action]).unwrap();
            previous = action;
        }
    }

    #[test]
    fn encoded_features_include_delivered_hp_and_previous_action() {
        let config = Config {
            initial_health: [12., 6.],
            expose_opponent_health: true,
            ..Config::default()
        };
        let duel = Duel::new(config, 1).unwrap();
        let observation = duel.observe(0).unwrap();
        let previous = Action {
            keys: Keys {
                backward: true,
                left: true,
                ..Keys::default()
            },
            click: true,
            ..Action::default()
        };
        let features = observe(&duel, 0, &observation, previous, Input::Full).unwrap();
        assert_eq!(features.values[..3], [0.6, 0.3, 1.]);
        assert_eq!(features.values[40..44], [-1., 1., 0., 1.]);
        for input in [Input::Legacy, Input::NoOpponentHp, Input::NoAction] {
            observe(&duel, 0, &observation, previous, input).unwrap();
        }
    }

    #[test]
    fn hp_features_use_delivered_observation_not_server_health() {
        let duel = Duel::new(
            Config {
                expose_opponent_health: true,
                ..Config::default()
            },
            7,
        )
        .unwrap();
        let mut observation = duel.observe(0).unwrap();
        observation.opponent_health = Some(3.);
        observation.opponent_health_age_ns = Some(150_000_000);
        let features = observe(&duel, 0, &observation, Action::default(), Input::Full).unwrap();
        assert_eq!(features.values[1], 3. / 20.);
        // Full input index 29 is delivered opponent-health age, after the four local ages.
        let expected = (0.15_f64.ln_1p() / 11_f64.ln()) as f32;
        assert!((features.values[29] - expected).abs() < 1e-6);
        observation.opponent_health = None;
        assert!(observe(&duel, 0, &observation, Action::default(), Input::Full).is_err());
        assert!(
            observe(
                &duel,
                0,
                &observation,
                Action::default(),
                Input::NoOpponentHp
            )
            .is_ok()
        );
    }
}
