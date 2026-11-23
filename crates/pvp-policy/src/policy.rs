//! Practice opponents. Each controller receives only its own client observation.
use pvp_sim::{Action, Duel, Keys, Look, Observation, SimError, TICK_NS};

/// Tunable thresholds for Critical+. The original Critical branch stays frozen.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CriticalParams {
    pub jump_distance: f64,
    pub jump_charge: f32,
    pub coast_distance: f64,
    pub coast_velocity: f64,
    pub forward_distance: f64,
    pub backward_distance: f64,
    pub attack_charge: f32,
    pub fallback_ticks: u64,
    pub release_ticks: u64,
}
impl CriticalParams {
    pub const ORIGINAL: Self = Self {
        jump_distance: 7.5,
        jump_charge: 0.35,
        coast_distance: 4.5,
        coast_velocity: 0.25,
        forward_distance: 1.4,
        backward_distance: 1.0,
        attack_charge: 0.93,
        fallback_ticks: 15,
        release_ticks: 1,
    };
    // Selected by the pvp-rl critical_tune example; see docs/research/bots/critical-plus.md.
    pub const PLUS: Self = Self {
        jump_distance: 7.5,
        jump_charge: 0.95,
        coast_distance: 4.0,
        coast_velocity: 0.0,
        forward_distance: 0.0,
        backward_distance: 1.7,
        attack_charge: 0.9,
        fallback_ticks: 12,
        release_ticks: 4,
    };
}
pub fn baseline(o: &Observation) -> Action {
    Action {
        look: Look::TrackTarget,
        keys: Keys {
            forward: o.horizontal_distance > 2.4,
            backward: o.horizontal_distance < 1.5,
            ..Keys::default()
        },
        click: o.attack_charge >= 1.0 && o.eye_to_target < 2.95,
        attack_held: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Baseline,
    Pressure,
    Orbit,
    Counter,
    Critical,
    CriticalPlus,
    DuelRl,
    LeagueRl,
    LeagueG5,
    LeagueG6,
    Reach,
    RecurrentPpo,
    RecurrentG2,
    RecurrentG2Plus,
}

impl Style {
    pub const ALL: [Self; 10] = [
        Self::Baseline,
        Self::Pressure,
        Self::Orbit,
        Self::Counter,
        Self::LeagueG6,
        Self::LeagueG5,
        Self::LeagueRl,
        Self::DuelRl,
        Self::CriticalPlus,
        Self::Critical,
    ];
    pub fn is_recurrent(self) -> bool {
        matches!(
            self,
            Self::RecurrentPpo | Self::RecurrentG2 | Self::RecurrentG2Plus
        )
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Baseline => "Baseline",
            Self::Pressure => "Pressure",
            Self::Orbit => "Orbit",
            Self::Counter => "Counter",
            Self::Critical => "Critical",
            Self::CriticalPlus => "Critical+",
            Self::DuelRl => "Duel-RL",
            Self::LeagueRl => "League-RL",
            Self::LeagueG5 => "League-G5",
            Self::LeagueG6 => "League-G6",
            Self::Reach => "Reach",
            Self::RecurrentPpo => "Recurrent-PPO",
            Self::RecurrentG2 => "recurrent-G2",
            Self::RecurrentG2Plus => "recurrent-g2-plus",
        }
    }
}
impl std::str::FromStr for Style {
    type Err = String;
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        if name.eq_ignore_ascii_case("critical-plus") {
            return Ok(Self::CriticalPlus);
        }
        if name.eq_ignore_ascii_case("recurrent-ppo") {
            return Ok(Self::RecurrentPpo);
        }
        if name.eq_ignore_ascii_case("recurrent-g2") {
            return Ok(Self::RecurrentG2);
        }
        if name.eq_ignore_ascii_case(Self::RecurrentG2Plus.name()) {
            return Ok(Self::RecurrentG2Plus);
        }
        Self::ALL.into_iter().find(|style| style.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Unknown opponent '{name}'. Choose baseline, pressure, orbit, counter, critical, critical-plus, duel-rl, league-rl, league-g5, league-g6, recurrent-ppo, recurrent-g2 or recurrent-g2-plus."))
    }
}

/// One controller per seat per episode. Pausing retains the tactic in progress.
pub struct Bot {
    style: Style,
    last_swing: Option<u64>,
    switch_at: u64,
    strafe_left: bool,
    last_health: f32,
    critical_params: CriticalParams,
    learned_memory: crate::learned::G5Memory,
    recurrent_memory: crate::recurrent::Controller,
}
impl Bot {
    pub fn new(style: Style) -> Self {
        Self {
            style,
            last_swing: None,
            switch_at: 0,
            strafe_left: false,
            last_health: 20.0,
            critical_params: CriticalParams::PLUS,
            learned_memory: Default::default(),
            recurrent_memory: Default::default(),
        }
    }
    pub fn style(&self) -> Style {
        self.style
    }

    pub fn act_in_duel(
        &mut self,
        duel: &Duel,
        seat: usize,
        observation: &Observation,
        ppo_model: Option<&crate::recurrent::Model>,
    ) -> Result<Action, SimError> {
        let model = match self.style {
            Style::RecurrentPpo => ppo_model.unwrap_or_else(|| crate::recurrent::Model::bundled()),
            Style::RecurrentG2 => crate::recurrent::Model::bundled_g2(),
            Style::RecurrentG2Plus => crate::recurrent::Model::bundled_g2_plus(),
            _ => return Ok(self.act(observation)),
        };
        self.recurrent_memory.act(model, duel, seat, observation)
    }

    pub fn with_critical_params(params: CriticalParams) -> Self {
        Self {
            critical_params: params,
            ..Self::new(Style::CriticalPlus)
        }
    }

    pub fn act(&mut self, o: &Observation) -> Action {
        if self.style == Style::Baseline {
            return baseline(o);
        }
        let tick = o.time_ns / TICK_NS;
        let since_swing = self.last_swing.map_or(u64::MAX, |t| tick.saturating_sub(t));
        let d = o.horizontal_distance;
        let in_reach = o.eye_to_target < 2.99;
        let mut keys = Keys::default();
        let mut click = in_reach && o.attack_charge >= 0.99;
        match self.style {
            Style::Baseline => unreachable!(),
            Style::RecurrentPpo | Style::RecurrentG2 | Style::RecurrentG2Plus => {
                unreachable!("recurrent policy requires act_in_duel")
            }
            Style::DuelRl => return crate::learned::trained().act(o),
            Style::LeagueRl => return crate::learned::league_trained().act(o),
            Style::LeagueG5 => {
                return crate::learned::g5_trained().act(&mut self.learned_memory, o);
            }
            Style::LeagueG6 => {
                return crate::learned::g6_trained().act(&mut self.learned_memory, o);
            }
            Style::CriticalPlus => {
                let p = self.critical_params;
                keys.jump = o.on_ground && d < p.jump_distance && o.attack_charge > p.jump_charge;
                let coasting =
                    !o.on_ground && d < p.coast_distance && o.velocity.y < p.coast_velocity;
                keys.forward = d > p.forward_distance
                    && !coasting
                    && !(1..=p.release_ticks).contains(&since_swing);
                keys.backward = d < p.backward_distance;
                let ready = in_reach && o.attack_charge > p.attack_charge;
                click = ready && o.falling && !o.sprinting;
                if click {
                    keys.forward = false;
                    keys.jump = false;
                } else if ready && since_swing >= p.fallback_ticks {
                    keys.forward = true;
                    keys.backward = false;
                    keys.jump = false;
                    click = o.sprinting;
                }
            }
            Style::Reach => {
                // S-tap through two following movement ticks so sprint-hit
                // knockback opens a combo gap before we resume chasing.
                keys.backward = o.eye_to_target < 1.0;
                keys.forward = !keys.backward && o.eye_to_target > 2.0;
                if since_swing <= 2 {
                    keys.forward = false;
                    keys.backward = true;
                }
                click = in_reach && o.attack_charge > 0.9;
                if click && !o.sprinting {
                    keys.forward = true;
                    keys.backward = false;
                    click = false;
                }
                if click {
                    keys.forward = false;
                    keys.backward = true;
                }
            }
            Style::Pressure => {
                // Hold close and W-tap for the next charged knockback. Swing
                // as soon as the attack crosses the strong-hit threshold.
                click = in_reach && o.attack_charge > 0.9;
                keys.forward = since_swing != 1 && d > 0.9;
            }
            Style::Orbit => {
                if d < 4.6 {
                    if tick >= self.switch_at || o.health < self.last_health {
                        self.strafe_left = !self.strafe_left;
                        self.switch_at = tick + if self.strafe_left { 17 } else { 12 };
                    }
                    keys.left = self.strafe_left;
                    keys.right = !self.strafe_left;
                }
                keys.forward = d > 1.9 && since_swing != 1;
                keys.backward = d < 1.25;
            }
            Style::Counter => {
                // Back off only during recovery. Give an advancing target
                // more room and commit to closing again before charge returns.
                let rel = o.relative_position;
                let moving_in = rel.x * o.opponent_displacement.x
                    + rel.z * o.opponent_displacement.z
                    < -0.05 * d;
                let recovery = since_swing <= 7;
                keys.backward = recovery && d < if moving_in { 3.8 } else { 3.35 };
                keys.forward =
                    !keys.backward && since_swing != 1 && d > if recovery { 4.1 } else { 2.55 };
            }
            Style::Critical => {
                // Sprint-jump on approach. Release W before the apex when
                // close enough to coast through sword range on the descent.
                keys.jump = o.on_ground && d < 7.5 && o.attack_charge > 0.35;
                let coasting = !o.on_ground && d < 4.5 && o.velocity.y < 0.25;
                keys.forward = d > 1.4 && !coasting && since_swing != 1;
                keys.backward = d < 1.0;
                // Prefer a falling, non-sprinting critical. After waiting for
                // that opening, re-engage sprint before clicking: attack input
                // is processed BEFORE this tick's movement/sprint update.
                let ready = in_reach && o.attack_charge > 0.93;
                let critical = o.falling && !o.sprinting;
                click = ready && critical;
                if click {
                    keys.forward = false;
                    keys.jump = false;
                } else if ready && since_swing >= 15 {
                    keys.forward = true;
                    keys.backward = false;
                    keys.jump = false;
                    click = o.sprinting;
                }
            }
        }
        if click {
            self.last_swing = Some(tick);
        }
        self.last_health = o.health;
        Action {
            keys,
            click,
            ..Action::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrent::Model;
    use pvp_sim::Config;

    #[test]
    fn custom_recurrent_weights_only_apply_to_ppo() {
        let config = Config {
            expose_opponent_health: true,
            ..Config::default()
        };
        let duel = Duel::new(config, 42).unwrap();
        let observations = duel.observations();
        let actions = |style, model| {
            [0, 1].map(|seat| {
                Bot::new(style)
                    .act_in_duel(&duel, seat, &observations[seat], model)
                    .unwrap()
            })
        };

        let ppo = actions(Style::RecurrentPpo, None);
        let g2 = actions(Style::RecurrentG2, None);
        assert_ne!(ppo, g2);
        assert_eq!(actions(Style::RecurrentPpo, Some(Model::bundled_g2())), g2);
        assert_eq!(actions(Style::RecurrentG2, Some(Model::bundled())), g2);
        let g2_plus = actions("recurrent-g2-plus".parse().unwrap(), None);
        assert_eq!(
            actions(Style::RecurrentPpo, Some(Model::bundled_g2_plus())),
            g2_plus
        );
        assert_eq!(
            actions(Style::RecurrentG2Plus, Some(Model::bundled())),
            g2_plus
        );
    }

    fn charged_in_reach() -> Observation {
        let duel = Duel::new(Config::default(), 42).unwrap();
        let mut o = duel.observe(0).unwrap();
        o.horizontal_distance = 2.5;
        o.eye_to_target = 2.5;
        o.attack_charge = 1.;
        o.sprinting = true;
        o
    }

    #[test]
    fn parameterized_original_matches_frozen_critical_actions() {
        for opponent in [
            Style::Baseline,
            Style::Pressure,
            Style::Orbit,
            Style::Counter,
            Style::Critical,
        ] {
            for first in 0..2 {
                let mut duel = Duel::new(
                    Config {
                        first_client: Some(first),
                        ..Config::default()
                    },
                    42,
                )
                .unwrap();
                let mut frozen = Bot::new(Style::Critical);
                let mut parameterized = Bot::with_critical_params(CriticalParams::ORIGINAL);
                let mut other = Bot::new(opponent);
                while !duel.is_finished() {
                    let o = duel.observations();
                    let action = frozen.act(&o[0]);
                    assert_eq!(
                        action,
                        parameterized.act(&o[0]),
                        "opponent={opponent:?}, tick={}",
                        o[0].time_ns / TICK_NS
                    );
                    duel.step([action, other.act(&o[1])]).unwrap();
                }
            }
        }
    }

    #[test]
    fn critical_plus_wins_original_with_both_seats_and_orders() {
        for seat in 0..2 {
            for first in 0..2 {
                let mut duel = Duel::new(
                    Config {
                        first_client: Some(first),
                        ..Config::default()
                    },
                    42,
                )
                .unwrap();
                let mut bots = std::array::from_fn::<_, 2, _>(|i| {
                    Bot::new(if i == seat {
                        Style::CriticalPlus
                    } else {
                        Style::Critical
                    })
                });
                while !duel.is_finished() {
                    let o = duel.observations();
                    duel.step(std::array::from_fn(|i| bots[i].act(&o[i])))
                        .unwrap();
                }
                assert_eq!(
                    duel.outcome(),
                    pvp_sim::Outcome::Victory { winner: seat },
                    "plus_seat={seat}, first_client={first}"
                );
            }
        }
    }

    #[test]
    fn reach_s_taps_on_attack_and_respects_range_and_charge() {
        let mut bot = Bot::new(Style::Reach);
        let mut o = charged_in_reach();
        o.on_ground = true;
        let hit = bot.act(&o);
        assert!(hit.click);
        assert!(!hit.keys.forward);
        assert!(hit.keys.backward);
        o.time_ns += TICK_NS;
        o.attack_charge = 0.1;
        let recovery = bot.act(&o);
        assert!(!recovery.click);
        assert!(recovery.keys.backward);
        o.eye_to_target = 3.4;
        o.horizontal_distance = 3.7;
        o.time_ns = 3 * TICK_NS;
        let close = bot.act(&o);
        assert!(close.keys.forward);
        assert!(!close.keys.backward);
        o.attack_charge = 1.;
        o.eye_to_target = 3.;
        assert!(!bot.act(&o).click);
        o.eye_to_target = 2.9;
        o.attack_charge = 0.9;
        assert!(!bot.act(&o).click);
        assert!("reach".parse::<Style>().is_err());
        assert!(!Style::ALL.contains(&Style::Reach));
    }

    #[test]
    fn ground_reach_at_ten_hp_beats_baseline_including_first_hit_waiter() {
        for (range, wait) in [(2.0, false), (2.5, true)] {
            for seed in 1..=8 {
                for seat in 0..2 {
                    for first in 0..2 {
                        let mut duel = Duel::new(
                            Config {
                                first_client: Some(first),
                                initial_health: if seat == 0 { [10., 20.] } else { [20., 10.] },
                                ..Config::default()
                            },
                            seed,
                        )
                        .unwrap();
                        let mut bots = std::array::from_fn::<_, 2, _>(|p| {
                            Bot::new(if p == seat {
                                Style::Reach
                            } else {
                                Style::Baseline
                            })
                        });
                        let mut activated = !wait;
                        while !duel.is_finished() {
                            let observations = duel.observations();
                            activated |= observations[1 - seat].health < 20.;
                            let actions = std::array::from_fn(|p| {
                                let mut a = bots[p].act(&observations[p]);
                                if p != seat {
                                    a.click = activated
                                        && observations[p].attack_charge >= 1.
                                        && observations[p].eye_to_target < range;
                                    a.keys.forward =
                                        observations[p].horizontal_distance > range - 0.55;
                                    a.keys.backward =
                                        observations[p].horizontal_distance < range - 1.45;
                                } else {
                                    assert!(!a.keys.jump && !a.keys.left && !a.keys.right);
                                }
                                a
                            });
                            duel.step(actions).unwrap();
                        }
                        assert_eq!(
                            duel.outcome(),
                            pvp_sim::Outcome::Victory { winner: seat },
                            "seed={seed} reach_seat={seat} first_client={first}"
                        );
                        assert!(activated);
                    }
                }
            }
        }
    }

    #[test]
    fn reach_releases_forward_and_s_taps_on_attack() {
        let mut bot = Bot::new(Style::Reach);
        let o = charged_in_reach();
        let action = bot.act(&o);
        assert!(action.click);
        assert!(!action.keys.forward);
        assert!(action.keys.backward);
        let mut recovery = o.clone();
        recovery.time_ns += TICK_NS;
        recovery.attack_charge = 0.1;
        let action = bot.act(&recovery);
        assert!(!action.click);
        assert!(!action.keys.forward);
        assert!(action.keys.backward);
        recovery.time_ns = 2 * TICK_NS;
        assert!(bot.act(&recovery).keys.backward);
        recovery.time_ns = 3 * TICK_NS;
        let chase = bot.act(&recovery);
        assert!(chase.keys.forward);
        assert!(!chase.keys.backward);
    }

    #[test]
    fn critical_bot_keeps_a_falling_non_sprinting_critical() {
        let mut bot = Bot::new(Style::Critical);
        let mut o = charged_in_reach();
        o.on_ground = false;
        o.falling = true;
        o.sprinting = false;
        o.velocity.y = -0.1;
        let action = bot.act(&o);
        assert!(action.click);
        assert!(!action.keys.forward);
        assert!(!action.keys.jump);
    }

    #[test]
    fn critical_bot_prepares_sprint_before_non_critical_click() {
        for (on_ground, vy, distance) in [
            (true, -0.0784, 2.5),
            (false, 0.2, 2.5),
            (true, -0.0784, 0.8),
        ] {
            let mut bot = Bot::new(Style::Critical);
            let mut o = charged_in_reach();
            o.on_ground = on_ground;
            o.velocity.y = vy;
            o.falling = false;
            o.horizontal_distance = distance;
            o.sprinting = false;
            let preparing = bot.act(&o);
            assert!(!preparing.click, "must not swing before sprint is active");
            assert!(preparing.keys.forward);
            assert!(!preparing.keys.backward);
            assert!(!preparing.keys.jump);
            o.time_ns += TICK_NS;
            o.sprinting = true;
            let attack = bot.act(&o);
            assert!(attack.click);
            assert!(attack.keys.forward);
            assert!(!attack.keys.jump);
        }
    }

    #[test]
    fn critical_bot_retains_w_tap_and_waits_for_charge_and_opening() {
        let mut bot = Bot::new(Style::Critical);
        let mut o = charged_in_reach();
        o.sprinting = true;
        assert!(bot.act(&o).click);
        o.time_ns = TICK_NS;
        o.attack_charge = 0.1;
        let recovery = bot.act(&o);
        assert!(!recovery.click);
        assert!(!recovery.keys.forward);
        o.time_ns = 14 * TICK_NS;
        o.attack_charge = 1.;
        assert!(!bot.act(&o).click);
        o.time_ns = 15 * TICK_NS;
        o.sprinting = false;
        assert!(!bot.act(&o).click);
        o.time_ns = 16 * TICK_NS;
        o.sprinting = true;
        assert!(bot.act(&o).click);
        o.time_ns = 40 * TICK_NS;
        o.attack_charge = 0.9;
        assert!(!bot.act(&o).click);
        o.attack_charge = 1.;
        o.eye_to_target = 3.1;
        assert!(!bot.act(&o).click);
    }

    #[test]
    fn critical_fallback_reaches_server_as_a_sprint_hit() {
        use pvp_sim::trace::TraceEvent;
        for first in 0..2 {
            let mut config = Config {
                first_client: Some(first),
                trace_capacity: 1024,
                ..Config::default()
            };
            config.spawns[0].position.z = 0.;
            config.spawns[1].position.z = 2.5;
            let mut duel = Duel::new(config, 42).unwrap();
            let mut bot = Bot::new(Style::Critical);
            let mut accepted = None;
            for _ in 0..20 {
                let action = bot.act(&duel.observe(0).unwrap());
                duel.step([action, Action::default()]).unwrap();
                for record in duel.drain_trace() {
                    if let TraceEvent::Attack {
                        attacker: 0,
                        result,
                    } = record.event
                    {
                        if result.damage.accepted {
                            accepted = Some(result);
                            break;
                        }
                    }
                }
                if accepted.is_some() {
                    break;
                }
            }
            let hit = accepted.expect("charged fallback must reach the target");
            assert!(hit.sprint_hit);
            assert!(!hit.critical);
        }
    }
}
