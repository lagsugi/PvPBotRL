// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

//! League training with frozen generations, online self-play and the simulator's RTT population.
#[path = "common/ars.rs"]
mod ars;
#[path = "league_rl/g5.rs"]
mod g5;
#[path = "league_rl/g6.rs"]
mod g6;
#[path = "common/statistics.rs"]
mod stats;

use ars::Random;
use pvp_policy::{
    learned::LeaguePolicy,
    policy::{Bot, Style},
};
use pvp_rl::league::{Trial, trial};
use pvp_sim::{Action, Duel, Observation, Outcome, TICK_NS, ping::PingProfile, rng::derive_seed};
use rayon::prelude::*;
use serde::Serialize;
use std::{error::Error, fs, path::Path};

const OUTPUT: &str = "build/benchmarks/league-rl";
const TRAIN: u64 = 0x4c01;
const VALID: u64 = 0x4c02;
const TEST: u64 = 0x4c31;
const STRESS: u64 = 0x4c32;
const STYLES: [Style; 7] = [
    Style::DuelRl,
    Style::CriticalPlus,
    Style::Critical,
    Style::Pressure,
    Style::Orbit,
    Style::Counter,
    Style::Baseline,
];
const BATCH: usize = 16;
const DIRECTIONS: usize = 24;
const GENERATIONS: [usize; 3] = [24, 48, 120];

#[derive(Clone, Copy)]
enum Rival<'a> {
    Style(Style),
    Frozen(&'static str, &'a LeaguePolicy),
    G5(&'a pvp_policy::learned::G5Policy),
    G6(&'a pvp_policy::learned::G6Policy),
    Switch,
}
impl Rival<'_> {
    fn name(&self) -> &'static str {
        match self {
            Self::Style(s) => s.name(),
            Self::Frozen(name, _) => name,
            Self::G5(_) => "G5 self-play",
            Self::G6(_) => "G6 self-play",
            Self::Switch => "Orbit/Pressure",
        }
    }
}
fn roster(archive: &[LeaguePolicy]) -> Vec<Rival<'_>> {
    STYLES
        .into_iter()
        .map(Rival::Style)
        .chain(
            archive
                .iter()
                .zip(["League-G1", "League-G2"])
                .map(|(p, name)| Rival::Frozen(name, p)),
        )
        .collect()
}

#[derive(Serialize)]
struct Game {
    seed: u64,
    seat: usize,
    // Role order is learner, rival. RTTs are read after the simulator samples the profile.
    rtt_ms: [f64; 2],
    outcome: Outcome,
    episode_return: f64,
}
impl Game {
    fn won(&self) -> bool {
        self.outcome == (Outcome::Victory { winner: self.seat })
    }
}
fn play(actor: impl FnMut(&Observation) -> Action, rival: Rival, t: &Trial, shaping: bool) -> Game {
    play_match(actor, rival, t, shaping, None).0
}

#[derive(Clone, Copy, Default, Serialize)]
struct Attacks {
    critical: usize,
    damaging: usize,
    rising_clicks: usize,
}

fn play_match(
    mut actor: impl FnMut(&Observation) -> Action,
    rival: Rival,
    t: &Trial,
    shaping: bool,
    critical_bonus: Option<f64>,
) -> (Game, Attacks) {
    use pvp_sim::sim::CombatSound;
    let mut config = t.config.clone();
    if shaping {
        config.reward.damage = 0.12;
        config.reward.time_penalty = 0.0001;
    }
    let mut env = Duel::new(config, t.seed).unwrap();
    let rtt_ms = [t.seat, 1 - t.seat].map(|s| {
        let c = &env.config().clients[s];
        (c.upstream_ns + c.downstream_ns) as f64 / 1e6
    });
    let mut bot = Bot::new(match rival {
        Rival::Style(s) => s,
        _ => Style::Orbit,
    });
    let mut pressure = Bot::new(Style::Pressure);
    let mut g5_memory = Default::default();
    let mut obs = env.observations();
    let mut episode_return = 0.;
    let mut attacks = Attacks::default();
    loop {
        let other = &obs[1 - t.seat];
        let response = match rival {
            Rival::Frozen(_, p) => p.act(other),
            Rival::G5(p) => p.act(&mut g5_memory, other),
            Rival::G6(p) => p.act(&mut g5_memory, other),
            // A held-out opponent changes style during a match, including when hurt.
            Rival::Switch if (other.time_ns / (40 * TICK_NS)) % 2 == 1 || other.health < 6. => {
                pressure.act(other)
            }
            _ => bot.act(other),
        };
        let mut actions = [Action::default(); 2];
        actions[t.seat] = actor(&obs[t.seat]);
        actions[1 - t.seat] = response;
        let step = if let Some(bonus) = critical_bonus {
            attacks.rising_clicks += usize::from(actions[t.seat].click && obs[t.seat].rising);
            env.step_with_sounds(actions, |e| {
                if e.source == t.seat {
                    match e.sound {
                        CombatSound::Critical => {
                            attacks.critical += 1;
                            attacks.damaging += 1;
                            if attacks.critical <= 6 {
                                episode_return += bonus;
                            }
                        }
                        CombatSound::Strong | CombatSound::Weak | CombatSound::Sweep => {
                            attacks.damaging += 1;
                        }
                        _ => {}
                    }
                }
            })
        } else {
            env.step(actions)
        }
        .unwrap();
        episode_return += f64::from(step.rewards[t.seat]);
        if step.terminated || step.truncated {
            return (
                Game {
                    seed: t.seed,
                    seat: t.seat,
                    rtt_ms,
                    outcome: step.outcome,
                    episode_return,
                },
                attacks,
            );
        }
        obs = step.observations;
    }
}

fn rounds(split: u64, n: usize, stress: bool) -> Vec<Trial> {
    (0..n)
        .map(|i| trial(derive_seed(split, i as u64), stress))
        .collect()
}
fn rates(policy: &LeaguePolicy, pool: &[Rival], cases: &[Trial]) -> Vec<f64> {
    pool.par_iter()
        .map(|&rival| {
            let wins = cases
                .par_iter()
                .filter(|t| play(|o| policy.act(o), rival, t, false).won())
                .count();
            wins as f64 / cases.len() as f64
        })
        .collect()
}
fn rank(rates: &[f64]) -> f64 {
    let mut sorted = rates.to_vec();
    sorted.sort_by(f64::total_cmp);
    0.50 * sorted[0]
        + 0.30 * sorted[..3].iter().sum::<f64>() / 3.0
        + 0.20 * sorted.iter().sum::<f64>() / sorted.len() as f64
}
fn sample_hard(rng: &mut Random, rates: &[f64]) -> usize {
    let weight = |r: f64| 0.03 + (1.0 - r).powi(2);
    let mut target = rng.unit() * rates.iter().copied().map(weight).sum::<f64>();
    for (i, &r) in rates.iter().enumerate() {
        target -= weight(r);
        if target <= 0.0 {
            return i;
        }
    }
    rates.len() - 1
}
fn save(file: &str, data: &impl Serialize) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(OUTPUT)?;
    fs::write(
        Path::new(OUTPUT).join(file),
        serde_json::to_string_pretty(data)? + "\n",
    )?;
    Ok(())
}
// Rejection sampling keeps the configured Gamma population and asymmetry within the adverse slice.
fn slower_trial(rng: &mut Random) -> Trial {
    loop {
        let t = trial(rng.next_u64(), false);
        let mut clients = t.config.clients.clone();
        t.config
            .ping_profile
            .as_ref()
            .unwrap()
            .apply(&mut clients, t.seed);
        let rtt = clients.map(|c| c.upstream_ns + c.downstream_ns);
        if rtt[t.seat] >= 90_000_000 && rtt[1 - t.seat] <= 45_000_000 {
            return t;
        }
    }
}

fn train_stage(
    mut policy: LeaguePolicy,
    pool: &[Rival],
    rng: &mut Random,
    generation: usize,
    iterations: usize,
    robust: bool,
) -> (LeaguePolicy, serde_json::Value) {
    let normal = rounds(
        derive_seed(VALID, generation as u64),
        if robust { 144 } else { 96 },
        false,
    );
    let mut dev_rng = Random::new(0x4c18);
    let slow: Vec<_> = (0..if robust { 64 } else { 0 })
        .map(|_| slower_trial(&mut dev_rng))
        .collect();
    let evaluate = |p: &LeaguePolicy| {
        let mut r = rates(p, pool, &normal);
        let mut score = rank(&r);
        if robust {
            let adverse = rates(p, pool, &slow);
            score = 0.75 * score + 0.25 * rank(&adverse);
            for (v, a) in r.iter_mut().zip(adverse) {
                *v = 0.75 * *v + 0.25 * a;
            }
        }
        (r, score)
    };
    let (mut latest_rates, mut best_rank) = evaluate(&policy);
    let mut best = policy;
    let mut selected = 0;
    let mut validations = vec![(0, latest_rates.clone())];
    for i in 1..=iterations {
        if robust && i == iterations / 2 + 1 {
            policy = best;
            latest_rates = evaluate(&best).0;
        }
        let current = policy;
        let batch = if robust { 20 } else { BATCH };
        let cases: Vec<_> = (0..batch)
            .map(|slot| {
                let rival = if slot < pool.len() {
                    pool[slot]
                } else if slot < pool.len() + 2 {
                    Rival::Frozen("Current", &current)
                } else {
                    pool[sample_hard(rng, &latest_rates)]
                };
                let t = if robust && (i + slot) % 4 == 0 {
                    slower_trial(rng)
                } else {
                    trial(rng.next_u64(), false)
                };
                (rival, t)
            })
            .collect();
        let fraction = i as f64 / iterations as f64;
        ars::update(
            &mut policy.weights,
            rng,
            DIRECTIONS,
            8,
            0.38 - 0.21 * fraction,
            0.085 - 0.045 * fraction,
            |weights| {
                let actor = LeaguePolicy { weights };
                cases
                    .iter()
                    .map(|(r, t)| play(|o| actor.act(o), *r, t, true).episode_return)
                    .sum::<f64>()
                    / batch as f64
            },
        );
        if i % 4 == 0 {
            let score;
            (latest_rates, score) = evaluate(&policy);
            validations.push((i, latest_rates.clone()));
            if score > best_rank {
                best_rank = score;
                best = policy;
                selected = i;
                eprintln!(
                    "generation {} update {i}: {latest_rates:.2?}",
                    generation + 1
                );
            }
        }
    }
    eprintln!("generation {} selected update {selected}", generation + 1);
    let log = serde_json::json!({"generation":generation+1, "selected_update":selected,
        "roster":pool.iter().map(Rival::name).collect::<Vec<_>>(), "validations":validations});
    (best, log)
}

fn train() -> Result<(), Box<dyn Error>> {
    let mut rng = Random::new(TRAIN);
    let mut policy = LeaguePolicy::from_duel_rl();
    let mut archive = Vec::new();
    let mut history = Vec::new();
    for (generation, iterations) in GENERATIONS.into_iter().enumerate() {
        let (best, log) = train_stage(
            policy,
            &roster(&archive),
            &mut rng,
            generation,
            iterations,
            false,
        );
        history.push(log);
        policy = best;
        if generation + 1 < GENERATIONS.len() {
            archive.push(best);
        }
    }
    save("history-models.json", &archive)?;
    save("checkpoint.json", &policy)?;
    save("training.json", &history)
}
fn generation_three() -> LeaguePolicy {
    serde_json::from_str(include_str!("../../../models/archive/league_g3.json"))
        .expect("archived G3")
}
fn g2_challenger() -> LeaguePolicy {
    serde_json::from_str(include_str!(
        "../../../models/archive/league_challenger.json"
    ))
    .expect("archived G2 challenger")
}

fn robust_training() -> Result<(), Box<dyn Error>> {
    let archive = archived()?;
    let g3 = generation_three();
    let counter = g2_challenger();
    let mut pool = roster(&archive);
    pool.extend([
        Rival::Frozen("League-G3", &g3),
        Rival::Frozen("G2 challenger", &counter),
    ]);
    let (best, log) = train_stage(g3, &pool, &mut Random::new(0x4c17), 3, 192, true);
    save("robust-checkpoint.json", &best)?;
    save("robust-training.json", &log)
}

fn train_challenger(generation: usize) -> Result<LeaguePolicy, Box<dyn Error>> {
    let archive = archived()?;
    let g3 = generation_three();
    let (rival, seed, file) = match generation {
        3 => (
            Rival::Frozen("League-G3", &g3),
            0x4c21,
            "g3-challenger.json",
        ),
        4 => (
            Rival::Frozen("League-G4", pvp_policy::learned::league_trained()),
            0x5e11,
            "g4-challenger.json",
        ),
        _ => (
            Rival::Frozen("League-G2", &archive[1]),
            0x4c05,
            "challenger.json",
        ),
    };
    fit_challenger(rival, LeaguePolicy::from_duel_rl(), seed, file)
}

// A specialist sees only the target's behavior during play. Use a fresh stream for a held-out specialist.
fn fit_challenger(
    rival: Rival,
    mut policy: LeaguePolicy,
    seed: u64,
    file: &str,
) -> Result<LeaguePolicy, Box<dyn Error>> {
    let mut rng = Random::new(seed);
    let validation = rounds(seed + 1, 128, false);
    let mut best = policy;
    let mut best_wins = rates(&policy, &[rival], &validation)[0];
    let mut selected = 0;
    for i in 1..=64 {
        let cases: Vec<_> = (0..10).map(|_| trial(rng.next_u64(), false)).collect();
        ars::update(
            &mut policy.weights,
            &mut rng,
            DIRECTIONS,
            8,
            0.35,
            0.07,
            |weights| {
                let p = LeaguePolicy { weights };
                cases
                    .iter()
                    .map(|t| play(|o| p.act(o), rival, t, true).episode_return)
                    .sum::<f64>()
                    / cases.len() as f64
            },
        );
        if i % 4 == 0 {
            let wins = rates(&policy, &[rival], &validation)[0];
            if wins > best_wins {
                selected = i;
                best = policy;
                best_wins = wins;
            }
        }
    }
    save(file, &best)?;
    println!(
        "{} challenger update {selected}, validation {best_wins:.3}",
        rival.name()
    );
    Ok(best)
}

fn archived() -> Result<[LeaguePolicy; 2], serde_json::Error> {
    serde_json::from_str(include_str!("../../../models/archive/league_history.json"))
}

#[derive(Serialize, Default)]
struct Tally {
    games: usize,
    wins: usize,
    timeouts: usize,
}
impl Tally {
    fn add(&mut self, game: &Game) {
        self.games += 1;
        self.wins += usize::from(game.won());
        self.timeouts += usize::from(game.outcome == Outcome::Truncated);
    }
}
fn deployed(style: Style, rival: Rival, t: &Trial) -> Game {
    let mut bot = Bot::new(style);
    play(|o| bot.act(o), rival, t, false)
}
fn evaluate() -> Result<(), Box<dyn Error>> {
    let archive = archived()?;
    let g3 = generation_three();
    let counter = g2_challenger();
    let unseen = serde_json::from_str(include_str!(
        "../../../models/archive/league_g3_challenger.json"
    ))?;
    let mut rivals = roster(&archive);
    rivals.extend([
        Rival::Frozen("League-G3", &g3),
        Rival::Frozen("G2 challenger", &counter),
    ]);
    let trained_rivals = rivals.len();
    rivals.extend([
        Rival::Style(Style::Reach),
        Rival::Switch,
        Rival::Frozen("G3 challenger", &unseen),
    ]);
    let mut summary = Vec::new();
    let mut details = Vec::new();
    for (i, &rival) in rivals.iter().enumerate() {
        let cases = rounds(derive_seed(TEST, i as u64), 768, false);
        let paired: Vec<_> = cases
            .par_iter()
            .map(|t| {
                (
                    deployed(Style::LeagueRl, rival, t),
                    deployed(Style::DuelRl, rival, t),
                    play(|o| g3.act(o), rival, t, false),
                )
            })
            .collect();
        let mut overall = Tally::default();
        let mut previous = Tally::default();
        let mut generation_three = Tally::default();
        let mut by_rtt: [Tally; 3] = Default::default();
        for (game, old, earlier) in &paired {
            overall.add(game);
            previous.add(old);
            generation_three.add(earlier);
            let rtt = game.rtt_ms[0];
            by_rtt[usize::from(rtt >= 50.) + usize::from(rtt >= 100.)].add(game);
        }
        let stress: Vec<_> = rounds(derive_seed(STRESS, i as u64), 192, true)
            .par_iter()
            .map(|t| {
                (
                    deployed(Style::LeagueRl, rival, t),
                    play(|o| g3.act(o), rival, t, false),
                )
            })
            .collect();
        let mut adverse = Tally::default();
        let mut g3_adverse = Tally::default();
        for (game, earlier) in &stress {
            adverse.add(game);
            g3_adverse.add(earlier);
        }
        let (p, ci) = stats::statistics(overall.wins, overall.games);
        summary.push(serde_json::json!({
            "opponent": rival.name(), "held_out_opponent": i >= trained_rivals,
            "league": overall, "duel_rl_same_conditions": previous, "wilson_95": ci,
            "g3_same_conditions": generation_three, "g3_120_vs_20_ms": g3_adverse,
            "bonferroni_one_sided_p": (p*rivals.len() as f64).min(1.0),
            "own_rtt_5_50_100_150_ms": by_rtt, "120_vs_20_ms": adverse,
        }));
        println!(
            "{}: {}/{} (Duel-RL {}), 120 vs 20 ms {}/{}",
            rival.name(),
            overall.wins,
            overall.games,
            previous.wins,
            adverse.wins,
            adverse.games
        );
        details.push(serde_json::json!({"opponent":rival.name(),"paired_league_duel_rl_g3":paired,"stress_league_g3":stress}));
    }
    save(
        "evaluation.json",
        &serde_json::json!({"test_seed": TEST, "stress_seed": STRESS,
        "policy":pvp_policy::learned::league_trained(), "summary":summary, "episodes":details}),
    )?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("train") => train(),
        Some("challenger") => train_challenger(2).map(drop),
        Some("robust") => robust_training(),
        Some("g3-challenger") => train_challenger(3).map(drop),
        Some("g4-challenger") => train_challenger(4).map(drop),
        Some("g5-train") => g5::train(),
        Some("g5-refine") => g5::resume_refinement(),
        Some("g5-evaluate") => g5::evaluate(),
        Some("g6-train") => g6::train(),
        Some("g6-refine") => g6::resume_refinement(),
        Some("g6-evaluate") => g6::evaluate(),
        Some("audit-jumps") => g5::audit_jumps(),
        Some("evaluate") => evaluate(),
        _ => Err("usage: league_rl <g6-train|g6-refine|g6-evaluate|g5-train|g5-refine|g5-evaluate|audit-jumps|train|challenger|robust|g3-challenger|g4-challenger|evaluate>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_and_conditioned_gamma_use_learners_seat_after_sampling() {
        let mut rng = Random::new(142);
        let mut seats = [false; 2];
        for _ in 0..12 {
            let t = slower_trial(&mut rng);
            seats[t.seat] = true;
            let s = deployed(Style::LeagueRl, Rival::Style(Style::Baseline), &t);
            assert!((90.0..=150.0).contains(&s.rtt_ms[0]));
            assert!((5.0..=45.0).contains(&s.rtt_ms[1]));
            let fixed = trial(rng.next_u64(), true);
            let stress = deployed(Style::LeagueRl, Rival::Style(Style::Baseline), &fixed);
            assert_eq!(stress.rtt_ms, [120.0, 20.0]);
        }
        assert_eq!(seats, [true; 2]);
    }

    #[test]
    fn extra_observation_features_preserve_duel_rl_warm_start() {
        let actor = LeaguePolicy::from_duel_rl();
        let mut opponent = Bot::new(Style::CriticalPlus);
        let t = trial(2141, false);
        let mut duel = Duel::new(t.config, t.seed).unwrap();
        let mut obs = duel.observations();
        while !duel.is_finished() {
            let a = actor.act(&obs[0]);
            assert_eq!(a, pvp_policy::learned::trained().act(&obs[0]));
            obs = duel.step([a, opponent.act(&obs[1])]).unwrap().observations;
        }
    }
}
