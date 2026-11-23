//! Opt-in replay of received packets and interpolation ticks for one remote entity.
use super::Duel;
use crate::{config::Config, math::Vec3, network::Downstream, state::Body};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Capture {
    pub schema: u32,
    pub initial: Body,
    pub position_base: Vec3,
    pub events: Vec<Event>,
}

#[derive(Deserialize)]
pub struct Event {
    pub time_ns: u64,
    /// Absence denotes a remote interpolation tick.
    pub packet: Option<Downstream>,
}

#[derive(Serialize)]
pub struct Sample {
    pub body: Body,
    pub position_base: Vec3,
    pub observed_displacement: Vec3,
    pub position_age_ns: Option<u64>,
    pub velocity_age_ns: Option<u64>,
    pub metadata_age_ns: Option<u64>,
}

pub fn run(capture: Capture) -> Result<Vec<Sample>, String> {
    if capture.schema != 1 || capture.events.is_empty() {
        return Err("Unsupported or empty replica capture".into());
    }
    let mut duel = Duel::new(Config::default(), 0).map_err(|e| e.to_string())?;
    let r = &mut duel.replicas[0];
    r.body = capture.initial;
    r.position_base = capture.position_base;
    r.target_position = r.body.position;
    r.target_yaw = r.body.yaw;
    r.target_pitch = r.body.pitch;
    r.target_velocity = r.body.velocity;
    let (mut position_seen, mut velocity_seen, mut metadata_seen) = (false, false, false);
    let mut samples = Vec::with_capacity(capture.events.len());
    for event in capture.events {
        if event.time_ns < duel.time_ns {
            return Err("Measured client clock moved backwards".into());
        }
        duel.time_ns = event.time_ns;
        if let Some(packet) = event.packet {
            match &packet {
                Downstream::Motion { entity: 1, .. } => velocity_seen = true,
                Downstream::RelativeMove { entity: 1, .. }
                | Downstream::PositionSync { entity: 1, .. } => {
                    position_seen = true;
                }
                Downstream::Metadata { entity: 1, .. } => metadata_seen = true,
                _ => return Err("Unsupported packet or entity in a replica capture".into()),
            }
            duel.apply_down(0, packet);
        } else {
            duel.replicas[0].tick();
        }
        let r = &duel.replicas[0];
        samples.push(Sample {
            body: r.body,
            position_base: r.position_base,
            observed_displacement: r.observed_displacement,
            position_age_ns: position_seen.then(|| event.time_ns - r.position_received_ns),
            velocity_age_ns: velocity_seen.then(|| event.time_ns - r.velocity_received_ns),
            metadata_age_ns: metadata_seen.then(|| event.time_ns - r.metadata_received_ns),
        });
    }
    Ok(samples)
}
