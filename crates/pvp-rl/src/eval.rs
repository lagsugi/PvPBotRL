use pvp_policy::recurrent::Model;
use pvp_rl::{Episode, G6, MatchResult, NETWORKS, RIVALS, SNAPSHOT_START};
use pvp_sim::{rng::derive_seed, trace::Replay};
use rayon::prelude::*;
use serde::Serialize;
use std::{error::Error, io::Write, path::Path, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Serialize)]
struct Trial {
    seed: u64,
    new: MatchResult,
    g6: Option<MatchResult>,
    anchor: Option<MatchResult>,
}

#[derive(Serialize)]
struct Cell {
    rival: &'static str,
    network: pvp_rl::league::Network,
    trials: Vec<Trial>,
}

#[derive(Serialize)]
struct SeatPair {
    seed: u64,
    seats: [usize; 2],
    candidate: [MatchResult; 2],
    champion: Option<[MatchResult; 2]>,
}

fn paired(args: &[String], fixed_equipment: bool) -> Result<()> {
    let model = Model::load(Path::new(&args[1]))?;
    let champion = Arc::new(Model::load(Path::new(&args[2]))?);
    let direct = args[3] == "direct";
    let (rival, snapshots) = if direct {
        (SNAPSHOT_START, vec![Some(Arc::clone(&champion))])
    } else if let Ok(id) = args[3].parse::<usize>() {
        if id > G6 {
            return Err("invalid built-in opponent".into());
        }
        (id, vec![])
    } else {
        (
            SNAPSHOT_START,
            vec![Some(Arc::new(Model::load(Path::new(&args[3]))?))],
        )
    };
    let net = args[4].parse()?;
    let base = args[5].parse()?;
    let count: usize = args[6].parse()?;
    let max_steps = args[7].parse()?;
    if count == 0 {
        return Err("pair count must be positive".into());
    }
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    // Bound memory and preserve deterministic order irrespective of thread count.
    for start in (0..count).step_by(256) {
        let rows = (start..count.min(start + 256))
            .into_par_iter()
            .map(|i| {
                let seed = derive_seed(base, i as u64);
                let fight = |policy, seat| {
                    Episode::seated_with_equipment(
                        seed,
                        net,
                        rival,
                        max_steps,
                        &snapshots,
                        seat,
                        fixed_equipment,
                    )?
                    .run(Some(policy), |_| {})
                };
                Ok(SeatPair {
                    seed,
                    seats: [0, 1],
                    candidate: [fight(&model, 0)?, fight(&model, 1)?],
                    champion: if direct {
                        None
                    } else {
                        Some([fight(champion.as_ref(), 0)?, fight(champion.as_ref(), 1)?])
                    },
                })
            })
            .collect::<std::result::Result<Vec<_>, pvp_sim::SimError>>()?;
        for row in rows {
            serde_json::to_writer(&mut out, &row)?;
            writeln!(out)?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let fixed_equipment = args.last().is_some_and(|arg| arg == "--fixed-equipment");
    if fixed_equipment {
        args.pop();
    }
    match args.first().map(String::as_str) {
        Some("paired") if args.len() == 8 => paired(&args, fixed_equipment)?,
        Some("evaluate") if (7..=8).contains(&args.len()) => {
            let model = Model::load(Path::new(&args[1]))?;
            let final_eval = match args[2].as_str() {
                "selection" => false,
                "final" => true,
                _ => return Err("expected selection or final".into()),
            };
            let seed = args[3].parse()?;
            let direct: usize = args[4].parse()?;
            let common: usize = args[5].parse()?;
            let max_steps = args[6].parse()?;
            if direct == 0 || common == 0 {
                return Err("evaluation counts must be positive".into());
            }
            let anchor = args.get(7).map(|p| Model::load(Path::new(p))).transpose()?;
            let cells: Vec<_> = std::iter::once(G6)
                .chain(0..if final_eval { G6 } else { G6 - 2 })
                .flat_map(|rival| (0..NETWORKS.len()).map(move |net| (rival, net)))
                .collect();
            let results: Vec<Cell> = cells.into_par_iter().map(|(rival, net)| {
                let split = derive_seed(seed, (rival * NETWORKS.len() + net) as u64);
                let n = if rival == G6 { direct } else { common };
                let trials = (0..n).into_par_iter().map(|i| {
                    let seed = derive_seed(split, i as u64);
                    let fight = |policy| Episode::with_equipment(seed, net, rival, max_steps, &[], fixed_equipment)?.run(policy, |_| {});
                    Ok(Trial {
                        seed,
                        new: fight(Some(&model))?,
                        g6: if final_eval && rival != G6 { Some(fight(None)?) } else { None },
                        anchor: if final_eval && rival != G6 && net == 0 {
                            anchor.as_ref().map(|p| fight(Some(p))).transpose()?
                        } else { None },
                    })
                }).collect::<std::result::Result<Vec<_>, pvp_sim::SimError>>()?;
                Ok(Cell { rival: RIVALS[rival], network: NETWORKS[net], trials })
            }).collect::<std::result::Result<_, pvp_sim::SimError>>()?;
            serde_json::to_writer(std::io::stdout().lock(), &results)?;
        }
        Some("duel") if (6..=7).contains(&args.len()) => {
            let model = Model::load(Path::new(&args[1]))?;
            let network = args[2].parse()?;
            let rival = args[3].parse()?;
            let seed = args[4].parse()?;
            let max_steps = args.get(6).map(|s| s.parse()).transpose()?.unwrap_or(6000);
            let mut episode = Episode::with_equipment(seed, network, rival, max_steps, &[], fixed_equipment)?;
            let mut replay = Replay::new(episode.duel.config().clone(), episode.duel.snapshot().seed);
            let result = episode.run(Some(&model), |actions| replay.record(actions))?;
            let replayed = replay.run()?;
            if replayed.outcome() != episode.duel.outcome()
                || replayed.server_health() != episode.duel.server_health()
                || replayed.decision_step() != episode.duel.decision_step() {
                return Err("recorded replay does not reproduce the duel".into());
            }
            serde_json::to_writer(std::io::BufWriter::new(std::fs::File::create(&args[5])?), &replay)?;
            serde_json::to_writer(std::io::stdout().lock(), &result)?;
        }
        _ => return Err("pvp-rl-eval evaluate MODEL selection|final SEED DIRECT COMMON MAX_STEPS [ANCHOR] | duel MODEL NETWORK_ID RIVAL_ID SEED REPLAY [MAX_STEPS] | paired MODEL CHAMPION direct|RIVAL_ID|RIVAL_MODEL NETWORK_ID SEED PAIRS MAX_STEPS".into()),
    }
    Ok(())
}
