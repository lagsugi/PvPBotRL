//! Train a linear policy against frozen Critical+, or evaluate the embedded checkpoint.
use pvp_policy::{
    learned::{FEATURES, LinearPolicy, OUTPUTS, trained},
    policy::{Bot, Style},
};
use pvp_sim::{Action, Config, Duel, Outcome, TICK_NS, rng::derive_seed};
use rayon::prelude::*;
use serde::Serialize;
use std::{error::Error, fs, path::Path};
#[path = "common/ars.rs"]
mod ars;
use ars::Random;
#[path = "common/statistics.rs"]
mod stats;
use stats::statistics;

const DIR: &str = "build/benchmarks/duel-rl";
const TRAIN: u64 = 0x0054_5241_494e;
const VALID: u64 = 0x0056_414c_4944;
const TEST: u64 = 0x5445_5354;
const TEST_MATCHES: usize = 512;

struct Trial {
    config: Config,
    seed: u64,
    seat: usize,
}
fn random_trial(seed: u64) -> Trial {
    let mut rng = Random::new(seed);
    let mut config = Config::default();
    let gap = 3.5 + 28.5 * rng.unit();
    config.spawns[0].position.z = -gap / 2.0;
    config.spawns[1].position.z = gap / 2.0;
    config.spawns[1].position.x = 4.0 * rng.unit() - 2.0;
    for client in &mut config.clients {
        client.upstream_ns = rng.next_u64() % 90_000_001;
        client.downstream_ns = rng.next_u64() % 90_000_001;
        client.fps = [30, 60, 144, 240][(rng.next_u64() % 4) as usize];
        client.tick_phase_ns = rng.next_u64() % TICK_NS;
        client.frame_phase_ns = rng.next_u64() % (1_000_000_000 / u64::from(client.fps));
    }
    config.server_phase_ns = rng.next_u64() % TICK_NS;
    config.first_client = Some((rng.next_u64() % 2) as usize);
    Trial {
        config,
        seed: rng.next_u64(),
        seat: (rng.next_u64() % 2) as usize,
    }
}
fn standard(i: usize) -> Trial {
    Trial {
        config: Config {
            first_client: Some(i % 2),
            ..Config::default()
        },
        seed: 42,
        seat: i / 2,
    }
}
fn trials(split: u64, count: usize) -> Vec<Trial> {
    (0..count)
        .map(|i| random_trial(derive_seed(split, i as u64)))
        .collect()
}

#[derive(Serialize)]
struct Episode {
    duel_seed: u64,
    first_client: usize,
    outcome: Outcome,
    seat: usize,
    steps: u64,
    episode_return: f64,
}
fn rollout(
    mut act: impl FnMut(&pvp_sim::Observation) -> Action,
    trial: &Trial,
    shaping: bool,
) -> Episode {
    let mut config = trial.config.clone();
    if shaping {
        config.reward.damage = 0.15;
        config.reward.time_penalty = 0.0001;
    }
    let mut env = Duel::new(config, trial.seed).unwrap();
    let mut opponent = Bot::new(Style::CriticalPlus);
    let mut obs = env.observations();
    let mut episode_return = 0.0;
    loop {
        let seat = trial.seat;
        let mut actions = [Action::default(); 2];
        actions[seat] = act(&obs[seat]);
        actions[1 - seat] = opponent.act(&obs[1 - seat]);
        let step = env.step(actions).unwrap();
        episode_return += f64::from(step.rewards[seat]);
        if step.terminated || step.truncated {
            return Episode {
                duel_seed: trial.seed,
                first_client: trial
                    .config
                    .first_client
                    .expect("trial specifies client order"),
                outcome: step.outcome,
                seat,
                steps: step.decision_step,
                episode_return,
            };
        }
        obs = step.observations;
    }
}

#[derive(Debug, Default, Serialize)]
struct Score {
    wins: usize,
    losses: usize,
    timeouts: usize,
}
fn score(episodes: &[Episode]) -> Score {
    let mut s = Score::default();
    for e in episodes {
        match e.outcome {
            Outcome::Victory { winner } if winner == e.seat => s.wins += 1,
            Outcome::Victory { .. } => s.losses += 1,
            Outcome::Truncated => s.timeouts += 1,
            Outcome::Ongoing => unreachable!(),
        }
    }
    s
}
fn validation(p: &LinearPolicy, validation: &[Trial]) -> (usize, usize, f64) {
    let run = |t: &Trial| rollout(|o| p.act(o), t, true);
    let standard_results: Vec<_> = (0..4).map(|i| run(&standard(i))).collect();
    let results: Vec<_> = validation.par_iter().map(run).collect();
    (
        score(&standard_results).wins,
        score(&results).wins,
        results.iter().map(|e| e.episode_return).sum(),
    )
}
fn save(file: &str, obj: &impl Serialize) -> Result<(), Box<dyn Error>> {
    let dir = Path::new(DIR);
    fs::create_dir_all(dir)?;
    fs::write(dir.join(file), serde_json::to_string_pretty(obj)? + "\n")?;
    Ok(())
}

// ARS with paired rollouts (common conditions for positive/negative perturbations).
fn train() -> Result<(), Box<dyn Error>> {
    const ITERATIONS: usize = 64;
    const DIRECTIONS: usize = 24;
    const TOP: usize = 8;
    const BATCH: usize = 10;
    let mut rng = Random::new(TRAIN);
    let mut policy = LinearPolicy {
        weights: [[0.0; FEATURES]; OUTPUTS],
    };
    // A straight approach and charged swing give exploration a playable starting point.
    policy.weights[0][0] = 0.75;
    policy.weights[1][0] = -0.4;
    policy.weights[2][0] = -0.4;
    policy.weights[3][0] = 0.25;
    policy.weights[3][2] = -2.0;
    let validation_trials = trials(VALID, 96);
    let mut best = policy;
    let mut best_score = validation(&policy, &validation_trials);
    let mut selected = 0;
    let mut history = vec![(0, best_score)];
    eprintln!("initial validation (standard, randomized, return): {best_score:?}");
    for iteration in 1..=ITERATIONS {
        let progress = iteration as f64 / ITERATIONS as f64;
        let noise = 0.45 - 0.27 * progress;
        let rate = 0.10 - 0.06 * progress;
        let conditions: Vec<_> = (0..BATCH)
            .map(|i| {
                if i < 2 {
                    standard((rng.next_u64() % 4) as usize)
                } else {
                    random_trial(rng.next_u64())
                }
            })
            .collect();
        ars::update(
            &mut policy.weights,
            &mut rng,
            DIRECTIONS,
            TOP,
            noise,
            rate,
            |weights| {
                let actor = LinearPolicy { weights };
                conditions
                    .iter()
                    .map(|t| rollout(|o| actor.act(o), t, true).episode_return)
                    .sum::<f64>()
                    / BATCH as f64
            },
        );
        if iteration % 2 == 0 {
            let s = validation(&policy, &validation_trials);
            history.push((iteration, s));
            if s > best_score {
                selected = iteration;
                best_score = s;
                best = policy;
                eprintln!("iteration {iteration}: {s:?}");
            }
        }
    }
    save("checkpoint.json", &best)?;
    save(
        "training.json",
        &serde_json::json!({
            "iterations": ITERATIONS, "training_episodes": ITERATIONS * DIRECTIONS * 2 * BATCH,
            "selected_iteration": selected, "validation_history": history,
        }),
    )?;
    Ok(())
}

fn evaluate() -> Result<(), Box<dyn Error>> {
    let run = |t: &Trial| {
        let mut deployed = Bot::new(Style::DuelRl);
        rollout(|o| deployed.act(o), t, false)
    };
    let standard_results: Vec<_> = (0..4).map(|i| run(&standard(i))).collect();
    let results: Vec<_> = trials(TEST, TEST_MATCHES).par_iter().map(run).collect();
    let s = score(&results);
    let (p, ci) = statistics(s.wins, results.len());
    let summary = serde_json::json!({
        "opponent": Style::CriticalPlus.name(), "test_seed": TEST, "randomized": s,
        "wilson_95": ci, "one_sided_binomial_p": p, "standard": score(&standard_results),
        "weights": trained().weights,
    });
    save(
        "evaluation.json",
        &serde_json::json!({"summary": summary, "episodes": results, "standard": standard_results}),
    )?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("train") => train(),
        Some("evaluate") => evaluate(),
        _ => Err("usage: duel_rl <train|evaluate>".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_upper_tail_includes_observed_wins() {
        assert_eq!(statistics(3, 4).0, 5.0 / 16.0);
        assert_eq!(
            statistics(TEST_MATCHES, TEST_MATCHES).0,
            0.5_f64.powi(TEST_MATCHES as i32)
        );
        let (_, ci) = statistics(5, 10);
        assert!((ci[0] - 0.23659).abs() < 0.00001);
        assert!((ci[1] - 0.76341).abs() < 0.00001);
    }

    #[test]
    fn shipped_bot_wins_standard_matchup_in_both_seats_and_orders() {
        for i in 0..4 {
            let trial = standard(i);
            let mut bot = Bot::new("duel-rl".parse().unwrap());
            let result = rollout(|o| bot.act(o), &trial, false);
            assert_eq!(result.outcome, Outcome::Victory { winner: trial.seat });
        }
    }
}
