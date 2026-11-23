use pvp_sim::{Action, Batch, Config, ObservationSchema};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config {
        max_steps: 300,
        ..Config::default()
    };
    let mut environments = Batch::new(config, 16, 42, 4)?;
    let schema = ObservationSchema::default();
    let mut total_reward = 0.0;
    for training_step in 0..400 {
        let observations = environments.observe();
        let _features = observations
            .iter()
            .map(|pair| {
                pair.iter()
                    .map(|o| schema.project(o))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Replace this with batched policy inference. Each seat sees only its
        // local observation; neither authoritative snapshots nor opponent HP.
        let actions = vec![[Action::default(); 2]; observations.len()];
        for (lane, result) in environments.step(&actions)?.into_iter().enumerate() {
            total_reward += result.rewards[0];
            if result.terminated || result.truncated {
                // Store the final transition before resetting. Bootstrap from
                // truncation according to the trainer, never from true death.
                environments.reset_at(lane, 1000 + training_step * 16 + lane as u64)?;
            }
        }
    }
    println!(
        "lanes={}, features={}, seat_zero_reward={total_reward}",
        environments.len(),
        schema.features.len()
    );
    Ok(())
}
