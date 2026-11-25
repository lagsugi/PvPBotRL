// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

#![deny(unsafe_op_in_unsafe_fn)]

mod exchange;
mod ffi;
pub mod league;
mod reward;

use league::Network;
use pvp_policy::{
    learned::LeaguePolicy,
    policy::{Bot, Style},
    recurrent::{self, Controller, Features, Input, Model},
};
use pvp_sim::{Action, Duel, Observation, Outcome, SimError, TICK_NS};
use serde::Serialize;
use std::sync::{Arc, OnceLock};

pub const NETWORKS: [Network; 13] = [
    Network::Mixed,
    Network::Both0,
    Network::Both2,
    Network::Both20,
    Network::Both60,
    Network::Both120,
    Network::Both150,
    Network::LocalRival,
    Network::LocalActor,
    Network::Adverse,
    Network::Bot20Rival0,
    Network::EarlyBot20Rival0,
    Network::Legacy,
];

pub const RIVALS: [&str; 18] = [
    "Duel-RL",
    "Critical+",
    "Critical",
    "Pressure",
    "Orbit",
    "Counter",
    "Baseline",
    "League-G5",
    "League-RL",
    "League-G1",
    "League-G2",
    "League-G3",
    "G2 challenger",
    "G3 challenger",
    "G4 challenger",
    "Reach",
    "Orbit/Pressure",
    "League-G6",
];
pub const G6: usize = 17;
pub const SNAPSHOT_START: usize = RIVALS.len();
const STYLES: [Style; 9] = [
    Style::DuelRl,
    Style::CriticalPlus,
    Style::Critical,
    Style::Pressure,
    Style::Orbit,
    Style::Counter,
    Style::Baseline,
    Style::LeagueG5,
    Style::LeagueRl,
];

fn archive() -> &'static [LeaguePolicy; 6] {
    static ARCHIVE: OnceLock<[LeaguePolicy; 6]> = OnceLock::new();
    ARCHIVE.get_or_init(|| {
        let [g1, g2] =
            serde_json::from_str(include_str!("../../../models/archive/league_history.json"))
                .expect("archived G1 and G2");
        let parse = |s| serde_json::from_str(s).expect("archived policy");
        [
            g1,
            g2,
            parse(include_str!("../../../models/archive/league_g3.json")),
            parse(include_str!(
                "../../../models/archive/league_challenger.json"
            )),
            parse(include_str!(
                "../../../models/archive/league_g3_challenger.json"
            )),
            parse(include_str!(
                "../../../models/archive/league_g4_challenger.json"
            )),
        ]
    })
}

enum Rival {
    Script(Bot),
    Archived(&'static LeaguePolicy),
    Switch { orbit: Bot, pressure: Bot },
    Recurrent(Arc<Model>, Controller),
}

impl Rival {
    fn new(id: usize, snapshots: &[Option<Arc<Model>>]) -> Result<Self, SimError> {
        Ok(match id {
            0..=8 => Self::Script(Bot::new(STYLES[id])),
            9..=14 => Self::Archived(&archive()[id - 9]),
            15 => Self::Script(Bot::new(Style::Reach)),
            16 => Self::Switch {
                orbit: Bot::new(Style::Orbit),
                pressure: Bot::new(Style::Pressure),
            },
            G6 => Self::Script(Bot::new(Style::LeagueG6)),
            _ => {
                let model = snapshots
                    .get(id - SNAPSHOT_START)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| SimError::InvalidBatch(format!("unknown opponent slot {id}")))?;
                Self::Recurrent(Arc::clone(model), Controller::default())
            }
        })
    }

    fn act(&mut self, duel: &Duel, seat: usize, obs: &Observation) -> Result<Action, SimError> {
        Ok(match self {
            Self::Script(bot) => bot.act(obs),
            Self::Archived(policy) => policy.act(obs),
            Self::Switch { orbit, pressure } => {
                if pressure_phase(obs) {
                    pressure.act(obs)
                } else {
                    orbit.act(obs)
                }
            }
            Self::Recurrent(model, controller) => controller.act(model, duel, seat, obs)?,
        })
    }
}

fn pressure_phase(obs: &Observation) -> bool {
    (obs.time_ns / (40 * TICK_NS)) % 2 == 1 || obs.health < 6.
}

#[derive(Default, Serialize)]
pub struct SwitchResponse {
    pub switches: u32,
    pub decisions: u32,
    pub net_damage: f32,
}

pub struct Episode {
    pub duel: Duel,
    pub seat: usize,
    obs: [Observation; 2],
    rival: Rival,
    previous: Action,
    exchanges: exchange::Exchanges,
}

impl Episode {
    /// Fixed physical seat, with network parameters tied to seats, not policies.
    pub fn seated(
        seed: u64,
        network: usize,
        opponent: usize,
        max_steps: u64,
        snapshots: &[Option<Arc<Model>>],
        seat: usize,
    ) -> Result<Self, SimError> {
        Self::seated_with_equipment(seed, network, opponent, max_steps, snapshots, seat, false)
    }

    pub fn seated_with_equipment(
        seed: u64,
        network: usize,
        opponent: usize,
        max_steps: u64,
        snapshots: &[Option<Arc<Model>>],
        seat: usize,
        fixed_equipment: bool,
    ) -> Result<Self, SimError> {
        if seat > 1 {
            return Err(SimError::InvalidBatch("invalid seat".into()));
        }
        let mut episode = Self::with_equipment(
            seed,
            network,
            opponent,
            max_steps,
            snapshots,
            fixed_equipment,
        )?;
        episode.seat = seat;
        Ok(episode)
    }

    pub fn new(
        seed: u64,
        network: usize,
        opponent: usize,
        max_steps: u64,
        snapshots: &[Option<Arc<Model>>],
    ) -> Result<Self, SimError> {
        Self::with_equipment(seed, network, opponent, max_steps, snapshots, false)
    }

    pub fn with_equipment(
        seed: u64,
        network: usize,
        opponent: usize,
        max_steps: u64,
        snapshots: &[Option<Arc<Model>>],
        fixed_equipment: bool,
    ) -> Result<Self, SimError> {
        let network = *NETWORKS
            .get(network)
            .ok_or_else(|| SimError::InvalidBatch("invalid network".into()))?;
        let trial = league::scenario(seed, network);
        let mut config = trial.config;
        config.expose_opponent_health = true;
        config.server_rules = pvp_sim::config::ServerRules::PaperFlatV2;
        config.max_steps = max_steps;
        if fixed_equipment {
            config = config.with_fixed_equipment();
        }
        let duel = Duel::new(config, trial.seed)?;
        let obs = duel.observations();
        Ok(Self {
            duel,
            seat: trial.seat,
            obs,
            rival: Rival::new(opponent, snapshots)?,
            previous: Action::default(),
            exchanges: Default::default(),
        })
    }

    fn features(&self, input: Input) -> Result<Features, SimError> {
        recurrent::observe(
            &self.duel,
            self.seat,
            &self.obs[self.seat],
            self.previous,
            input,
        )
    }

    fn advance(&mut self, action: Action) -> Result<[Action; 2], SimError> {
        let mut actions = [action; 2];
        let rival = 1 - self.seat;
        actions[rival] = self.rival.act(&self.duel, rival, &self.obs[rival])?;
        self.obs = self.duel.step(actions)?.observations;
        self.previous = action;
        Ok(actions)
    }

    fn record_exchange(&mut self, before: [f32; 2], after: [f32; 2], action: Action) -> f32 {
        let loss = |s: usize| (before[s] - after[s]).max(0.);
        self.exchanges.record(
            loss(1 - self.seat),
            loss(self.seat),
            action,
            self.duel.is_finished(),
        )
    }

    pub fn run(
        &mut self,
        model: Option<&Model>,
        mut record: impl FnMut([Action; 2]),
    ) -> Result<MatchResult, SimError> {
        let mut controller = Controller::default();
        let mut g6 = Bot::new(Style::LeagueG6);
        let switching = matches!(self.rival, Rival::Switch { .. });
        let mut last_phase = pressure_phase(&self.obs[1 - self.seat]);
        let mut response = SwitchResponse::default();
        let mut remaining = 0;
        while !self.duel.is_finished() {
            if switching {
                let phase = pressure_phase(&self.obs[1 - self.seat]);
                if phase != last_phase {
                    response.switches += 1;
                    remaining = 12;
                    last_phase = phase;
                }
            }
            let o = &self.obs[self.seat];
            let action = if let Some(model) = model {
                controller.act(model, &self.duel, self.seat, o)?
            } else {
                g6.act(o)
            };
            let before = self.duel.server_health();
            record(self.advance(action)?);
            let after = self.duel.server_health();
            self.record_exchange(before, after, action);
            if remaining > 0 {
                remaining -= 1;
                response.decisions += 1;
                let net = |hp: [f32; 2]| hp[self.seat] - hp[1 - self.seat];
                response.net_damage += net(after) - net(before);
            }
        }
        let mut result = self.result();
        result.switch_response = switching.then_some(response);
        Ok(result)
    }

    fn result(&self) -> MatchResult {
        let health = self.duel.server_health();
        MatchResult {
            outcome: match self.duel.outcome() {
                Outcome::Ongoing => 0,
                Outcome::Victory { winner } if winner == self.seat => 1,
                Outcome::Victory { .. } => 2,
                Outcome::Truncated => 3,
            },
            hp: health[self.seat],
            rival_hp: health[1 - self.seat],
            steps: self.duel.decision_step(),
            exchanges: self.exchanges.totals,
            switch_response: None,
        }
    }
}

#[derive(Serialize)]
pub struct MatchResult {
    pub outcome: u8,
    pub hp: f32,
    pub rival_hp: f32,
    pub steps: u64,
    pub exchanges: exchange::ExchangeTotals,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_response: Option<SwitchResponse>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pvp_sim::{
        action::Look,
        config::ClientTiming,
        math::Vec3,
        network::{Downstream, Envelope},
        trace::TraceEvent,
    };

    #[test]
    fn swapped_seats_keep_physical_network_and_scenario_fixed() {
        for network in 0..NETWORKS.len() {
            let left = Episode::seated(731, network, G6, 6000, &[], 0).unwrap();
            let right = Episode::seated(731, network, G6, 6000, &[], 1).unwrap();
            assert_eq!(left.seat, 0);
            assert_eq!(right.seat, 1);
            assert_eq!(left.duel.config(), right.duel.config());
            assert_eq!(
                left.duel.config().server_rules,
                pvp_sim::config::ServerRules::PaperFlatV2
            );
            assert_eq!(left.duel.snapshot().seed, right.duel.snapshot().seed);
            assert_eq!(left.duel.server_health(), right.duel.server_health());
        }
        assert!(Episode::seated(731, 0, G6, 6000, &[], 2).is_err());
    }

    #[test]
    fn training_duels_send_downward_motion_to_a_falling_victim() {
        for striker in 0..2 {
            let victim = 1 - striker;
            let episode =
                Episode::seated(0, Network::Both20 as usize, G6, 40, &[], striker).unwrap();
            let mut config = episode.duel.config().clone();
            config.spawns[0].position = Vec3::new(0.0, 0.0, -1.25);
            config.spawns[1].position = Vec3::new(0.0, 0.0, 1.25);
            config.clients = [ClientTiming::default(), ClientTiming::default()];
            config.ping_profile = None;
            config.server_phase_ns = 0;
            config.first_client = Some(striker);
            config.trace_capacity = 2048;
            let mut duel = Duel::new(config, 0).unwrap();
            for tick in 0..30 {
                let mut actions: [Action; 2] = [Action {
                    look: Look::Keep,
                    ..Action::default()
                }; 2];
                actions[victim].keys.jump = tick == 10;
                actions[striker].click = tick == 18;
                duel.step(actions).unwrap();
            }

            let mut impulses =
                duel.drain_trace()
                    .into_iter()
                    .filter_map(|record| match record.event {
                        TraceEvent::SendDown {
                            player,
                            envelope:
                                Envelope {
                                    packet: Downstream::Motion { entity, velocity },
                                    ..
                                },
                        } if player == victim && entity == victim => Some(velocity),
                        _ => None,
                    });
            let velocity = impulses.next().expect("expected victim motion packet");
            assert!(impulses.next().is_none());
            assert!(
                velocity.y < 0.0,
                "seat {striker} sent upward motion: {velocity:?}"
            );
        }
    }
}
