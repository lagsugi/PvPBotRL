// SPDX-FileCopyrightText: 2026 lagsugi
// SPDX-License-Identifier: GPL-3.0-only
// Additional permissions are in LICENSE-EXCEPTIONS.md at the project root.

use super::*;
use pvp_policy::learned::{G5Memory, G6Policy, g5_trained, g6_trained};
use pvp_rl::league::{LOCAL_RIVAL, NETWORKS, Network, scenario};

struct League {
    history: [LeaguePolicy; 2],
    frozen: [LeaguePolicy; 4],
}

impl League {
    fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            history: archived()?,
            frozen: [
                generation_three(),
                g2_challenger(),
                serde_json::from_str(include_str!(
                    "../../../../models/archive/league_g3_challenger.json"
                ))?,
                serde_json::from_str(include_str!(
                    "../../../../models/archive/league_g4_challenger.json"
                ))?,
            ],
        })
    }

    fn rivals(&self) -> Vec<Rival<'_>> {
        let mut pool = vec![Rival::Style(Style::LeagueG5), Rival::Style(Style::LeagueRl)];
        pool.extend(roster(&self.history));
        pool.extend(
            [
                "League-G3",
                "G2 challenger",
                "G3 challenger",
                "G4 challenger",
            ]
            .into_iter()
            .zip(&self.frozen)
            .map(|(name, policy)| Rival::Frozen(name, policy)),
        );
        pool.push(Rival::Style(Style::Reach));
        pool
    }
}

fn fight(policy: &G6Policy, rival: Rival, t: &Trial, bonus: f64) -> (Game, Attacks) {
    let mut memory = G5Memory::default();
    play_match(
        |o| policy.act(&mut memory, o),
        rival,
        t,
        bonus > 0.,
        Some(bonus),
    )
}

#[derive(Default, Serialize)]
struct Cell {
    tally: Tally,
    attacks: Attacks,
}

impl Cell {
    fn add(&mut self, (game, attacks): &(Game, Attacks)) {
        self.tally.add(game);
        self.attacks.critical += attacks.critical;
        self.attacks.damaging += attacks.damaging;
        self.attacks.rising_clicks += attacks.rising_clicks;
    }

    fn wins(&self) -> f64 {
        self.tally.wins as f64 / self.tally.games as f64
    }
}

type Matrix = Vec<Vec<Cell>>;

fn validation(policy: &G6Policy, pool: &[Rival], cases: &[Vec<Trial>]) -> Matrix {
    pool.par_iter()
        .map(|&rival| {
            cases
                .iter()
                .map(|trials| {
                    let mut cell = Cell::default();
                    for t in trials {
                        cell.add(&fight(policy, rival, t, 0.));
                    }
                    cell
                })
                .collect()
        })
        .collect()
}

fn cases(networks: &[Network], seed: u64, n: u64) -> Vec<Vec<Trial>> {
    networks
        .iter()
        .enumerate()
        .map(|(i, &net)| {
            let split = derive_seed(seed, i as u64);
            (0..n)
                .map(|j| scenario(derive_seed(split, j), net))
                .collect()
        })
        .collect()
}

fn selection_score(matrix: &Matrix, baseline: &Matrix) -> f64 {
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len() as f64;
    let direct: Vec<_> = matrix[0].iter().map(Cell::wins).collect();
    let general: Vec<_> = matrix[1..].iter().flatten().map(Cell::wins).collect();
    let mut drops: Vec<_> = matrix[1..]
        .iter()
        .flatten()
        .zip(baseline[1..].iter().flatten())
        .map(|(a, b)| (b.wins() - a.wins()).max(0.))
        .collect();
    drops.sort_by(|a, b| b.total_cmp(a));
    let local_crit: Vec<_> = matrix
        .iter()
        .map(|row| {
            let cell = &row[LOCAL_RIVAL];
            cell.attacks.critical as f64 / cell.attacks.damaging.max(1) as f64
        })
        .collect();
    0.35 * mean(&direct)
        + 0.10 * direct.iter().copied().fold(1., f64::min)
        + 0.06 * direct[LOCAL_RIVAL]
        + 0.43 * mean(&general)
        - 0.10 * mean(&drops[..12])
        + 0.06 * mean(&local_crit)
}

pub(super) fn train() -> Result<(), Box<dyn Error>> {
    const STAGES: [(usize, usize, usize); 2] = [(144, 28, 32), (144, 32, 36)];
    let league = League::new()?;
    let pool = league.rivals();
    let start = G6Policy::warm_start(g5_trained());
    let mut best = start;
    let mut log = Vec::new();
    let mut training_games = 0;
    for (stage, (updates, directions, slots)) in STAGES.into_iter().enumerate() {
        let dev = cases(
            &NETWORKS,
            0x6d10 + stage as u64,
            if stage == 0 { 40 } else { 64 },
        );
        let baseline = validation(&start, &pool, &dev);
        let mut policy = best;
        let mut rates = validation(&policy, &pool, &dev);
        let mut score = selection_score(&rates, &baseline);
        eprintln!("G6 stage {} baseline {score:.4}", stage + 1);
        let mut selected = 0;
        let mut rng = Random::new(0x6a10 + stage as u64);
        let mut history = vec![(0, score)];
        for i in 1..=updates {
            if i > 1 && (i - 1) % (updates / 4) == 0 {
                policy = best;
                rates = validation(&policy, &pool, &dev);
            }
            let current = policy;
            let mut difficult = Vec::new();
            for (j, row) in rates.iter().enumerate().skip(1) {
                for (k, cell) in row.iter().enumerate() {
                    let drop = (baseline[j][k].wins() - cell.wins()).max(0.);
                    difficult.push((cell.wins() - 1.2 * drop, j, k));
                }
            }
            difficult.sort_by(|a, b| a.0.total_cmp(&b.0));
            let batch: Vec<_> = (0..slots)
                .map(|slot| {
                    let random_network =
                        |rng: &mut Random| NETWORKS[rng.next_u64() as usize % NETWORKS.len()];
                    let (rival, net) = if slot < pool.len() {
                        let net = if slot == 0 || rng.next_u64() % 5 == 0 {
                            Network::LocalRival
                        } else {
                            random_network(&mut rng)
                        };
                        (pool[slot], net)
                    } else if slot < pool.len() + 10 {
                        (pool[0], NETWORKS[slot - pool.len()])
                    } else if slot == pool.len() + 10 {
                        (Rival::G6(&current), Network::LocalRival)
                    } else if slot == pool.len() + 11 {
                        (Rival::G6(&current), Network::Mixed)
                    } else if slot == pool.len() + 12 {
                        (pool[0], Network::Bot20Rival0)
                    } else {
                        let (_, j, k) = difficult[rng.next_u64() as usize % 16];
                        (pool[j], NETWORKS[k])
                    };
                    let bonus = match net {
                        Network::LocalRival
                        | Network::Both0
                        | Network::Both2
                        | Network::Bot20Rival0 => 0.055,
                        _ => 0.018,
                    };
                    (rival, scenario(rng.next_u64(), net), bonus)
                })
                .collect();
            let progress = i as f64 / updates as f64;
            let (noise, rate) = if stage == 0 {
                (0.125 - 0.035 * progress, 0.041 - 0.013 * progress)
            } else {
                (0.080 - 0.029 * progress, 0.025 - 0.010 * progress)
            };
            ars::update(
                &mut policy.weights,
                &mut rng,
                directions,
                10,
                noise,
                rate,
                |weights| {
                    let candidate = G6Policy { weights };
                    batch
                        .iter()
                        .map(|(r, t, bonus)| fight(&candidate, *r, t, *bonus).0.episode_return)
                        .sum::<f64>()
                        / slots as f64
                },
            );
            if i % 6 == 0 {
                rates = validation(&policy, &pool, &dev);
                let candidate_score = selection_score(&rates, &baseline);
                history.push((i, candidate_score));
                if candidate_score > score {
                    score = candidate_score;
                    best = policy;
                    selected = i;
                    save("g6-final.json", &best)?;
                }
                let local = &rates[0][LOCAL_RIVAL];
                eprintln!(
                    "G6 stage {} update {i}/{updates}: score {candidate_score:.4}, best {score:.4} at {selected}, local G5 {}/{} wins, {} criticals",
                    stage + 1,
                    local.tally.wins,
                    local.tally.games,
                    local.attacks.critical
                );
            }
        }
        let games = updates * directions * 2 * slots;
        training_games += games;
        log.push(serde_json::json!({"updates":updates,"directions":directions,"trials_per_candidate":slots,"selected":selected,"games":games,"validation":history}));
        save("g6-final.json", &best)?;
        save(
            "g6-training.json",
            &serde_json::json!({"training_games":training_games,"networks":NETWORKS,"rivals":pool.iter().map(Rival::name).collect::<Vec<_>>(),"stages":log}),
        )?;
    }
    save("g6-stage2.json", &best)?;
    refine(&pool, best, training_games)
}

pub(super) fn resume_refinement() -> Result<(), Box<dyn Error>> {
    let league = League::new()?;
    let start = serde_json::from_reader(fs::File::open(Path::new(OUTPUT).join("g6-stage2.json"))?)?;
    let log: serde_json::Value =
        serde_json::from_reader(fs::File::open(Path::new(OUTPUT).join("g6-training.json"))?)?;
    refine(
        &league.rivals(),
        start,
        log["training_games"]
            .as_u64()
            .ok_or("missing G6 training count")? as usize,
    )
}

fn refine(pool: &[Rival], start: G6Policy, prior_games: usize) -> Result<(), Box<dyn Error>> {
    const UPDATES: usize = 144;
    const DIRECTIONS: usize = 32;
    const SLOTS: usize = 36;
    let networks: Vec<_> = NETWORKS
        .into_iter()
        .chain([Network::Bot20Rival0, Network::EarlyBot20Rival0])
        .collect();
    let dev = cases(&networks, 0x6da2, 64);
    let baseline = validation(&G6Policy::warm_start(g5_trained()), pool, &dev);
    let score = |rates: &Matrix| {
        let mean = |sum: f64, n: usize| sum / n as f64;
        let general = rates[1..].iter().flatten();
        let overall = mean(
            general.clone().map(Cell::wins).sum(),
            (pool.len() - 1) * networks.len(),
        );
        let direct = mean(rates[0].iter().map(Cell::wins).sum(), networks.len());
        let local_indices = [1, LOCAL_RIVAL, networks.len() - 2, networks.len() - 1];
        let local = rates[1..]
            .iter()
            .flat_map(|row| local_indices.map(|k| &row[k]));
        let n_local = (pool.len() - 1) * local_indices.len();
        let local_win = mean(local.clone().map(Cell::wins).sum(), n_local);
        let crit = mean(
            local
                .map(|c| c.attacks.critical as f64 / c.attacks.damaging.max(1) as f64)
                .sum(),
            n_local,
        );
        let mut drops: Vec<_> = general
            .zip(baseline[1..].iter().flatten())
            .map(|(a, b)| (b.wins() - a.wins()).max(0.))
            .collect();
        drops.sort_by(|a, b| b.total_cmp(a));
        0.47 * overall + 0.22 * local_win + 0.23 * direct + 0.08 * crit
            - 0.14 * mean(drops[..16].iter().sum(), 16)
    };
    let mut policy = start;
    let mut best = start;
    let mut rates = validation(&policy, pool, &dev);
    let mut best_score = score(&rates);
    let mut selected = 0;
    let mut rng = Random::new(0x6af3);
    let mut history = vec![(0, best_score)];
    for i in 1..=UPDATES {
        if i > 1 && (i - 1) % 36 == 0 {
            policy = best;
            rates = validation(&policy, pool, &dev);
        }
        let current = policy;
        let mut difficult = Vec::new();
        for (j, row) in rates.iter().enumerate().skip(1) {
            for (k, c) in row.iter().enumerate() {
                let drop = (baseline[j][k].wins() - c.wins()).max(0.);
                difficult.push((c.wins() - 1.55 * drop, j, k));
            }
        }
        difficult.sort_by(|a, b| a.0.total_cmp(&b.0));
        let batch: Vec<_> = (0..SLOTS)
            .map(|slot| {
                let choose = |rng: &mut Random| networks[rng.next_u64() as usize % networks.len()];
                let (rival, net) = if slot < pool.len() {
                    (pool[slot], choose(&mut rng))
                } else if slot < pool.len() + 5 {
                    (pool[0], choose(&mut rng))
                } else if slot < pool.len() + 7 {
                    (Rival::G6(&current), choose(&mut rng))
                } else if slot < pool.len() + 13 {
                    let r = pool[1 + rng.next_u64() as usize % (pool.len() - 1)];
                    let net = [
                        Network::LocalRival,
                        Network::Bot20Rival0,
                        Network::EarlyBot20Rival0,
                    ][slot % 3];
                    (r, net)
                } else {
                    let (_, j, k) = difficult[rng.next_u64() as usize % 24];
                    (pool[j], networks[k])
                };
                let bonus = match net {
                    Network::Both0
                    | Network::Both2
                    | Network::LocalRival
                    | Network::Bot20Rival0
                    | Network::EarlyBot20Rival0 => 0.035,
                    _ => 0.012,
                };
                (rival, scenario(rng.next_u64(), net), bonus)
            })
            .collect();
        let scale = i as f64 / UPDATES as f64;
        ars::update(
            &mut policy.weights,
            &mut rng,
            DIRECTIONS,
            12,
            0.09 - 0.045 * scale,
            0.024 - 0.012 * scale,
            |weights| {
                let candidate = G6Policy { weights };
                batch
                    .iter()
                    .map(|(r, t, bonus)| fight(&candidate, *r, t, *bonus).0.episode_return)
                    .sum::<f64>()
                    / SLOTS as f64
            },
        );
        if i % 6 == 0 {
            rates = validation(&policy, pool, &dev);
            let value = score(&rates);
            history.push((i, value));
            if value > best_score {
                best_score = value;
                best = policy;
                selected = i;
            }
            eprintln!(
                "G6 timing refinement {i}/{UPDATES}: score {value:.4}, best {best_score:.4} at {selected}"
            );
        }
    }
    let games = UPDATES * DIRECTIONS * 2 * SLOTS;
    save("g6-final.json", &best)?;
    save(
        "g6-refinement.json",
        &serde_json::json!({"training_games":prior_games+games,"games":games,"selected":selected,"networks":networks,"validation":history}),
    )
}

pub(super) fn evaluate() -> Result<(), Box<dyn Error>> {
    const SEED: u64 = 0x6ea8;
    let policy = g6_trained();
    let league = League::new()?;
    let mut pool = league.rivals();
    pool.push(Rival::Switch);
    let base = G6Policy::warm_start(g5_trained());
    let mut rows = Vec::new();
    let networks = NETWORKS.into_iter().chain([
        Network::Bot20Rival0,
        Network::EarlyBot20Rival0,
        Network::Legacy,
    ]);
    let comparisons = networks.clone().count();
    for (j, rival) in pool.into_iter().enumerate() {
        for (k, network) in networks.clone().enumerate() {
            let n = if j == 0 { 768 } else { 256 };
            let split = derive_seed(SEED, (j * comparisons + k) as u64);
            let pairs: Vec<_> = (0..n as u64)
                .into_par_iter()
                .map(|i| {
                    let t = scenario(derive_seed(split, i), network);
                    (fight(policy, rival, &t, 0.), fight(&base, rival, &t, 0.))
                })
                .collect();
            let mut g6 = Cell::default();
            let mut g5 = Cell::default();
            let mut gained = 0;
            let mut lost = 0;
            for (new, old) in &pairs {
                g6.add(new);
                g5.add(old);
                gained += usize::from(new.0.won() && !old.0.won());
                lost += usize::from(!new.0.won() && old.0.won());
            }
            let (p, ci) = stats::statistics(g6.tally.wins, n);
            if network == Network::Mixed
                || j == 0
                || network == Network::Bot20Rival0
                || network == Network::EarlyBot20Rival0
            {
                eprintln!(
                    "{} {network:?}: G6 {}/{n}, G5 {}/{n}; criticals {} vs {}",
                    rival.name(),
                    g6.tally.wins,
                    g5.tally.wins,
                    g6.attacks.critical,
                    g5.attacks.critical
                );
            }
            rows.push(serde_json::json!({"opponent":rival.name(),"network":network,"split":split,
                "g6":g6,"g5":g5,"g6_only_wins":gained,"g5_only_wins":lost,
                "direct_win_wilson_95":(j == 0).then_some(ci),"direct_adjusted_p":(j == 0).then_some((p * comparisons as f64).min(1.))}));
        }
    }
    save(
        "g6-summary.json",
        &serde_json::json!({"seed":SEED,"policy":policy,"rows":rows}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localhost_and_boundary_rtt_follow_the_actor_across_seats() {
        let mut seats = [false; 2];
        for seed in 0..32 {
            for (network, expected) in [
                (Network::Both0, [0., 0.]),
                (Network::Both150, [150., 150.]),
                (Network::Bot20Rival0, [20., 0.]),
                (Network::EarlyBot20Rival0, [20., 0.]),
                (Network::Adverse, [150., 0.]),
            ] {
                let t = scenario(seed, network);
                seats[t.seat] = true;
                let d = Duel::new(t.config, t.seed).unwrap();
                let rtt = [t.seat, 1 - t.seat].map(|seat| {
                    let c = &d.config().clients[seat];
                    (c.upstream_ns + c.downstream_ns) as f64 / 1e6
                });
                assert_eq!(rtt, expected);
            }
        }
        assert_eq!(seats, [true; 2]);
    }

    #[test]
    fn early_phase_delivers_a_localhost_match_attack_one_tick_sooner() {
        let t = scenario(42, Network::EarlyBot20Rival0);
        let mut early = t.config;
        early.spawns[0].position = pvp_sim::math::Vec3::new(0., 0., -1.);
        early.spawns[1].position = pvp_sim::math::Vec3::new(0., 0., 1.);
        let mut late = early.clone();
        late.server_phase_ns = 0;
        let mut actions = [Action::default(); 2];
        actions[t.seat].click = true;
        let mut early = Duel::new(early, t.seed).unwrap();
        let mut late = Duel::new(late, t.seed).unwrap();
        early.step(actions).unwrap();
        late.step(actions).unwrap();
        assert!(early.snapshot().server[1 - t.seat].health < 20.);
        assert_eq!(late.snapshot().server[1 - t.seat].health, 20.);
        late.step([Action::default(); 2]).unwrap();
        assert!(late.snapshot().server[1 - t.seat].health < 20.);
    }

    #[test]
    fn warm_start_matches_g5_across_a_duel() {
        let t = scenario(42, Network::Bot20Rival0);
        let mut d = Duel::new(t.config, t.seed).unwrap();
        let g5 = g5_trained();
        let g6 = G6Policy::warm_start(g5);
        let mut g5_memory = G5Memory::default();
        let mut g6_memory = G5Memory::default();
        let mut opponent = Bot::new(Style::Pressure);
        while !d.is_finished() {
            let obs = d.observations();
            let old = g5.act(&mut g5_memory, &obs[t.seat]);
            assert_eq!(g6.act(&mut g6_memory, &obs[t.seat]), old);
            let mut actions = [Action::default(); 2];
            actions[t.seat] = old;
            actions[1 - t.seat] = opponent.act(&obs[1 - t.seat]);
            d.step(actions).unwrap();
        }
    }
}
