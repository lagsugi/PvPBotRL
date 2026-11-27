//! Diagnostic replay of the measured order of server handlers and world ticks.
use super::{Duel, TraceRecord};
use crate::{
    config::{Config, ServerRules},
    math::Vec3,
    network::Upstream,
    state::{Body, ServerPlayer},
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Capture {
    pub schema: u32,
    pub config: Config,
    pub seed: u64,
    pub initial: [ServerPlayer; 2],
    pub events: Vec<Event>,
    #[serde(default)]
    pub paper_moves: Option<[MoveEventState; 2]>,
    #[serde(default)]
    pub knockback_random: Option<Vec<f64>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct MoveEventState {
    pub just_teleported: bool,
    pub has_moved: bool,
    pub last_move_event_position: Vec3,
    pub last_move_event_rotation: [f32; 2],
}

impl MoveEventState {
    pub(crate) fn ignores_move(
        &mut self,
        body: &Body,
        target: Vec3,
        rotation: Option<[f32; 2]>,
    ) -> bool {
        let current_rotation = [body.yaw, body.pitch];
        if !self.has_moved {
            self.last_move_event_position = body.position;
            self.last_move_event_rotation = current_rotation;
            self.has_moved = true;
        }
        let rotation = rotation.unwrap_or(current_rotation);
        let [old_yaw, old_pitch] = self.last_move_event_rotation;
        let angular_step = (rotation[0] - old_yaw).abs() + (rotation[1] - old_pitch).abs();
        if (target - self.last_move_event_position).length_squared() <= 0.00390625
            && angular_step <= 10.0
        {
            return false;
        }
        // Paper compares the last Bukkit move event's `from` to the current pose.
        // An accepted micro-move can change the pose without updating that event.
        let different = body.position != self.last_move_event_position
            || current_rotation != self.last_move_event_rotation;
        self.last_move_event_position = target;
        self.last_move_event_rotation = rotation;
        if self.just_teleported && different {
            self.just_teleported = false;
            return true;
        }
        false
    }
}

#[derive(Deserialize)]
pub struct Event {
    pub time_ns: u64,
    #[serde(flatten)]
    pub phase: Phase,
}

#[derive(Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum Phase {
    Tick,
    Sample,
    Packet { player: usize, packet: Upstream },
}

#[derive(Serialize)]
pub struct ReplayedEvent {
    pub time_ns: u64,
    pub server: [ServerPlayer; 2],
    pub trace: Vec<TraceRecord>,
    pub paper_moves: Option<[MoveEventState; 2]>,
    pub attack_charge: [f32; 2],
}

/// Reuse Duel's authoritative handlers with supplied packet payloads. Local client
/// ticks, packet selection and transmission are outside this diagnostic replay.
pub fn run(capture: Capture) -> Result<Vec<ReplayedEvent>, String> {
    if capture.schema != 1 || capture.events.is_empty() {
        return Err("Unsupported or empty server replay".into());
    }
    let mut config = capture.config;
    if config.duel_driver == crate::config::DuelDriver::PaperCitizens {
        return Err("Server packet replay requires the client_server driver".into());
    }
    config.trace_capacity = 64;
    config.trace_states = false;
    let mut duel = Duel::new(config, capture.seed).map_err(|e| e.to_string())?;
    if let Some(values) = capture.knockback_random {
        if values.iter().any(|v| !(0.0..1.0).contains(v)) {
            return Err("Invalid recorded knockback draw".into());
        }
        duel.rng.use_measured(values);
    }
    if matches!(
        duel.config.server_rules,
        ServerRules::PaperFlatV1 | ServerRules::PaperFlatV2
    ) {
        duel.paper_moves = capture.paper_moves;
    }
    duel.server = capture.initial;
    duel.trackers = capture.initial.map(crate::state::Tracker::new);
    let mut output = Vec::with_capacity(capture.events.len());
    for event in capture.events {
        if event.time_ns < duel.time_ns {
            return Err("Measured server clock moved backwards".into());
        }
        duel.time_ns = event.time_ns;
        match event.phase {
            Phase::Tick => duel.server_tick(&mut |_| ()),
            Phase::Sample => (),
            Phase::Packet { player, packet } => {
                if player >= duel.server.len() {
                    return Err(format!("Invalid player index {player}"));
                }
                duel.apply_up(player, packet, &mut |_| ());
            }
        }
        // No clients consume these packets. Preserve the trace, and bound the wire.
        duel.wire.down.iter_mut().for_each(|queue| queue.clear());
        if duel.dropped_trace_records() != 0 {
            return Err("Server replay trace overflow".into());
        }
        output.push(ReplayedEvent {
            time_ns: event.time_ns,
            server: duel.server,
            trace: duel.drain_trace(),
            paper_moves: duel.paper_moves,
            attack_charge: duel
                .server
                .map(|player| crate::combat::charge(player.body.attack_ticks, player.attack_speed)),
        });
    }
    if !duel.rng.measured_consumed() {
        return Err("Rust did not consume the exact measured knockback draw sequence".into());
    }
    Ok(output)
}
