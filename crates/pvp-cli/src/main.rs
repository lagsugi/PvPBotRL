use pvp_policy::{policy, recurrent::Model, run_matchup_with_model};
use pvp_sim::{Batch, Config, Duel, Outcome, sim::Snapshot, trace::Replay};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const HELP: &str = "pvp-sim <duel|batch|replay> [options]\n\n  duel   [--bot-a NAME] [--bot-b NAME] [--config FILE] [--seed N] [--max-steps N] [--record FILE] [--trace FILE]\n  batch  [--bot-a NAME] [--bot-b NAME] [--config FILE] [--seed N] [--max-steps N] [--matches N] [--threads N]\n  replay --input FILE [--trace FILE]\n\nPing options (duel/batch): --ping-profile gamma|fixed, --ping-a MS, --ping-b MS, --ping-asymmetry 0..0.5.\nGamma: k=2 theta=35ms conditional on 5..150ms; sampled once per match. Default asymmetry half-width 0.1.\nBatch: --network-output FILE saves each lane seed and sampled timing.\n\nBots: baseline, pressure, orbit, counter, critical, critical-plus (Critical+), duel-rl, league-rl (G4), league-g5, league-g6, recurrent-ppo, recurrent-g2, recurrent-g2-plus.\nUse --recurrent-model FILE to override the bundled Recurrent-PPO weights.\nAll simulation time is virtual. No Minecraft process is launched.\nOutputs are JSON. Trace is NDJSON. --record stores replayable actions.\n";
fn output(path: &str) -> Result<BufWriter<File>> {
    let file = open_output(path)?;
    file.set_len(0)?;
    Ok(BufWriter::new(file))
}
fn open_output(path: &str) -> Result<File> {
    let path = Path::new(path);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    Ok(File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?)
}
fn resolved_output_path(path: &Path) -> std::io::Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let (Some(parent), Some(name)) = (path.parent(), path.components().next_back()) else {
                return Err(error);
            };
            let mut resolved = resolved_output_path(parent)?;
            if name == std::path::Component::ParentDir {
                resolved.pop();
            } else {
                resolved.push(name);
            }
            Ok(resolved)
        }
        Err(error) => Err(error),
    }
}
fn file_collision(flag: &str) -> Box<dyn std::error::Error> {
    format!("--{flag} and --trace refer to the same file").into()
}
fn ensure_distinct(trace: &File, other: &File, flag: &str) -> Result<()> {
    // Check the opened files so hard links and filesystem aliases are detected.
    if same_file::Handle::from_file(trace.try_clone()?)?
        == same_file::Handle::from_file(other.try_clone()?)?
    {
        return Err(file_collision(flag));
    }
    Ok(())
}
fn summary(s: &Snapshot) -> serde_json::Value {
    json!({"seed":s.seed,"outcome":s.outcome,"decision_steps":s.decision_step,"virtual_time_ns":s.time_ns,"server_health":s.server.map(|p|p.health),"positions":s.server.map(|p|p.body.position)})
}
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };
    if ["help", "--help", "-h"].contains(&command.as_str()) {
        print!("{HELP}");
        return Ok(());
    }
    let allowed: &[&str] = match command.as_str() {
        "duel" => &[
            "ping-profile",
            "ping-a",
            "ping-b",
            "ping-asymmetry",
            "config",
            "seed",
            "max-steps",
            "record",
            "trace",
            "bot-a",
            "bot-b",
            "recurrent-model",
        ],
        "batch" => &[
            "network-output",
            "ping-profile",
            "ping-a",
            "ping-b",
            "ping-asymmetry",
            "config",
            "seed",
            "max-steps",
            "matches",
            "threads",
            "bot-a",
            "bot-b",
            "recurrent-model",
        ],
        "replay" => &["input", "trace"],
        _ => return Err(format!("unknown command: {command}").into()),
    };
    let mut options = BTreeMap::new();
    while let Some(flag) = args.next() {
        let key = flag.strip_prefix("--").ok_or("expected --option")?;
        if !allowed.contains(&key) {
            return Err(format!("unknown option: {flag}").into());
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if options.insert(key.to_owned(), value).is_some() {
            return Err(format!("duplicate option: {flag}").into());
        }
    }
    let number = |key: &str, default: u64| -> Result<u64> {
        Ok(options
            .get(key)
            .map(|s| s.parse())
            .transpose()?
            .unwrap_or(default))
    };
    let mut config: Config = if let Some(path) = options.get("config") {
        serde_json::from_reader(BufReader::new(File::open(path)?))?
    } else {
        Config::default()
    };
    if options.contains_key("max-steps") {
        config.max_steps = number("max-steps", 6000)?;
    }
    let seed = number("seed", 1)?;
    if let Some(mode) = options.get("ping-profile") {
        config.ping_profile = match mode.as_str() {
            "gamma" => Some(pvp_sim::ping::PingProfile::default()),
            "fixed" => None,
            _ => return Err("--ping-profile must be gamma or fixed".into()),
        };
    }
    if let Some(value) = options.get("ping-asymmetry") {
        config
            .ping_profile
            .as_mut()
            .ok_or("--ping-asymmetry requires gamma profile")?
            .asymmetry = value.parse()?;
    }
    for (seat, key) in ["ping-a", "ping-b"].into_iter().enumerate() {
        if let Some(value) = options.get(key) {
            let ms: f64 = value.parse()?;
            if !ms.is_finite() || !(0.0..=10000.).contains(&ms) {
                return Err("Ping must be 0..10000 ms".into());
            }
            if let Some(p) = &mut config.ping_profile {
                p.fixed_ms[seat] = Some(ms);
            } else {
                let total = (ms * 1e6).round() as u64;
                config.clients[seat].upstream_ns = total / 2;
                config.clients[seat].downstream_ns = total - total / 2;
            }
        }
    }
    let style = |key| {
        options
            .get(key)
            .map(|s| s.parse::<policy::Style>())
            .transpose()
    };
    let styles = [
        style("bot-a")?.unwrap_or(policy::Style::Baseline),
        style("bot-b")?.unwrap_or(policy::Style::Baseline),
    ];
    let model = options
        .get("recurrent-model")
        .map(|path| Model::load(Path::new(path)))
        .transpose()?;
    if styles.iter().any(|s| s.is_recurrent()) {
        config.expose_opponent_health = true;
    }
    let trace_path = options.get("trace");
    let paired_flag = if command == "replay" {
        "input"
    } else {
        "record"
    };
    if let (Some(trace_path), Some(other)) = (trace_path, options.get(paired_flag)) {
        let resolve = |p: &str| resolved_output_path(&std::path::absolute(p)?);
        if resolve(trace_path)? == resolve(other)? {
            return Err(file_collision(paired_flag));
        }
    }
    if trace_path.is_some() {
        config.trace_capacity = config.trace_capacity.max(100_000);
    }
    match command.as_str() {
        "duel" => {
            let mut d = Duel::new(config.clone(), seed)?;
            let trace = trace_path.map(|path| open_output(path)).transpose()?;
            let record = if let (Some(trace), Some(path)) = (&trace, options.get("record")) {
                let record = open_output(path)?;
                ensure_distinct(trace, &record, "record")?;
                Some(record)
            } else {
                None
            };
            if let Some(file) = &trace {
                file.set_len(0)?;
            }
            let mut trace = trace.map(BufWriter::new);
            let recording = options.contains_key("record");
            let mut replay = Replay::new(d.config().clone(), seed);
            let mut bots = styles.map(policy::Bot::new);
            let start = Instant::now();
            while !d.is_finished() {
                let obs = d.observations();
                let actions = [
                    bots[0].act_in_duel(&d, 0, &obs[0], model.as_ref())?,
                    bots[1].act_in_duel(&d, 1, &obs[1], model.as_ref())?,
                ];
                if recording {
                    replay.record(actions);
                }
                d.step(actions)?;
                write_trace(&mut d, &mut trace)?;
            }
            if let Some(path) = options.get("record") {
                let mut f = if let Some(record) = record {
                    record.set_len(0)?;
                    BufWriter::new(record)
                } else {
                    output(path)?
                };
                serde_json::to_writer_pretty(&mut f, &replay)?;
                f.flush()?;
            }
            let mut out = summary(&d.snapshot());
            out["clients"] = json!(d.config().clients);
            out["bots"] = json!(styles.map(|s| s.name()));
            out["elapsed_seconds"] = json!(start.elapsed().as_secs_f64());
            out["dropped_trace_records"] = json!(d.dropped_trace_records());
            println!("{}", serde_json::to_string_pretty(&out)?);
            if let Some(mut trace) = trace {
                trace.flush()?;
            }
        }
        "replay" => {
            let path = options.get("input").ok_or("replay requires --input FILE")?;
            let input = File::open(path)?;
            let replay: Replay = serde_json::from_reader(BufReader::new(&input))?;
            if replay.version != 1 {
                return Err("unsupported replay version".into());
            }
            let mut config = replay.config;
            if trace_path.is_some() {
                config.trace_capacity = config.trace_capacity.max(100_000);
            }
            let mut d = Duel::new(config, replay.seed)?;
            let mut trace = if let Some(path) = trace_path {
                let trace = open_output(path)?;
                ensure_distinct(&trace, &input, "input")?;
                trace.set_len(0)?;
                Some(BufWriter::new(trace))
            } else {
                None
            };
            for a in replay.actions {
                d.step_recorded(a)?;
                write_trace(&mut d, &mut trace)?;
            }
            println!("{}", serde_json::to_string_pretty(&summary(&d.snapshot()))?);
            if let Some(mut trace) = trace {
                trace.flush()?;
            }
        }
        "batch" => {
            let count = usize::try_from(number("matches", 1000)?)?;
            let threads = usize::try_from(number(
                "threads",
                std::thread::available_parallelism().map_or(1, usize::from) as u64,
            )?)?;
            let mut batch = Batch::new(config, count, seed, threads)?;
            let network: Vec<_> = (0..if options.contains_key("network-output") {
                count
            } else {
                0
            })
                .map(|i| {
                    let d = batch.duel(i).expect("valid lane");
                    json!({"seed":d.snapshot().seed,"clients":d.config().clients})
                })
                .collect();
            let start = Instant::now();
            let results = run_matchup_with_model(&mut batch, styles, model.as_ref())?;
            let elapsed = start.elapsed().as_secs_f64();
            let steps: u64 = results.iter().map(|r| r.decision_step).sum();
            let wins: [usize; 2] = std::array::from_fn(|p| {
                results
                    .iter()
                    .filter(|r| r.outcome == Outcome::Victory { winner: p })
                    .count()
            });
            let ping_summary = json!({"network":network});
            if let Some(path) = options.get("network-output") {
                let mut f = output(path)?;
                serde_json::to_writer_pretty(&mut f, &ping_summary)?;
                f.flush()?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"bots":styles.map(|s| s.name()),"matches":count,"threads":threads,"seed":seed,"wins":wins,"truncated":results.iter().filter(|r|r.outcome==Outcome::Truncated).count(),"decision_steps":steps,"elapsed_seconds":elapsed,"matches_per_second":count as f64/elapsed,"decision_steps_per_second":steps as f64/elapsed,"virtual_seconds_per_wall_second":steps as f64*0.05/elapsed})
                )?
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}
fn write_trace(d: &mut Duel, writer: &mut Option<BufWriter<File>>) -> Result<()> {
    if let Some(f) = writer.as_mut() {
        for event in d.drain_trace() {
            serde_json::to_writer(&mut *f, &event)?;
            writeln!(f)?;
        }
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}
