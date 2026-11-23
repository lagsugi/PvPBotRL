use pvp_sim::sim::server_replay::{Capture, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("expected measured schedule JSON")?;
    let capture: Capture =
        serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path)?))?;
    let results = run(capture)?;
    serde_json::to_writer(std::io::stdout().lock(), &results)?;
    Ok(())
}
