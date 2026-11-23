//! Replays measured client initial state through the SAME movement routine used by Duel.
//! This is an isolated movement check, not a network/combat equivalence claim.
use pvp_sim::{action::AppliedKeys, config::Spawn, math::Vec3, physics, state::LocalPlayer};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
};

#[derive(Deserialize)]
struct InputAction {
    #[serde(default)]
    keys: AppliedKeys,
    yaw: Option<f32>,
    pitch: Option<f32>,
    #[serde(default)]
    click: bool,
    #[serde(default)]
    attack_held: bool,
}

#[derive(Deserialize)]
struct ReceivedMovement {
    #[serde(flatten)]
    feedback: physics::MovementFeedback,
    velocity_impulse: Option<Vec3>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args()
        .nth(1)
        .ok_or("Expected captured scenario directory")?;
    let dir = Path::new(&directory);
    let scenario: Value = serde_json::from_str(&fs::read_to_string(dir.join("scenario.json"))?)?;
    let initial: Value = serde_json::from_str(&fs::read_to_string(dir.join("initial.json"))?)?;
    let header: Value = serde_json::from_str(
        fs::read_to_string(dir.join("client.ndjson"))?
            .lines()
            .next()
            .ok_or("No header")?,
    )?;
    if scenario["schema"] != 1 || header["schema"] != 1 {
        return Err("Unsupported schema".into());
    }
    let actions: Vec<InputAction> = serde_json::from_value(scenario["actions"].clone())?;
    let received_only = std::env::args_os()
        .nth(3)
        .is_some_and(|value| value == "--physics-after-receive");
    if actions.is_empty() || (!received_only && actions.iter().any(|a| a.click || a.attack_held)) {
        return Err("Movement-only runner requires nonempty actions without attacks".into());
    }
    let position: Vec3 = serde_json::from_value(initial["position"].clone())?;
    let floor = scenario["floor_y"].as_f64().ok_or("floor_y required")?;
    let friction = scenario["floor_friction"]
        .as_f64()
        .ok_or("floor_friction required")? as f32;
    let mut local = LocalPlayer::new(
        Spawn {
            position,
            yaw: initial["yaw"].as_f64().ok_or("yaw")? as f32,
            pitch: initial["pitch"].as_f64().ok_or("pitch")? as f32,
        },
        floor,
    );
    local.body.velocity = serde_json::from_value(initial["velocity"].clone())?;
    local.body.on_ground = initial["on_ground"].as_bool().ok_or("on_ground")?;
    local.body.sprinting = initial["sprinting"].as_bool().ok_or("sprinting")?;
    local.body.pose = serde_json::from_value(initial["pose"].clone())?;
    local.body.fall_distance = initial["fall_distance"].as_f64().ok_or("fall_distance")?;
    local.body.attack_ticks = serde_json::from_value(initial["attack_ticks"].clone())?;
    local.jump_delay = serde_json::from_value(initial["jump_delay"].clone())?;
    local.sprint_trigger = serde_json::from_value(initial["sprint_trigger"].clone())?;
    local.crouching = initial["crouching"].as_bool().ok_or("crouching")?;
    local.input = serde_json::from_value(initial["input"].clone())?;
    let feedback: Option<Vec<ReceivedMovement>> = std::env::args_os()
        .nth(2)
        .map(|path| -> Result<_, Box<dyn std::error::Error>> {
            Ok(serde_json::from_reader(fs::File::open(path)?)?)
        })
        .transpose()?;
    if received_only && feedback.is_none() {
        return Err("Expected measured pre-movement inputs during combat".into());
    }
    if feedback
        .as_ref()
        .is_some_and(|rows| rows.len() != actions.len())
    {
        return Err("Incomplete pre-movement feedback".into());
    }
    let output_name = if feedback.is_some() {
        "rust-with-feedback.ndjson"
    } else {
        "rust.ndjson"
    };
    let mut output = BufWriter::new(fs::File::create(dir.join(output_name))?);
    let mut head = header;
    head["engine"] = json!(if received_only {
        "rust-motion-after-receive-v1"
    } else if feedback.is_some() {
        "rust-measured-feedback-v1"
    } else {
        "rust-local-motion-v1"
    });
    writeln!(output, "{head}")?;
    for (i, action) in actions.iter().enumerate() {
        let received = feedback.as_ref().map(|rows| &rows[i]);
        if let Some(impulse) = received.and_then(|row| row.velocity_impulse) {
            if !received_only || !impulse.finite() {
                return Err("Expected a finite combat motion packet".into());
            }
            local.body.velocity = impulse;
        }
        if let Some(yaw) = action.yaw {
            local.body.yaw = yaw;
        }
        if let Some(pitch) = action.pitch {
            local.body.pitch = pitch;
        }
        physics::tick_local_with_feedback(
            &mut local,
            action.keys,
            floor,
            friction,
            7,
            received.map(|row| row.feedback),
        );
        let b = local.body;
        let row = json!({"schema":1,"event":"state","step":i,"phase":"post_movement",
            "actor":0,"channel":"local","logical_ns":i as u64 * 50_000_000,
            "state":{"position":b.position,"velocity":b.velocity,"yaw":b.yaw,"pitch":b.pitch,
                "on_ground":b.on_ground,"sprinting":b.sprinting,"pose":b.pose,
                "fall_distance":b.fall_distance,"jump_delay":local.jump_delay}});
        writeln!(output, "{row}")?;
    }
    writeln!(
        output,
        "{}",
        json!({"schema":1,"event":"end","status":"complete","samples":actions.len(),"dropped":0})
    )?;
    Ok(())
}
