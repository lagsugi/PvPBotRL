// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

use super::*;
use pvp_policy::learned::{G5Memory, G5Policy, league_trained};

// The fixed RTTs have identical upstream and downstream on both seats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
enum Network {
    Population,
    Equal20,
    Equal60,
    Equal120,
    Adverse,
}
const NETWORKS: [Network; 5] = [
    Network::Population,
    Network::Equal20,
    Network::Equal60,
    Network::Equal120,
    Network::Adverse,
];
fn scenario(seed: u64, net: Network) -> Trial {
    let mut t = trial(seed, net == Network::Adverse);
    let ms = match net {
        Network::Equal20 => 20.,
        Network::Equal60 => 60.,
        Network::Equal120 => 120.,
        _ => return t,
    };
    t.config.ping_profile = Some(PingProfile {
        fixed_ms: [Some(ms); 2],
        asymmetry: 0.,
    });
    t
}

struct League {
    history: [LeaguePolicy; 2],
    g3: LeaguePolicy,
    g2_counter: LeaguePolicy,
    g3_counter: LeaguePolicy,
    g4_counter: LeaguePolicy,
}
impl League {
    fn new(g4_counter: LeaguePolicy) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            history: archived()?,
            g3: generation_three(),
            g2_counter: g2_challenger(),
            g3_counter: serde_json::from_str(include_str!(
                "../../../../models/archive/league_g3_challenger.json"
            ))?,
            g4_counter,
        })
    }
    fn rivals(&self) -> Vec<Rival<'_>> {
        let mut pool = roster(&self.history);
        pool.extend([
            Rival::Frozen("League-G3", &self.g3),
            Rival::Frozen("G2 challenger", &self.g2_counter),
            Rival::Frozen("G3 challenger", &self.g3_counter),
            Rival::Frozen("G4 challenger", &self.g4_counter),
            Rival::Style(Style::LeagueRl),
        ]);
        pool
    }
}
fn fight(actor: &G5Policy, rival: Rival, trial: &Trial, shaping: bool) -> Game {
    let mut memory = G5Memory::default();
    play(|o| actor.act(&mut memory, o), rival, trial, shaping)
}
type Rates = Vec<[f64; NETWORKS.len()]>;
fn matrix(policy: &G5Policy, pool: &[Rival], cases: &[Vec<Trial>]) -> Rates {
    pool.par_iter()
        .map(|&r| {
            std::array::from_fn(|j| {
                let wins = cases[j]
                    .par_iter()
                    .filter(|t| fight(policy, r, t, false).won())
                    .count();
                wins as f64 / cases[j].len() as f64
            })
        })
        .collect()
}
fn selection_score(rates: &Rates) -> f64 {
    let g4 = rates.last().unwrap();
    let mut cells: Vec<f64> = rates.iter().flat_map(|r| r[..4].iter().copied()).collect();
    cells.sort_by(f64::total_cmp);
    let means: Vec<_> = rates
        .iter()
        .map(|r| (2. * r[0] + r[1] + r[2] + r[3]) / 5.)
        .collect();
    let adverse = rates.iter().map(|r| r[4]).sum::<f64>() / rates.len() as f64;
    0.33 * rank(&means)
        + 0.22 * cells[..8].iter().sum::<f64>() / 8.0
        + 0.25 * g4[..4].iter().copied().fold(1., f64::min)
        + 0.12 * g4[..4].iter().sum::<f64>() / 4.0
        + 0.08 * adverse
}

pub(super) fn train() -> Result<(), Box<dyn Error>> {
    // Generate the exploiter from the fixed G4, independent of prior benchmark files.
    let league = League::new(train_challenger(4)?)?;
    let pool = league.rivals();
    let dev: Vec<Vec<Trial>> = NETWORKS
        .iter()
        .enumerate()
        .map(|(j, &net)| {
            (0..80)
                .map(|i| scenario(derive_seed(0x5e20 + j as u64, i), net))
                .collect()
        })
        .collect();
    let mut policy = G5Policy::warm_start(league_trained());
    let mut rng = Random::new(0x5e10);
    const UPDATES: usize = 288;
    const SLOTS: usize = 26;
    const DIRS: usize = 32;
    let mut rates = matrix(&policy, &pool, &dev);
    let mut best = policy;
    let mut score = selection_score(&rates);
    let mut selected = 0;
    let mut history = vec![(0, score, rates.clone())];
    for i in 1..=UPDATES {
        if i == 97 || i == 193 {
            policy = best;
            rates = matrix(&policy, &pool, &dev);
        }
        let current = policy;
        let means: Vec<_> = rates.iter().map(|r| r.iter().sum::<f64>() / 5.).collect();
        let cases: Vec<_> = (0..SLOTS)
            .map(|slot| {
                let (rival, net) = if slot < pool.len() {
                    (pool[slot], NETWORKS[sample_hard(&mut rng, &rates[slot])])
                } else if slot < pool.len() + 4 {
                    (Rival::Style(Style::LeagueRl), NETWORKS[slot - pool.len()])
                } else if slot < pool.len() + 6 {
                    (Rival::G5(&current), NETWORKS[(rng.next_u64() % 4) as usize])
                } else {
                    let j = sample_hard(&mut rng, &means);
                    (pool[j], NETWORKS[sample_hard(&mut rng, &rates[j])])
                };
                (rival, scenario(rng.next_u64(), net))
            })
            .collect();
        let fraction = i as f64 / UPDATES as f64;
        ars::update(
            &mut policy.weights,
            &mut rng,
            DIRS,
            10,
            0.36 - 0.20 * fraction,
            0.08 - 0.042 * fraction,
            |weights| {
                let p = G5Policy { weights };
                cases
                    .iter()
                    .map(|(r, t)| fight(&p, *r, t, true).episode_return)
                    .sum::<f64>()
                    / SLOTS as f64
            },
        );
        if i % 8 == 0 {
            rates = matrix(&policy, &pool, &dev);
            let s = selection_score(&rates);
            history.push((i, s, rates.clone()));
            if s > score {
                score = s;
                best = policy;
                selected = i;
                eprintln!(
                    "G5 update {i} rank {s:.3}, vs G4 {:?}, worst ordinary cell {:.3}",
                    rates.last().unwrap(),
                    rates
                        .iter()
                        .flat_map(|r| &r[..4])
                        .copied()
                        .fold(1., f64::min)
                );
            }
        }
    }
    save("g5-checkpoint.json", &best)?;
    save(
        "g5-training.json",
        &serde_json::json!({"selected":selected, "updates":UPDATES,
        "training_games":UPDATES*DIRS*2*SLOTS, "rivals":pool.iter().map(Rival::name).collect::<Vec<_>>(),
        "networks":NETWORKS,"validation":history}),
    )?;
    println!("selected G5 update {selected}, score {score:.3}");
    refine(&league, best)
}

fn read_training<T: serde::de::DeserializeOwned>(name: &str) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_reader(fs::File::open(
        Path::new(OUTPUT).join(name),
    )?)?)
}
pub(super) fn resume_refinement() -> Result<(), Box<dyn Error>> {
    refine(
        &League::new(read_training("g4-challenger.json")?)?,
        read_training("g5-checkpoint.json")?,
    )
}

// Keep the first G5 candidate as a fallback. Give extra matches to cells where the
// predecessor was stronger, then select using a distinct development sample.
fn refine(league: &League, start: G5Policy) -> Result<(), Box<dyn Error>> {
    let pool = league.rivals();
    let cases: Vec<Vec<Trial>> = NETWORKS
        .iter()
        .enumerate()
        .map(|(j, &net)| {
            (0..112)
                .map(|i| scenario(derive_seed(0x5e91 + j as u64, i), net))
                .collect()
        })
        .collect();
    let incumbent = matrix(&G5Policy::warm_start(league_trained()), &pool, &cases);
    let mut policy = start;
    let mut rates = matrix(&policy, &pool, &cases);
    let score = |m: &Rates| {
        let mut drops: Vec<_> = m
            .iter()
            .zip(&incumbent)
            .flat_map(|(row, g4)| (0..4).map(move |k| (g4[k] - row[k]).max(0.)))
            .collect();
        drops.sort_by(|a, b| b.total_cmp(a));
        selection_score(m) - 0.5 * drops[..6].iter().sum::<f64>() / 6.
    };
    let mut best = policy;
    let mut best_score = score(&rates);
    let mut selected = 0;
    let mut log = vec![(0, best_score, rates.clone())];
    let mut rng = Random::new(0x5e88);
    for i in 1..=144 {
        if i == 49 || i == 97 {
            policy = best;
            rates = matrix(&best, &pool, &cases);
        }
        let mut difficult = Vec::new();
        for (j, row) in rates.iter().enumerate() {
            for (k, &win) in row.iter().enumerate() {
                let drop = (incumbent[j][k] - win).max(0.);
                difficult.push((win - 1.4 * drop, j, k));
            }
        }
        difficult.sort_by(|a, b| a.0.total_cmp(&b.0));
        let current = policy;
        let batch: Vec<_> = (0..24)
            .map(|slot| {
                let (r, net) = if slot < pool.len() {
                    (pool[slot], NETWORKS[(rng.next_u64() % 5) as usize])
                } else if slot < pool.len() + 2 {
                    (Rival::G5(&current), NETWORKS[(rng.next_u64() % 4) as usize])
                } else {
                    let (_, j, k) = difficult[(rng.next_u64() % 10) as usize];
                    (pool[j], NETWORKS[k])
                };
                (r, scenario(rng.next_u64(), net))
            })
            .collect();
        ars::update(
            &mut policy.weights,
            &mut rng,
            32,
            10,
            0.18,
            0.032,
            |weights| {
                let p = G5Policy { weights };
                batch
                    .iter()
                    .map(|(r, t)| fight(&p, *r, t, true).episode_return)
                    .sum::<f64>()
                    / batch.len() as f64
            },
        );
        if i % 8 == 0 {
            rates = matrix(&policy, &pool, &cases);
            let s = score(&rates);
            log.push((i, s, rates.clone()));
            if s > best_score {
                best = policy;
                best_score = s;
                selected = i;
                eprintln!("refine {i} rank {s:.3}, G4 {:?}", rates.last().unwrap());
            }
        }
    }
    save("g5-final.json", &best)?;
    save(
        "g5-refinement.json",
        &serde_json::json!({"selected":selected,"training_games":144*32*2*24,"validation":log}),
    )?;
    println!("selected G5 refinement {selected}");
    Ok(())
}

pub(super) fn evaluate() -> Result<(), Box<dyn Error>> {
    let league = League::new(serde_json::from_str(include_str!(
        "../../../../models/archive/league_g4_challenger.json"
    ))?)?;
    // Independent initialization and rollouts. G5 never trains against this branch.
    let probe = fit_challenger(
        Rival::Style(Style::LeagueRl),
        league.g3,
        0x5e61,
        "g5-unseen-rival.json",
    )?;
    let mut pool = league.rivals();
    let known = pool.len();
    pool.extend([
        Rival::Style(Style::Reach),
        Rival::Switch,
        Rival::Frozen("Unseen G4 challenger", &probe),
    ]);
    let mut summaries = Vec::new();
    let mut raw = Vec::new();
    for (j, rival) in pool.iter().copied().enumerate() {
        for (k, net) in NETWORKS.into_iter().enumerate() {
            let split = derive_seed(0x5e71, (j * NETWORKS.len() + k) as u64);
            let n = if net == Network::Adverse { 256 } else { 768 };
            let pairs: Vec<_> = (0..n)
                .into_par_iter()
                .map(|i| {
                    let t = scenario(derive_seed(split, i), net);
                    (
                        deployed(Style::LeagueG5, rival, &t),
                        deployed(Style::LeagueRl, rival, &t),
                    )
                })
                .collect();
            let mut new = Tally::default();
            let mut old = Tally::default();
            let mut gained = 0;
            let mut lost = 0;
            let mut by_rtt: [Tally; 3] = Default::default();
            for (a, b) in &pairs {
                new.add(a);
                old.add(b);
                gained += usize::from(a.won() && !b.won());
                lost += usize::from(!a.won() && b.won());
                let rtt = a.rtt_ms[0];
                by_rtt[usize::from(rtt >= 50.) + usize::from(rtt >= 100.)].add(a);
            }
            let (p, ci) = stats::statistics(new.wins, new.games);
            // The four non-stress direct matchups against G4 are the primary family.
            let primary_p = if rival.name() == Style::LeagueRl.name() && net != Network::Adverse {
                Some((4. * p).min(1.))
            } else {
                None
            };
            summaries.push(
                serde_json::json!({"opponent":rival.name(),"untrained_opponent":j>=known,
                "network":net,"g5":new,"g4":old,"wilson_95":ci,"primary_adjusted_p":primary_p,
                "own_rtt_5_50_100_150_ms":by_rtt,"g5_only_wins":gained,"g4_only_wins":lost}),
            );
            if rival.name() == Style::LeagueRl.name() || net == Network::Population {
                eprintln!(
                    "{} {net:?}: G5 {}/{}, G4 {}",
                    rival.name(),
                    new.wins,
                    new.games,
                    old.wins
                );
            }
            raw.push(serde_json::json!({"opponent":rival.name(),"network":net,"split":split,"g5_then_g4":pairs}));
        }
    }
    save(
        "g5-summary.json",
        &serde_json::json!({"seed":0x5e71_u64,"policy":pvp_policy::learned::g5_trained(),
        "unseen_g4_challenger":probe,"jump_comparison":jump_comparison(),"rows":summaries}),
    )?;
    save("g5-episodes.json", &raw)
}

// Paired intervention: both matches start identically and only the tested seat's jump key is masked.
fn jump_comparison() -> serde_json::Value {
    let mut result = Vec::new();
    for style in [Style::LeagueRl, Style::LeagueG5] {
        let wins = [true, false].map(|allow| {
            (0..384_u64)
                .into_par_iter()
                .filter(|&i| {
                    let t = scenario(derive_seed(0x5e81, i), Network::Population);
                    let mut bot = Bot::new(style);
                    play(
                        |o| {
                            let mut a = bot.act(o);
                            a.keys.jump &= allow;
                            a
                        },
                        Rival::Style(Style::LeagueRl),
                        &t,
                        false,
                    )
                    .won()
                })
                .count()
        });
        result.push(
            serde_json::json!({"actor":style.name(),"rival":"League-RL","games_per_setting":384,
            "wins_normal":wins[0],"wins_without_jump":wins[1]}),
        );
    }
    serde_json::json!(result)
}

// Replay accepted human hits. A nearby jump input is a timing observation, not a knockback counterfactual.
pub(super) fn audit_jumps() -> Result<(), Box<dyn Error>> {
    use pvp_sim::trace::{Replay, TraceEvent};
    let mut matched = 0;
    let mut hits = 0;
    let mut near = 0;
    let mut airborne = 0;
    let mut skipped = 0;
    for entry in fs::read_dir("recordings")? {
        let dir = entry?.path();
        if !dir.join("metadata.json").is_file() {
            continue;
        }
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("metadata.json"))?)?;
        if meta["mode"] != "human"
            || meta["opponent"] != "league-rl"
            || !dir.join("replay.json").is_file()
        {
            continue;
        }
        let replay: Replay = serde_json::from_reader(std::io::BufReader::new(fs::File::open(
            dir.join("replay.json"),
        )?))?;
        let mut config = replay.config;
        config.trace_capacity = 2048;
        let mut env = Duel::new(config, replay.seed)?;
        let mut hit_steps = Vec::new();
        let mut jumps = Vec::new();
        let mut matches_model = !replay.actions.is_empty();
        for (i, actions) in replay.actions.into_iter().enumerate() {
            let obs = env.observe(1)?;
            if league_trained().act(&obs).applied() != actions[1] {
                matches_model = false;
                break;
            }
            if actions[1].keys.jump && obs.on_ground {
                jumps.push(i);
            }
            env.step_recorded(actions)?;
            for event in env.drain_trace() {
                if let TraceEvent::Attack {
                    attacker: 0,
                    result,
                } = event.event
                {
                    if result.damage.accepted {
                        hit_steps.push((i, !obs.on_ground));
                    }
                }
            }
            assert_eq!(env.dropped_trace_records(), 0);
        }
        if !matches_model {
            skipped += 1;
            continue;
        }
        matched += 1;
        for (i, air) in hit_steps {
            hits += 1;
            near += usize::from(jumps.iter().any(|&j| j.abs_diff(i) <= 1));
            airborne += usize::from(air);
        }
    }
    let out = serde_json::json!({"g4_matching_replays":matched,"mismatched_or_empty":skipped,
        "accepted_human_hits":hits,"near_grounded_jump_key_plus_minus_one_step":near,
        "g4_airborne_at_start_of_hit_step":airborne});
    println!("{out}");
    save("g4-human-jumps.json", &out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_rtt_gives_both_seats_identical_one_way_delay() {
        for (net, ms) in [
            (Network::Equal20, 20),
            (Network::Equal60, 60),
            (Network::Equal120, 120),
        ] {
            let t = scenario(11, net);
            let d = Duel::new(t.config, t.seed).unwrap();
            for c in &d.config().clients {
                assert_eq!(
                    (c.upstream_ns, c.downstream_ns),
                    (ms * 500_000, ms * 500_000)
                );
            }
        }
    }
    #[test]
    fn g5_training_and_deployed_actions_match_across_new_episodes() {
        for seed in [11, 57] {
            let t = scenario(seed, Network::Population);
            let mut d = Duel::new(t.config, t.seed).unwrap();
            let mut bots = [Bot::new(Style::LeagueG5), Bot::new(Style::LeagueG5)];
            let mut memories: [G5Memory; 2] = Default::default();
            let mut obs = d.observations();
            while !d.is_finished() {
                let actions = std::array::from_fn(|i| bots[i].act(&obs[i]));
                for i in 0..2 {
                    assert_eq!(
                        actions[i],
                        pvp_policy::learned::g5_trained().act(&mut memories[i], &obs[i])
                    );
                }
                obs = d.step(actions).unwrap().observations;
            }
        }
    }
}
