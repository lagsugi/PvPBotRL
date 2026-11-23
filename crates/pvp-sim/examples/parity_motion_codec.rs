//! Compare Rust LpVec3 bytes with captured Paper motion packet encoding.
use pvp_sim::{math::Vec3, network};
use serde_json::{Value, json};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Expected server.json")?;
    let capture: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let report = compare(&capture)?;
    println!("{report}");
    if report["passed"] != true {
        std::process::exit(1);
    }
    Ok(())
}

fn compare(capture: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    if capture["schema"] != 1 || capture["status"] != "complete" {
        return Err("Incomplete or unsupported capture".into());
    }
    let events = capture["motion_transport"]
        .as_array()
        .ok_or("Missing transport events")?;
    let mut results = Vec::new();
    for event in events.iter().filter(|e| e["event"] == "motion_send") {
        let [x, y, z]: [f64; 3] = serde_json::from_value(event["raw_velocity"].clone())?;
        if ![x, y, z].into_iter().all(f64::is_finite) {
            return Err("Non-finite captured velocity".into());
        }
        let encoded = network::encode_motion(Vec3::new(x, y, z));
        let hex: String = encoded.iter().map(|b| format!("{b:02x}")).collect();
        let (decoded, used) = network::decode_motion(&encoded)?;
        let expected: [f64; 3] = serde_json::from_value(event["decoded_velocity"].clone())?;
        let passed = hex
            == event["lpvec3_hex"]
                .as_str()
                .ok_or("Missing encoded bytes")?
            && decoded.components() == expected
            && used == encoded.len();
        results.push(
            json!({"sequence":event["sequence"], "passed":passed, "lpvec3_hex":hex,
            "decoded_velocity":decoded.components()}),
        );
    }
    let passed = !results.is_empty() && results.iter().all(|r| r["passed"] == true);
    Ok(json!({"schema":1,"scope":"captured-motion-codec-only","passed":passed,"results":results}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/parity/combat/motion_transport_zero/bot.json"
        ))
        .unwrap()
    }
    #[test]
    fn captured_paper_packet_bytes_and_decoded_velocity_match() {
        assert_eq!(compare(&fixture()).unwrap()["passed"], true);
    }
    #[test]
    fn changed_wire_bytes_cannot_pass_even_with_unchanged_decoded_values() {
        let mut capture = fixture();
        capture["motion_transport"][2]["lpvec3_hex"] = json!("f9ffb3315c58");
        assert_eq!(compare(&capture).unwrap()["passed"], false);
    }
}
