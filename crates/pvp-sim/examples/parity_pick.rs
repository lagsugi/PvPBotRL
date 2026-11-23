use pvp_sim::{observation::pick, state::Body};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    target: Body,
    viewer: Body,
    floor_y: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(std::io::stdin().lock())?;
    let result: Vec<_> = cases
        .iter()
        .map(|c| pick(c.viewer, c.target, c.floor_y))
        .collect();
    serde_json::to_writer(std::io::stdout().lock(), &result)?;
    Ok(())
}
