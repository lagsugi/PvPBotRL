//! Reproducible parameter search and independent evaluation. Actors see only observations.
use pvp_policy::policy::{Bot, CriticalParams, Style};
use pvp_sim::{Config, Duel, Outcome, trace::TraceEvent};
use rayon::prelude::*;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Clone)]
struct Case {
    config: Config,
    seed: u64,
    seat: usize,
    opponent: Style,
}
#[derive(Default, Clone, Serialize, serde::Deserialize)]
struct Score {
    matches: usize,
    wins: usize,
    losses: usize,
    timeouts: usize,
    hp_difference: f64,
    seconds: f64,
    hits: [u64; 2],
    crits: [u64; 2],
    damage: [f64; 2],
}
impl Score {
    fn add(&mut self, b: &Self) {
        self.matches += b.matches;
        self.wins += b.wins;
        self.losses += b.losses;
        self.timeouts += b.timeouts;
        self.hp_difference += b.hp_difference;
        self.seconds += b.seconds;
        for i in 0..2 {
            self.hits[i] += b.hits[i];
            self.crits[i] += b.crits[i];
            self.damage[i] += b.damage[i];
        }
    }
    fn rank(&self) -> f64 {
        (self.wins as f64 - self.losses as f64) * 1000.0 + self.hp_difference
    }
}
fn fight(p: CriticalParams, c: &Case, trace: bool) -> Score {
    let mut config = c.config.clone();
    config.trace_capacity = if trace { 1024 } else { 0 };
    let mut d = Duel::new(config, c.seed).unwrap();
    let mut bots = std::array::from_fn::<_, 2, _>(|i| {
        if i == c.seat {
            Bot::with_critical_params(p)
        } else {
            Bot::new(c.opponent)
        }
    });
    let mut s = Score {
        matches: 1,
        ..Score::default()
    };
    while !d.is_finished() {
        let obs = d.observations();
        d.step(std::array::from_fn(|i| bots[i].act(&obs[i])))
            .unwrap();
        if trace {
            for r in d.drain_trace() {
                if let TraceEvent::Attack { attacker, result } = r.event {
                    if result.damage.accepted {
                        let i = attacker ^ c.seat;
                        s.hits[i] += 1;
                        s.crits[i] += u64::from(result.critical);
                        s.damage[i] += f64::from(result.damage.health_lost);
                    }
                }
            }
        }
    }
    assert_eq!(d.dropped_trace_records(), 0);
    match d.outcome() {
        Outcome::Victory { winner } if winner == c.seat => s.wins = 1,
        Outcome::Victory { .. } => s.losses = 1,
        Outcome::Truncated => s.timeouts = 1,
        _ => unreachable!(),
    }
    let snap = d.snapshot();
    s.hp_difference = f64::from(snap.server[c.seat].health - snap.server[1 - c.seat].health);
    s.seconds = snap.time_ns as f64 / 1e9;
    s
}
fn score(p: CriticalParams, cases: &[Case], trace: bool) -> Score {
    let mut s = Score::default();
    for c in cases {
        s.add(&fight(p, c, trace));
    }
    s
}
fn cases(variants: usize, held_out: bool, opponents: &[Style]) -> Vec<Case> {
    let mut cases = vec![];
    for &opponent in opponents {
        for v in 0..variants {
            let mut config = Config::default();
            let seed = if held_out {
                10_001 + v as u64
            } else {
                42 + v as u64
            };
            if v > 0 {
                let distance = if held_out {
                    [3.5, 5., 9., 14., 25., 32., 6.5][v % 7]
                } else {
                    [4., 6., 8., 12., 16., 20., 28.][v % 7]
                };
                config.spawns[0].position.z = -distance / 2.;
                config.spawns[1].position.z = distance / 2.;
                config.spawns[1].position.x = if held_out {
                    [0., 0.7, -1.3, 2.][v % 4]
                } else {
                    [0., 0.5, -1.][v % 3]
                };
                for (i, c) in config.clients.iter_mut().enumerate() {
                    let ms = if held_out {
                        [0, 7, 20, 40, 65, 90][(v + i) % 6]
                    } else {
                        [0, 10, 25, 35, 50][(v / 7 + i) % 5]
                    };
                    c.upstream_ns = ms * 1_000_000;
                    c.downstream_ns = ms * 1_000_000;
                    if held_out {
                        c.fps = [30, 60, 144, 240][(v + i) % 4];
                        c.tick_phase_ns = ((v * 7 + i * 13) as u64 % 50) * 1_000_000;
                        c.frame_phase_ns = ((v * 113 + i * 137) as u64 * 1000)
                            % (1_000_000_000 / u64::from(c.fps));
                    } else {
                        c.fps = [60, 120, 240][(v + i) % 3];
                        c.tick_phase_ns = ((v * 3 + i * 17) as u64 % 50) * 1_000_000;
                    }
                }
                if held_out {
                    config.server_phase_ns = (v as u64 * 11 % 50) * 1_000_000;
                }
            }
            for seat in 0..2 {
                for first in 0..2 {
                    config.first_client = Some(first);
                    cases.push(Case {
                        config: config.clone(),
                        seed,
                        seat,
                        opponent,
                    });
                }
            }
        }
    }
    cases
}
#[derive(Clone, Serialize, serde::Deserialize)]
struct Candidate {
    params: CriticalParams,
    score: Score,
}
fn random(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
fn pick<T: Copy>(state: &mut u64, values: &[T]) -> T {
    values[random(state) as usize % values.len()]
}
fn mutate(p: &mut CriticalParams, rng: &mut u64) {
    match random(rng) % 9 {
        0 => p.jump_distance = pick(rng, &[2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 9., 12.]),
        1 => {
            p.jump_charge = pick(
                rng,
                &[0., 0.15, 0.35, 0.55, 0.65, 0.75, 0.8, 0.85, 0.9, 0.95],
            )
        }
        2 => p.coast_distance = pick(rng, &[2.5, 3., 3.5, 4., 4.5, 5., 6., 8.]),
        3 => p.coast_velocity = pick(rng, &[-0.2, -0.1, 0., 0.05, 0.1, 0.2, 0.25, 0.32, 0.4, 0.5]),
        4 => p.forward_distance = pick(rng, &[0., 0.7, 1.1, 1.4, 1.8, 2.2, 2.6]),
        5 => p.backward_distance = pick(rng, &[0., 0.5, 0.8, 1., 1.3, 1.7]),
        6 => p.attack_charge = pick(rng, &[0.9, 0.93, 0.96, 0.99]),
        7 => p.fallback_ticks = pick(rng, &[10, 12, 13, 15, 18, 20, 22, 25, 30, 60]),
        _ => p.release_ticks = pick(rng, &[0, 1, 2, 3, 4]),
    }
}
fn save(name: &str, value: &impl Serialize) {
    let root = Path::new("build/benchmarks/critical-plus");
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join(name),
        serde_json::to_string_pretty(value).unwrap(),
    )
    .unwrap();
}
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "tune".into());
    if mode == "refine" {
        let previous: Vec<(f64, Candidate, Score)> = serde_json::from_str(
            &fs::read_to_string("build/benchmarks/critical-plus/selection.json").unwrap(),
        )
        .unwrap();
        let mut rng = 20261107;
        let mut params: Vec<_> = previous.iter().map(|c| c.1.params).collect();
        params.push(CriticalParams::ORIGINAL);
        for _ in 0..1024 {
            let mut p = previous[random(&mut rng) as usize % previous.len()]
                .1
                .params;
            for _ in 0..1 + random(&mut rng) % 3 {
                mutate(&mut p, &mut rng);
            }
            if !params.contains(&p) {
                params.push(p);
            }
        }
        let opponents = [
            Style::Critical,
            Style::Baseline,
            Style::Pressure,
            Style::Orbit,
            Style::Counter,
        ];
        let training = cases(8, false, &opponents);
        let mut candidates: Vec<_> = params
            .into_par_iter()
            .map(|params| Candidate {
                params,
                score: score(params, &training, false),
            })
            .collect();
        candidates.sort_by(|a, b| b.score.rank().total_cmp(&a.score.rank()));
        save("refinement-screening.json", &candidates);
        eprintln!("Refinement screened {} unique candidates", candidates.len());
        candidates.truncate(96);
        let full = cases(36, false, &opponents);
        candidates
            .par_iter_mut()
            .for_each(|c| c.score = score(c.params, &full, false));
        candidates.sort_by(|a, b| b.score.rank().total_cmp(&a.score.rank()));
        save("refinement.json", &candidates);
        println!("{}", serde_json::to_string_pretty(&candidates[0]).unwrap());
        return;
    }
    if mode == "evaluate" {
        let p = CriticalParams::PLUS;
        let default = score(p, &cases(1, false, &[Style::Critical]), true);
        let held = cases(128, true, &[Style::Critical]);
        let detail: Vec<_> = held.par_iter().map(|c| serde_json::json!({"seed":c.seed,"plus_seat":c.seat,"opponent":c.opponent.name(),"config":c.config,"score":fight(p,c,true)})).collect();
        let mut total = Score::default();
        for v in &detail {
            total.add(&serde_json::from_value::<Score>(v["score"].clone()).unwrap());
        }
        let league: Vec<_> = [Style::Baseline, Style::Pressure, Style::Orbit, Style::Counter].into_par_iter().map(|op| {
            let c = cases(32, true, &[op]);
            serde_json::json!({"opponent":op.name(),"original":score(CriticalParams::ORIGINAL,&c,true),"plus":score(p,&c,true)})
        }).collect();
        let report = serde_json::json!({"params":p,"default_vs_original":default,"held_out_vs_original":total,"league":league,"details":detail});
        save("evaluation.json", &report);
        println!("{}",serde_json::to_string_pretty(&serde_json::json!({"default":report["default_vs_original"],"held_out":report["held_out_vs_original"],"league":report["league"]})).unwrap());
        return;
    }
    let mut rng = 20261106;
    let mut params = vec![CriticalParams::ORIGINAL];
    for _ in 0..4095 {
        params.push(CriticalParams {
            jump_distance: pick(&mut rng, &[2.5, 3.5, 4.5, 5.5, 6.5, 7.5, 9., 12.]),
            jump_charge: pick(&mut rng, &[0., 0.15, 0.35, 0.55, 0.75, 0.9]),
            coast_distance: pick(&mut rng, &[2.5, 3., 3.5, 4., 4.5, 5., 6., 8.]),
            coast_velocity: pick(&mut rng, &[-0.2, 0., 0.1, 0.2, 0.25, 0.32, 0.4, 0.5]),
            forward_distance: pick(&mut rng, &[0., 0.7, 1.4, 1.8, 2.2, 2.6]),
            backward_distance: pick(&mut rng, &[0., 0.5, 1., 1.3, 1.7]),
            attack_charge: pick(&mut rng, &[0.9, 0.93, 0.96, 0.99]),
            fallback_ticks: pick(&mut rng, &[10, 12, 13, 15, 18, 22, 30, 60]),
            release_ticks: pick(&mut rng, &[0, 1, 2, 3, 4]),
        });
    }
    let mut candidates: Vec<_> = params
        .into_par_iter()
        .map(|params| Candidate {
            params,
            score: score(params, &cases(1, false, &[Style::Critical]), false),
        })
        .collect();
    candidates.sort_by(|a, b| b.score.rank().total_cmp(&a.score.rank()));
    save("screening.json", &candidates);
    eprintln!(
        "Screened {} candidates; best: {}",
        candidates.len(),
        serde_json::to_string(&candidates[0]).unwrap()
    );
    candidates.truncate(256);
    let training = cases(36, false, &[Style::Critical]);
    candidates
        .par_iter_mut()
        .for_each(|c| c.score = score(c.params, &training, false));
    candidates.sort_by(|a, b| b.score.rank().total_cmp(&a.score.rank()));
    save("finalists.json", &candidates);
    candidates.truncate(32);
    let league = cases(
        16,
        false,
        &[
            Style::Baseline,
            Style::Pressure,
            Style::Orbit,
            Style::Counter,
        ],
    );
    // Require original-Critical wins first; league score breaks ties in that win count.
    let mut finalists: Vec<_> = candidates
        .into_par_iter()
        .map(|c| {
            let other = score(c.params, &league, false);
            let rank = c.score.wins as f64 * 1e6 - c.score.losses as f64 * 1e6
                + other.rank()
                + c.score.hp_difference;
            (rank, c, other)
        })
        .collect();
    finalists.sort_by(|a, b| b.0.total_cmp(&a.0));
    save("selection.json", &finalists);
    println!("{}", serde_json::to_string_pretty(&finalists[0]).unwrap());
}
