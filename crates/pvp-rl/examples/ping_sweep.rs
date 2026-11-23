//! Fixed-policy, paired latency experiments. Ping means upstream + downstream RTT.
use pvp_policy::policy::{Bot, Style};
use pvp_sim::{Config, Duel, Outcome, rng::derive_seed, trace::TraceEvent};
use rayon::prelude::*;
use serde::Serialize;
use std::{fs, path::Path};

const SEED: u64 = 2026110603;
#[derive(Clone, Serialize)]
struct Condition {
    family: &'static str,
    style: &'static str,
    // Role 0 is the player whose win rate is reported.
    up_ms: [f64; 2],
    down_ms: [f64; 2],
    diagnostics: bool,
}
#[derive(Serialize)]
struct Episode {
    variant: usize,
    seat: usize,
    first: usize,
    winner_role: Option<usize>,
    hp: [f32; 2],
    seconds: f64,
    hits: [u64; 2],
    crits: [u64; 2],
    rejected_reach: [u64; 2],
    immune: [u64; 2],
    damage: [f64; 2],
}
#[derive(Serialize)]
struct ResultGroup {
    condition: Condition,
    wins: usize,
    losses: usize,
    timeouts: usize,
    mean_hp_difference: f64,
    mean_seconds: f64,
    episodes: Vec<Episode>,
}
fn base(variant: usize) -> Config {
    let mut c = Config::default();
    let mut index = 0;
    let mut next = || {
        index += 1;
        derive_seed(SEED + variant as u64, index)
    };
    let unit = |v: u64| (v >> 11) as f64 / (1u64 << 53) as f64;
    let gap = 3.5 + 28.5 * unit(next());
    c.spawns[0].position.z = -gap / 2.;
    c.spawns[1].position.z = gap / 2.;
    c.spawns[1].position.x = -2. + 4. * unit(next());
    c.server_phase_ns = next() % 50_000_000;
    for t in &mut c.clients {
        t.fps = 60;
        t.tick_phase_ns = next() % 50_000_000;
        t.frame_phase_ns = next() % (1_000_000_000 / 60);
    }
    c
}
fn fight(k: &Condition, variant: usize, seat: usize, first: usize) -> Episode {
    let mut c = base(variant);
    c.first_client = Some(first);
    c.trace_capacity = if k.diagnostics { 4096 } else { 0 };
    for role in 0..2 {
        c.clients[role ^ seat].upstream_ns = (k.up_ms[role] * 1e6).round() as u64;
        c.clients[role ^ seat].downstream_ns = (k.down_ms[role] * 1e6).round() as u64;
    }
    let style = if k.style == "critical-plus" {
        Style::CriticalPlus
    } else {
        Style::DuelRl
    };
    let mut bots = [Bot::new(style), Bot::new(style)];
    let mut d = Duel::new(c, derive_seed(SEED, variant as u64)).unwrap();
    let mut e = Episode {
        variant,
        seat,
        first,
        winner_role: None,
        hp: [0.; 2],
        seconds: 0.,
        hits: [0; 2],
        crits: [0; 2],
        rejected_reach: [0; 2],
        immune: [0; 2],
        damage: [0.; 2],
    };
    while !d.is_finished() {
        let o = d.observations();
        d.step(std::array::from_fn(|i| bots[i].act(&o[i]))).unwrap();
        if k.diagnostics {
            for r in d.drain_trace() {
                match r.event {
                    TraceEvent::Attack { attacker, result } => {
                        let role = attacker ^ seat;
                        if result.damage.accepted {
                            e.hits[role] += 1;
                            e.crits[role] += u64::from(result.critical);
                            e.damage[role] += f64::from(result.damage.health_lost);
                        } else {
                            e.immune[role] += 1;
                        }
                    }
                    TraceEvent::AttackRejected { attacker, .. } => {
                        e.rejected_reach[attacker ^ seat] += 1
                    }
                    _ => {}
                }
            }
        }
    }
    assert_eq!(d.dropped_trace_records(), 0);
    e.winner_role = match d.outcome() {
        Outcome::Victory { winner } => Some(winner ^ seat),
        Outcome::Truncated => None,
        Outcome::Ongoing => unreachable!(),
    };
    let s = d.snapshot();
    e.hp = [s.server[seat].health, s.server[1 ^ seat].health];
    e.seconds = s.time_ns as f64 / 1e9;
    e
}
fn main() {
    let variants: usize = std::env::args()
        .nth(1)
        .map(|v| v.parse().unwrap())
        .unwrap_or(64);
    assert!(variants > 0);
    let root = if variants == 64 {
        "build/benchmarks/ping-sweep".to_string()
    } else {
        format!("build/benchmarks/ping-sweep-{variants}")
    };
    fs::create_dir_all(&root).unwrap();
    fs::write(
        Path::new(&root).join("contexts.json"),
        serde_json::to_vec_pretty(
            &(0..variants)
                .map(|v| (v, derive_seed(SEED, v as u64), base(v)))
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
    let mut conditions = vec![];
    for style in ["critical-plus", "duel-rl"] {
        for a in (10..=150).step_by(10) {
            for b in (a..=150).step_by(10) {
                conditions.push(Condition {
                    family: "rtt-grid",
                    style,
                    up_ms: [a as f64 / 2., b as f64 / 2.],
                    down_ms: [a as f64 / 2., b as f64 / 2.],
                    diagnostics: false,
                });
            }
        }
        for ping in (10..=150).step_by(10) {
            for fraction in [0.1, 0.25, 0.5, 0.75, 0.9] {
                let p = ping as f64;
                conditions.push(Condition {
                    family: "fixed-rtt-split",
                    style,
                    up_ms: [p * fraction, p / 2.],
                    down_ms: [p * (1. - fraction), p / 2.],
                    diagnostics: false,
                });
            }
        }
        for up in (5..=95).step_by(5) {
            conditions.push(Condition {
                family: "100ms-fine-offset",
                style,
                up_ms: [up as f64, 50.],
                down_ms: [100. - up as f64, 50.],
                diagnostics: false,
            });
        }
        for (up_ms, down_ms) in [
            ([5., 75.], [5., 75.]),
            ([25., 50.], [25., 50.]),
            ([5., 50.], [95., 50.]),
            ([50., 50.], [50., 50.]),
            ([95., 50.], [5., 50.]),
        ] {
            conditions.push(Condition {
                family: "diagnostics",
                style,
                up_ms,
                down_ms,
                diagnostics: true,
            });
        }
    }
    let total = conditions.len();
    eprintln!(
        "{total} groups, {} matches, {variants} shared contexts",
        total * variants * 4
    );
    let mut results = vec![];
    for (i, condition) in conditions.into_iter().enumerate() {
        let episodes: Vec<_> = (0..variants * 4)
            .into_par_iter()
            .map(|j| fight(&condition, j / 4, (j / 2) % 2, j % 2))
            .collect();
        let wins = episodes.iter().filter(|e| e.winner_role == Some(0)).count();
        let losses = episodes.iter().filter(|e| e.winner_role == Some(1)).count();
        let n = episodes.len() as f64;
        results.push(ResultGroup {
            condition,
            wins,
            losses,
            timeouts: episodes.len() - wins - losses,
            mean_hp_difference: episodes
                .iter()
                .map(|e| f64::from(e.hp[0] - e.hp[1]))
                .sum::<f64>()
                / n,
            mean_seconds: episodes.iter().map(|e| e.seconds).sum::<f64>() / n,
            episodes,
        });
        if (i + 1) % 20 == 0 || i + 1 == total {
            eprintln!("completed {}/{total}", i + 1);
        }
    }
    fs::write(
        Path::new(&root).join("results.json"),
        serde_json::to_vec(&results).unwrap(),
    )
    .unwrap();
    eprintln!("saved {root}/results.json");
}
