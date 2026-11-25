//! Long, stateful cross-language inference evidence; not a live-observation test.
use pvp_policy::{
    policy::{Bot, Style},
    recurrent::{self, Controller, Model, WIDTH},
};
use pvp_sim::{Action, Config, Duel, math::Vec3};
use serde_json::json;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let output = args.next().ok_or("Expected output NDJSON")?;
    let models = match args.next() {
        Some(path) => vec![("training", PathBuf::from(path))],
        None => [
            "recurrent_ppo",
            "recurrent_g2",
            "recurrent_g2_plus",
            "g3_prototype",
        ]
        .map(|name| (name, PathBuf::from(format!("models/bundled/{name}.json"))))
        .to_vec(),
    };
    let mut writer = BufWriter::new(File::create(output)?);
    let mut frames = 0;
    let mut episodes = 0;
    writeln!(
        writer,
        "{}",
        json!({"schema":1,"event":"manifest","scope":"shared-observation-inference"})
    )?;
    for (name, path) in models {
        let model = Model::load(&path)?;
        for seed in 0..100 {
            for seat in 0..2 {
                let mut config = Config {
                    expose_opponent_health: true,
                    max_steps: 600,
                    ..Config::default()
                };
                let rtt = [0, 1, 20, 49, 50, 51, 99, 100, 101, 120, 150, 300][seed as usize % 12];
                for (p, timing) in config.clients.iter_mut().enumerate() {
                    timing.upstream_ns = rtt * 500_000;
                    timing.downstream_ns = rtt * 500_000;
                    timing.tick_phase_ns = (seed * 1_234_567 + p as u64 * 7_654_321) % 50_000_000;
                }
                config.spawns[0].position = Vec3::new(-0.35, 0., -1.7);
                config.spawns[1].position = Vec3::new(0.35, 0., 1.7);
                let mut duel = Duel::new(config, seed)?;
                let mut controller = Controller::default();
                let mut opponent = Bot::new(Style::LeagueG6);
                let mut previous = Action::default();
                let mut hidden = [0.; WIDTH];
                for step in 0..600 {
                    let observation = duel.observe(seat)?;
                    let local = duel.local_player(seat)?;
                    let features =
                        recurrent::observe(&duel, seat, &observation, previous, model.input)?;
                    let logits = model.logits(&features, &mut hidden);
                    let action = controller.act(&model, &duel, seat, &observation)?;
                    let row = json!({"schema":1,"event":"frame","model":name,"episode":episodes,"step":step,
                        "observation":observation,"recurrent":{
                            "opponent_health":observation.opponent_health.unwrap(),
                            "opponent_health_age_ns":observation.opponent_health_age_ns.unwrap(),
                            "health_age_ns":observation.health_age_ns,"jump_delay":local.jump_delay,
                            "miss_time":local.miss_time,"fall_distance":local.body.fall_distance,
                            "height_above_floor":local.body.position.y-duel.config().floor_y,
                            "remaining_fraction":duel.config().max_steps.saturating_sub(duel.decision_step()) as f32 / duel.config().max_steps as f32
                        },"features":&features.values[..model.input.size()],"logits":logits,"hidden":hidden.as_slice(),"action":action});
                    writeln!(writer, "{row}")?;
                    frames += 1;
                    previous = action;
                    let mut actions = [Action::default(); 2];
                    actions[seat] = action;
                    actions[1 - seat] = opponent.act(&duel.observe(1 - seat)?);
                    if duel.step(actions)?.terminated {
                        break;
                    }
                }
                episodes += 1;
            }
        }
    }
    writeln!(
        writer,
        "{}",
        json!({"schema":1,"event":"end","frames":frames,"episodes":episodes})
    )?;
    eprintln!("{frames} decisions in {episodes} episodes");
    Ok(())
}
