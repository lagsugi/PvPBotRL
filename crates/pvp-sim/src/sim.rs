use crate::{
    action::{Action, AppliedAction, Look},
    combat,
    config::{Config, SECOND_NS, ServerRules, SimError, TICK_NS},
    math::{Vec3, aim_angles, cos, pack_angle, radians, sin, unpack_angle, wrap_degrees},
    network::{Downstream, Upstream, Wire, decode_delta, encode_delta, quantize_motion},
    observation::{Observation, Pick, pick},
    physics,
    rng::JavaRandom,
    state::{LocalPlayer, Pose, Replica, ServerPlayer, Tracker},
    trace::{Trace, TraceEvent, TraceRecord},
};
use serde::{Deserialize, Serialize};

pub mod replica_replay;
pub mod server_replay;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ongoing,
    Victory { winner: usize },
    Truncated,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    pub observations: [Observation; 2],
    pub rewards: [f32; 2],
    pub terminated: bool,
    pub truncated: bool,
    pub outcome: Outcome,
    pub decision_step: u64,
    pub time_ns: u64,
}
/// Privileged diagnostic state. Never supply this to an actor as an observation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub seed: u64,
    pub time_ns: u64,
    pub decision_step: u64,
    pub outcome: Outcome,
    pub server: [ServerPlayer; 2],
    pub clients: [LocalPlayer; 2],
    pub replicas: [Replica; 2],
    pub queued_up: [usize; 2],
    pub queued_down: [usize; 2],
}
/// Renderable state belonging to one client, without authoritative opponent data.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientView {
    pub local: crate::state::Body,
    pub opponent: crate::state::Body,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CombatSound {
    Weak,
    Strong,
    Critical,
    Sweep,
    Knockback,
    NoDamage,
    Hurt,
    Death,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundEvent {
    pub time_ns: u64,
    pub source: usize,
    pub sound: CombatSound,
}
#[derive(Clone, Debug)]
pub struct Duel {
    config: Config,
    seed: u64,
    time_ns: u64,
    decision_step: u64,
    outcome: Outcome,
    server: [ServerPlayer; 2],
    clients: [LocalPlayer; 2],
    replicas: [Replica; 2],
    trackers: [Tracker; 2],
    wire: Wire,
    trace: Trace,
    rng: JavaRandom,
    next_server: u64,
    frames: [u64; 2],
    client_ticks: [u64; 2],
    first_client: usize,
    paper_moves: Option<[server_replay::MoveEventState; 2]>,
}
impl Duel {
    pub fn new(mut config: Config, seed: u64) -> Result<Self, SimError> {
        config.validate()?;
        if let Some(profile) = &config.ping_profile {
            profile.apply(&mut config.clients, seed);
        }
        let server = std::array::from_fn(|p| {
            let mut player = ServerPlayer::new(config.spawns[p], config.floor_y);
            player.health = config.initial_health[p];
            player
        });
        let clients = std::array::from_fn(|p| {
            let mut player = LocalPlayer::new(config.spawns[p], config.floor_y);
            player.health = config.initial_health[p];
            player
        });
        let replicas = std::array::from_fn(|p| {
            let mut replica = Replica::new(config.spawns[1 - p], config.floor_y);
            if config.expose_opponent_health {
                replica.health = Some(config.initial_health[1 - p]);
                replica.health_received_ns = Some(0);
            }
            replica
        });
        let trackers = server.map(Tracker::new);
        Ok(Self {
            next_server: config.server_phase_ns,
            first_client: config.first_client.unwrap_or((seed & 1) as usize),
            trace: Trace::new(config.trace_capacity),
            config,
            seed,
            time_ns: 0,
            decision_step: 0,
            outcome: Outcome::Ongoing,
            server,
            clients,
            replicas,
            trackers,
            wire: Wire::default(),
            rng: JavaRandom::new(seed),
            frames: [0; 2],
            client_ticks: [0; 2],
            paper_moves: None,
        })
    }
    pub fn reset(&mut self, seed: u64) -> Result<[Observation; 2], SimError> {
        *self = Self::new(self.config.clone(), seed)?;
        Ok(self.observations())
    }
    pub fn config(&self) -> &Config {
        &self.config
    }
    pub fn outcome(&self) -> Outcome {
        self.outcome
    }
    pub fn is_finished(&self) -> bool {
        self.outcome != Outcome::Ongoing
    }
    pub fn time_ns(&self) -> u64 {
        self.time_ns
    }
    pub fn decision_step(&self) -> u64 {
        self.decision_step
    }
    pub fn observe(&self, player: usize) -> Result<Observation, SimError> {
        if player > 1 {
            return Err(SimError::InvalidPlayer(player));
        }
        Ok(Observation::new(
            &self.clients[player],
            &self.replicas[player],
            self.time_ns,
            self.config.floor_y,
        ))
    }
    pub fn observations(&self) -> [Observation; 2] {
        std::array::from_fn(|p| self.observe(p).unwrap())
    }
    /// The player's own predicted client state, without authoritative opponent data.
    pub fn local_player(&self, player: usize) -> Result<&LocalPlayer, SimError> {
        self.clients
            .get(player)
            .ok_or(SimError::InvalidPlayer(player))
    }

    pub fn server_health(&self) -> [f32; 2] {
        self.server.map(|p| p.health)
    }
    pub fn client_view(&self, player: usize) -> Result<ClientView, SimError> {
        if player > 1 {
            return Err(SimError::InvalidPlayer(player));
        }
        Ok(ClientView {
            local: self.clients[player].body,
            opponent: self.replicas[player].body,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            seed: self.seed,
            time_ns: self.time_ns,
            decision_step: self.decision_step,
            outcome: self.outcome,
            server: self.server,
            clients: self.clients,
            replicas: self.replicas,
            queued_up: std::array::from_fn(|p| self.wire.up[p].len()),
            queued_down: std::array::from_fn(|p| self.wire.down[p].len()),
        }
    }
    pub fn drain_trace(&mut self) -> Vec<TraceRecord> {
        self.trace.drain()
    }
    pub fn dropped_trace_records(&self) -> u64 {
        self.trace.dropped
    }
    fn record(&mut self, event: TraceEvent) {
        self.trace.push(self.time_ns, event);
    }
    fn record_state(&mut self, phase: &str, player: Option<usize>) {
        if self.config.trace_states && self.trace.enabled() {
            self.record(TraceEvent::State {
                phase: phase.into(),
                player,
                snapshot: Box::new(self.snapshot()),
            });
        }
    }
    fn send_up(&mut self, p: usize, packet: Upstream) {
        let e = self
            .wire
            .send_up(p, self.time_ns, self.config.clients[p].upstream_ns, packet);
        if self.trace.enabled() {
            self.record(TraceEvent::SendUp {
                player: p,
                envelope: e,
            });
        }
    }
    fn send_down(&mut self, p: usize, packet: Downstream) {
        let e = self.wire.send_down(
            p,
            self.time_ns,
            self.config.clients[p].downstream_ns,
            packet,
        );
        if self.trace.enabled() {
            self.record(TraceEvent::SendDown {
                player: p,
                envelope: e,
            });
        }
    }
    fn frame_time(&self, p: usize) -> u64 {
        self.config.clients[p].frame_phase_ns
            + self.frames[p] * SECOND_NS / self.config.clients[p].fps as u64
    }
    pub fn step(&mut self, actions: [Action; 2]) -> Result<StepResult, SimError> {
        self.step_with_sounds(actions, |_| {})
    }

    /// Report audible combat events at their virtual server times. Frontends
    /// schedule delivery to the listener. Batch training uses a no-op listener.
    pub fn step_with_sounds(
        &mut self,
        actions: [Action; 2],
        emit: impl FnMut(SoundEvent),
    ) -> Result<StepResult, SimError> {
        self.step_applied_with_sounds(actions.map(Action::applied), emit)
    }

    /// Play back already resolved physical inputs, including historical Ctrl states.
    pub fn step_recorded(&mut self, actions: [AppliedAction; 2]) -> Result<StepResult, SimError> {
        self.step_applied_with_sounds(actions, |_| {})
    }
    fn step_applied_with_sounds(
        &mut self,
        actions: [AppliedAction; 2],
        mut emit: impl FnMut(SoundEvent),
    ) -> Result<StepResult, SimError> {
        if self.is_finished() {
            return Err(SimError::EpisodeFinished);
        }
        for a in actions {
            a.validate()?;
        }
        let old_health = self.server.map(|p| p.health);
        for (p, a) in actions.into_iter().enumerate() {
            self.clients[p].control = a;
            if a.click {
                self.clients[p].pending_clicks = self.clients[p].pending_clicks.saturating_add(1);
            }
        }
        let end = (self.decision_step + 1) * TICK_NS;
        loop {
            let t = self
                .next_server
                .min(self.frame_time(0))
                .min(self.frame_time(1));
            if t >= end {
                break;
            }
            self.time_ns = t;
            // Zero-cost server work precedes coincident frames. Across connections,
            // packet arrival then send sequence determines server receive order.
            if self.next_server == t {
                self.server_tick(&mut emit);
                self.next_server += TICK_NS;
                if self.is_finished() {
                    break;
                }
            }
            for p in [self.first_client, 1 - self.first_client] {
                if self.frame_time(p) == t {
                    self.client_frame(p);
                    self.frames[p] += 1;
                }
            }
        }
        self.decision_step += 1;
        if !self.is_finished() {
            self.time_ns = end;
            if self.decision_step >= self.config.max_steps {
                self.outcome = Outcome::Truncated;
            }
        }
        let damage = [
            old_health[0] - self.server[0].health,
            old_health[1] - self.server[1].health,
        ];
        let mut rewards = std::array::from_fn(|p| {
            (damage[1 - p] - damage[p]) * self.config.reward.damage
                - self.config.reward.time_penalty
        });
        if let Outcome::Victory { winner } = self.outcome {
            rewards[winner] += self.config.reward.win;
            rewards[1 - winner] -= self.config.reward.win;
        }
        Ok(StepResult {
            observations: self.observations(),
            rewards,
            terminated: matches!(self.outcome, Outcome::Victory { .. }),
            truncated: self.outcome == Outcome::Truncated,
            outcome: self.outcome,
            decision_step: self.decision_step,
            time_ns: self.time_ns,
        })
    }
    fn check_death(&mut self) {
        for p in 0..2 {
            if self.server[p].health <= 0.0 {
                self.outcome = Outcome::Victory { winner: 1 - p };
                return;
            }
        }
    }
    fn client_frame(&mut self, p: usize) {
        while self.wire.down[p]
            .front()
            .is_some_and(|e| e.arrival_ns <= self.time_ns)
        {
            let e = self.wire.down[p].pop_front().unwrap();
            if self.trace.enabled() {
                self.record(TraceEvent::ApplyDown {
                    player: p,
                    sequence: e.sequence,
                });
            }
            self.apply_down(p, e.packet);
            self.record_state("after_downstream", Some(p));
        }
        let due = self.config.clients[p].tick_phase_ns + self.client_ticks[p] * TICK_NS;
        if due <= self.time_ns {
            self.client_tick(p);
            self.client_ticks[p] += 1;
        }
    }
    fn apply_down(&mut self, p: usize, packet: Downstream) {
        match packet {
            Downstream::Motion { entity, velocity } => {
                if entity == p {
                    self.clients[p].body.velocity = velocity;
                } else {
                    let r = &mut self.replicas[p];
                    r.target_velocity = velocity;
                    r.velocity_steps = 3;
                    r.velocity_received_ns = self.time_ns;
                }
            }
            Downstream::RelativeMove {
                entity: _,
                delta,
                rotation,
                on_ground,
            } => {
                let r = &mut self.replicas[p];
                let pos = if let Some(delta) = delta {
                    let pos = decode_delta(r.position_base, delta);
                    r.position_base = pos;
                    pos
                } else {
                    r.target_position
                };
                let [yaw, pitch] = rotation
                    .map(|r| r.map(unpack_angle))
                    .unwrap_or([r.target_yaw, r.target_pitch]);
                r.interpolate_to(pos, yaw, pitch);
                r.body.on_ground = on_ground;
                r.position_received_ns = self.time_ns;
            }
            Downstream::PositionSync {
                entity: _,
                position,
                velocity: _,
                yaw,
                pitch,
                on_ground,
            } => {
                let r = &mut self.replicas[p];
                r.position_base = position;
                r.interpolate_to(position, yaw, pitch);
                r.body.on_ground = on_ground;
                r.position_received_ns = self.time_ns;
            }
            Downstream::Metadata {
                entity: _,
                pose,
                sprinting,
            } => {
                let r = &mut self.replicas[p];
                r.body.pose = pose;
                r.body.sprinting = sprinting;
                r.metadata_received_ns = self.time_ns;
            }
            Downstream::Health { health } => {
                self.clients[p].health = health;
                self.clients[p].health_received_ns = self.time_ns;
            }
            Downstream::SelfSprint { sprinting } => self.clients[p].body.sprinting = sprinting,
            Downstream::SelfMovementSpeed { speed } => self.clients[p].body.cached_speed = speed,
            Downstream::OpponentHealth { entity, health } => {
                debug_assert_eq!(entity, 1 - p);
                self.replicas[p].health = Some(health);
                self.replicas[p].health_received_ns = Some(self.time_ns);
            }
            Downstream::Equipment { equipment } => {
                let c = &mut self.clients[p];
                c.equipment = equipment;
            }
            Downstream::Correction {
                position,
                yaw,
                pitch,
                id,
            } => {
                let c = &mut self.clients[p];
                c.body.position = position;
                c.body.velocity = Vec3::ZERO;
                c.body.yaw = yaw;
                c.body.pitch = pitch;
                self.send_up(p, Upstream::TeleportAck { id });
                self.send_up(
                    p,
                    Upstream::Move {
                        position: Some(position),
                        rotation: Some([yaw, pitch]),
                        on_ground: false,
                    },
                );
            }
        }
    }
    fn client_tick(&mut self, p: usize) {
        self.record_state("before_client_tick", Some(p));
        if self.trace.enabled() {
            self.record(TraceEvent::ClientTick { player: p });
        }
        let action = self.clients[p].control;
        let body = &mut self.clients[p].body;
        let (yaw, pitch) = match action.look {
            Look::TrackTarget => aim_angles(
                body.eye(),
                self.replicas[p].body.bounds().aim_point(body.eye()),
                body.yaw,
            ),
            Look::Keep => (body.yaw, body.pitch),
            Look::Angles { yaw, pitch } => (yaw, pitch),
            Look::Turn {
                yaw_delta,
                pitch_delta,
            } => (body.yaw + yaw_delta, body.pitch + pitch_delta),
        };
        body.yaw = wrap_degrees(yaw);
        body.pitch = pitch.clamp(-90.0, 90.0);
        let clicks = std::mem::take(&mut self.clients[p].pending_clicks);
        for _ in 0..clicks {
            if self.clients[p].miss_time == 0 {
                match pick(
                    self.clients[p].body,
                    self.replicas[p].body,
                    self.config.floor_y,
                ) {
                    Pick::Entity => {
                        self.send_up(p, Upstream::Attack);
                        let c = &mut self.clients[p];
                        let strong =
                            combat::charge(c.body.attack_ticks, c.equipment.attack_speed()) > 0.9;
                        c.body.attack_ticks = 0;
                        // Local attack predicts sprint slowdown/extra knockback before
                        // authoritative hurt acceptance is known.
                        if strong && c.body.sprinting {
                            let a = radians(c.body.yaw) as f64;
                            let r = &mut self.replicas[p];
                            combat::knockback(
                                &mut r.body.velocity,
                                r.body.on_ground,
                                0.5,
                                sin(a) as f64,
                                -cos(a) as f64,
                                &mut self.rng,
                            );
                            c.body.velocity = c.body.velocity.horizontal_scale(0.6);
                            c.body.sprinting = false;
                        }
                    }
                    Pick::Miss => {
                        self.clients[p].body.attack_ticks = 0;
                        self.clients[p].miss_time = 10;
                    }
                    Pick::Floor => {
                        self.clients[p].destroying_floor = true;
                    }
                }
                self.send_up(p, Upstream::Swing);
            }
        }
        if !action.attack_held {
            self.clients[p].miss_time = 0;
        }
        if self.clients[p].miss_time == 0 {
            if action.attack_held
                && pick(
                    self.clients[p].body,
                    self.replicas[p].body,
                    self.config.floor_y,
                ) == Pick::Floor
            {
                self.clients[p].destroying_floor = true;
                self.send_up(p, Upstream::Swing);
            } else if self.clients[p].destroying_floor {
                self.clients[p].destroying_floor = false;
                self.clients[p].body.attack_ticks = 0;
            }
        }
        self.clients[p].miss_time = self.clients[p].miss_time.saturating_sub(1);
        let feedback = (self.config.server_rules == ServerRules::PaperFlatV2).then(|| {
            physics::MovementFeedback {
                sprinting: self.clients[p].body.sprinting,
                movement_speed: self.clients[p].body.cached_speed,
            }
        });
        physics::tick_local_with_feedback(
            &mut self.clients[p],
            action.keys,
            self.config.floor_y,
            self.config.floor_friction,
            self.config.sprint_window_ticks,
            feedback,
        );
        if self.clients[p].sent_input != action.keys {
            self.clients[p].sent_input = action.keys;
            self.send_up(p, Upstream::Input { keys: action.keys });
        }
        let mut c = self.clients[p];
        if c.body.sprinting != c.sent_sprint {
            c.sent_sprint = c.body.sprinting;
            self.send_up(
                p,
                Upstream::Sprint {
                    enabled: c.sent_sprint,
                },
            );
        }
        c.position_reminder += 1;
        let moved = (c.body.position - c.sent_position).length_squared() > 4e-8
            || c.position_reminder >= 20;
        let rotated = c.body.yaw != c.sent_yaw || c.body.pitch != c.sent_pitch;
        if moved || rotated || c.body.on_ground != c.sent_ground {
            self.send_up(
                p,
                Upstream::Move {
                    position: moved.then_some(c.body.position),
                    rotation: rotated.then_some([c.body.yaw, c.body.pitch]),
                    on_ground: c.body.on_ground,
                },
            );
        }
        if moved {
            c.sent_position = c.body.position;
            c.position_reminder = 0;
        }
        if rotated {
            c.sent_yaw = c.body.yaw;
            c.sent_pitch = c.body.pitch;
        }
        c.sent_ground = c.body.on_ground;
        self.clients[p] = c;
        self.replicas[p].tick();
        self.send_up(p, Upstream::TickEnd);
        self.record_state("after_client_tick", Some(p));
    }
    fn server_tick(&mut self, emit: &mut impl FnMut(SoundEvent)) {
        self.record_state("before_server_tick", None);
        if self.trace.enabled() {
            self.record(TraceEvent::ServerTick);
        }
        while let Some((p, e)) = self.wire.pop_up(self.time_ns) {
            if self.trace.enabled() {
                self.record(TraceEvent::ApplyUp {
                    player: p,
                    sequence: e.sequence,
                });
            }
            self.apply_up(p, e.packet, emit);
            self.record_state("after_upstream", Some(p));
            self.check_death();
            if self.is_finished() {
                return;
            }
        }
        for p in 0..2 {
            self.track(p);
        }
        for p in 0..2 {
            let s = &mut self.server[p];
            s.invulnerable_ticks = s.invulnerable_ticks.saturating_sub(1);
            s.refresh_attributes();
            match self.config.server_rules {
                ServerRules::Legacy | ServerRules::PaperSwing => {
                    s.body.velocity = s.body.velocity * 0.98;
                    physics::trim_velocity(&mut s.body.velocity);
                }
                ServerRules::PaperFlatV1 | ServerRules::PaperFlatV2 => {
                    physics::tick_server_velocity(
                        &mut s.body,
                        self.config.floor_y,
                        self.config.floor_friction,
                    );
                }
            }
            s.hurt_ticks = s.hurt_ticks.saturating_sub(1);
            s.body.attack_ticks = s.body.attack_ticks.saturating_add(1);
            let sword = s.equipment.sword > 0;
            if sword != s.last_sword_present {
                s.body.attack_ticks = 0;
                s.last_sword_present = sword;
            }
            let old_pose = s.body.pose;
            s.body.pose = if s.input.sneak {
                Pose::Crouching
            } else {
                Pose::Standing
            };
            s.metadata_dirty |= old_pose != s.body.pose;
            s.body.cached_speed = physics::movement_speed(s.body.sprinting);
            s.first_good = s.body.position;
            s.last_good = s.body.position;
            s.move_packets = 0;
            let health = s.health;
            let equipment = s.equipment;
            if self.trackers[p].sent_health != health {
                self.trackers[p].sent_health = health;
                self.send_down(p, Downstream::Health { health });
                if self.config.expose_opponent_health {
                    self.send_down(1 - p, Downstream::OpponentHealth { entity: p, health });
                }
            }
            if self.trackers[p].sent_equipment != equipment {
                self.trackers[p].sent_equipment = equipment;
                self.send_down(p, Downstream::Equipment { equipment });
            }
        }
        self.record_state("after_server_tick", None);
    }
    fn apply_up(&mut self, p: usize, packet: Upstream, emit: &mut impl FnMut(SoundEvent)) {
        if !self.server[p].loaded {
            return;
        }
        match packet {
            Upstream::Attack => {
                let target = 1 - p;
                if !self.server[target].loaded
                    || self.server[target]
                        .body
                        .bounds()
                        .distance_squared(self.server[p].body.eye())
                        > 36.0
                {
                    if self.trace.enabled() {
                        self.record(TraceEvent::AttackRejected {
                            attacker: p,
                            reason: "server_range_or_unloaded".into(),
                        });
                    }
                    return;
                }
                let (a, b) = self.server.split_at_mut(1);
                let (attacker, victim) = if p == 0 {
                    (&mut a[0], &mut b[0])
                } else {
                    (&mut b[0], &mut a[0])
                };
                let result =
                    combat::attack(attacker, victim, &mut self.rng, self.config.unbreakable);
                let mut sound = |source, sound| {
                    emit(SoundEvent {
                        time_ns: self.time_ns,
                        source,
                        sound,
                    })
                };
                // Player.attack plays knockback before hurt(), even if hurt
                // rejects the hit. LivingEntity.hurt only voices full hits.
                if result.sprint_hit {
                    sound(p, CombatSound::Knockback);
                }
                if result.damage.full_hit {
                    sound(
                        target,
                        if victim.health <= 0.0 {
                            CombatSound::Death
                        } else {
                            CombatSound::Hurt
                        },
                    );
                }
                sound(
                    p,
                    if !result.damage.accepted {
                        CombatSound::NoDamage
                    } else if result.critical {
                        CombatSound::Critical
                    } else if result.sweep {
                        CombatSound::Sweep
                    } else if result.charge > 0.9 {
                        CombatSound::Strong
                    } else {
                        CombatSound::Weak
                    },
                );
                if let Some(v) = result.motion {
                    self.send_down(
                        target,
                        Downstream::Motion {
                            entity: target,
                            velocity: quantize_motion(v),
                        },
                    );
                }
                if self.trace.enabled() {
                    self.record(TraceEvent::Attack {
                        attacker: p,
                        result,
                    });
                }
            }
            Upstream::Swing => {
                if matches!(
                    self.config.server_rules,
                    ServerRules::PaperSwing | ServerRules::PaperFlatV2
                ) {
                    self.server[p].body.attack_ticks = 0;
                }
            }
            Upstream::Input { keys } => {
                self.server[p].metadata_dirty |= self.server[p].input.sneak != keys.sneak;
                self.server[p].input = keys;
            }
            Upstream::Sprint { enabled } => {
                self.server[p].metadata_dirty |= self.server[p].body.sprinting != enabled;
                self.server[p].body.sprinting = enabled;
            }
            Upstream::TickEnd => {
                let s = &mut self.server[p];
                if !s.received_movement {
                    s.known_movement = Vec3::ZERO;
                }
                s.received_movement = false;
            }
            Upstream::TeleportAck { id } => {
                let s = &mut self.server[p];
                if s.awaiting_teleport == Some(id) {
                    s.awaiting_teleport = None;
                    s.last_good = s.body.position;
                }
            }
            Upstream::Move {
                position,
                rotation,
                on_ground,
            } => self.server_move(p, position, rotation, on_ground, emit),
        }
    }
    fn server_move(
        &mut self,
        p: usize,
        position: Option<Vec3>,
        rotation: Option<[f32; 2]>,
        on_ground: bool,
        emit: &mut impl FnMut(SoundEvent),
    ) {
        let s = &mut self.server[p];
        if s.awaiting_teleport.is_some() {
            return;
        }
        let target = position.unwrap_or(s.body.position);
        s.move_packets += 1;
        let count = if s.move_packets > 5 {
            1
        } else {
            s.move_packets
        };
        let too_fast = (target - s.first_good).length_squared() - s.body.velocity.length_squared()
            > 100.0 * count as f64;
        if !target.finite() || target.y < self.config.floor_y - 1e-7 || too_fast {
            s.teleport_id = s.teleport_id.wrapping_add(1);
            s.awaiting_teleport = Some(s.teleport_id);
            let packet = Downstream::Correction {
                position: s.body.position,
                yaw: s.body.yaw,
                pitch: s.body.pitch,
                id: s.teleport_id,
            };
            self.send_down(p, packet);
            if self.trace.enabled() {
                self.record(TraceEvent::Correction { player: p });
            }
            return;
        }
        let delta = target - s.body.position;
        if s.body.on_ground && !on_ground && target.y - s.last_good.y > 0.0 {
            physics::jump(&mut s.body);
        }
        if let Some(moves) = &mut self.paper_moves
            && moves[p].ignores_move(&s.body, target, rotation)
        {
            return;
        }
        s.body.position = target;
        if let Some([yaw, pitch]) = rotation {
            s.body.yaw = wrap_degrees(yaw);
            s.body.pitch = wrap_degrees(pitch);
        }
        s.body.on_ground = on_ground;
        s.known_movement = delta;
        s.received_movement = true;
        s.last_good = target;
        if delta.y < 0.0 {
            s.body.fall_distance -= (delta.y as f32) as f64;
        }
        if on_ground {
            let damage = physics::fall_damage(s.body.fall_distance);
            if damage > 0.0 {
                let result = combat::hurt(s, damage, true, self.config.unbreakable);
                self.trace.push(
                    self.time_ns,
                    TraceEvent::FallDamage {
                        player: p,
                        damage: result,
                    },
                );
                if result.full_hit {
                    emit(SoundEvent {
                        time_ns: self.time_ns,
                        source: p,
                        sound: if s.health <= 0.0 {
                            CombatSound::Death
                        } else {
                            CombatSound::Hurt
                        },
                    });
                }
            }
            s.body.fall_distance = 0.0;
        } else if delta.y > 0.0 {
            s.body.fall_distance = 0.0;
        }
    }
    fn track(&mut self, p: usize) {
        let s = self.server[p];
        let mut t = self.trackers[p];
        let dirty = s.metadata_dirty || t.pose != s.body.pose || t.sprinting != s.body.sprinting;
        if t.tick % 2 == 0 || s.needs_sync || dirty {
            let yaw = pack_angle(s.body.yaw);
            let pitch = pack_angle(s.body.pitch);
            let rotated = yaw != t.yaw || pitch != t.pitch;
            t.teleport_delay += 1;
            let moved = (s.body.position - t.position_base).length_squared()
                >= 7.6293945e-6f32 as f64
                || t.tick % 60 == 0;
            let delta = encode_delta(t.position_base, s.body.position);
            let full = s.body.on_ground != t.on_ground
                || t.teleport_delay > 400
                || delta.iter().any(|&v| !(-32768..=32767).contains(&v));
            let diff = (s.body.velocity - t.velocity).length_squared();
            if diff > 1e-7 || (diff > 0.0 && s.body.velocity.length_squared() == 0.0) {
                t.velocity = s.body.velocity;
                self.send_down(
                    1 - p,
                    Downstream::Motion {
                        entity: p,
                        velocity: quantize_motion(t.velocity),
                    },
                );
            }
            if full {
                self.send_down(
                    1 - p,
                    Downstream::PositionSync {
                        entity: p,
                        position: s.body.position,
                        velocity: s.body.velocity,
                        yaw: s.body.yaw,
                        pitch: s.body.pitch,
                        on_ground: s.body.on_ground,
                    },
                );
                t.on_ground = s.body.on_ground;
                t.teleport_delay = 0;
            } else if moved || rotated {
                self.send_down(
                    1 - p,
                    Downstream::RelativeMove {
                        entity: p,
                        delta: moved.then_some(delta.map(|v| v as i16)),
                        rotation: rotated.then_some([yaw, pitch]),
                        on_ground: s.body.on_ground,
                    },
                );
            }
            if full || moved {
                t.position_base = s.body.position;
            }
            if full || rotated {
                t.yaw = yaw;
                t.pitch = pitch;
            }
            if dirty {
                t.pose = s.body.pose;
                if self.config.server_rules == ServerRules::PaperFlatV2
                    && t.sprinting != s.body.sprinting
                {
                    self.send_down(
                        p,
                        Downstream::SelfSprint {
                            sprinting: s.body.sprinting,
                        },
                    );
                    self.send_down(
                        p,
                        Downstream::SelfMovementSpeed {
                            speed: physics::movement_speed(s.body.sprinting),
                        },
                    );
                }
                t.sprinting = s.body.sprinting;
                self.send_down(
                    1 - p,
                    Downstream::Metadata {
                        entity: p,
                        pose: t.pose,
                        sprinting: t.sprinting,
                    },
                );
            }
            self.server[p].needs_sync = false;
            self.server[p].metadata_dirty = false;
        }
        t.tick += 1;
        self.trackers[p] = t;
        if self.server[p].hurt_marked {
            self.server[p].hurt_marked = false;
            for to in 0..2 {
                self.send_down(
                    to,
                    Downstream::Motion {
                        entity: p,
                        velocity: quantize_motion(s.body.velocity),
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Keys;
    fn near() -> Config {
        let mut c = Config::default();
        c.spawns[0].position.z = -1.0;
        c.spawns[1].position.z = 1.0;
        c.trace_capacity = 1000;
        c
    }

    #[test]
    fn paper_sprint_feedback_waits_for_the_receiving_clients_frame() {
        let mut config = near();
        config.server_rules = ServerRules::PaperFlatV2;
        config.clients[0].downstream_ns = 51_000_000;
        let mut duel = Duel::new(config, 0).unwrap();
        duel.clients[0].body.sprinting = true;
        duel.clients[0].body.cached_speed = physics::movement_speed(true);
        duel.trackers[0].sprinting = true;
        duel.client_ticks[0] = 10;

        duel.track(0);
        duel.time_ns = 50_000_000;
        duel.client_frame(0);
        assert!(duel.clients[0].body.sprinting);
        assert_eq!(
            duel.clients[0].body.cached_speed,
            physics::movement_speed(true)
        );

        duel.time_ns = 50_000_000 + 1_000_000;
        duel.client_frame(0);
        assert!(!duel.clients[0].body.sprinting);
        assert_eq!(
            duel.clients[0].body.cached_speed,
            physics::movement_speed(false)
        );
    }

    #[test]
    fn paper_swing_resets_server_charge_without_an_attack_packet() {
        for (rules, expected) in [
            (ServerRules::Legacy, 14),
            (ServerRules::PaperSwing, 0),
            (ServerRules::PaperFlatV2, 0),
        ] {
            let config = Config {
                server_rules: rules,
                ..near()
            };
            let mut duel = Duel::new(config, 0).unwrap();
            duel.server[0].body.attack_ticks = 14;
            duel.apply_up(0, Upstream::Swing, &mut |_| {});
            assert_eq!(duel.server[0].body.attack_ticks, expected);
        }
    }

    #[test]
    fn sword_sounds_follow_server_attack_branches_in_order() {
        use CombatSound::*;
        fn hit(setup: impl FnOnce(&mut [ServerPlayer; 2])) -> Vec<(usize, CombatSound)> {
            let mut duel = Duel::new(near(), 42).unwrap();
            setup(&mut duel.server);
            let mut sounds = vec![];
            duel.apply_up(0, Upstream::Attack, &mut |e| {
                sounds.push((e.source, e.sound))
            });
            sounds
        }
        assert_eq!(hit(|_| {}), [(1, Hurt), (0, Sweep)]);
        assert_eq!(hit(|p| p[0].body.attack_ticks = 2), [(1, Hurt), (0, Weak)]);
        assert_eq!(
            hit(|p| p[0].body.on_ground = false),
            [(1, Hurt), (0, Strong)]
        );
        assert_eq!(
            hit(|p| {
                p[0].body.on_ground = false;
                p[0].body.fall_distance = 0.1;
            }),
            [(1, Hurt), (0, Critical)]
        );
        assert_eq!(
            hit(|p| p[0].body.sprinting = true),
            [(0, Knockback), (1, Hurt), (0, Strong)]
        );
        assert_eq!(
            hit(|p| p[0].known_movement.x = f64::from(physics::movement_speed(false)) * 2.5),
            [(1, Hurt), (0, Strong)]
        );
        assert_eq!(hit(|p| p[0].equipment.sword = 0), [(1, Hurt), (0, Strong)]);
        assert_eq!(
            hit(|p| {
                p[1].invulnerable_ticks = 15;
                p[1].last_hurt = 1.0;
            }),
            [(0, Sweep)]
        );
        assert_eq!(
            hit(|p| {
                p[0].body.sprinting = true;
                p[1].invulnerable_ticks = 15;
                p[1].last_hurt = 7.0;
            }),
            [(0, Knockback), (0, NoDamage)]
        );
        assert_eq!(hit(|p| p[1].health = 0.1), [(1, Death), (0, Sweep)]);
        assert!(hit(|p| p[1].body.position.z += 10.0).is_empty());
    }
    #[test]
    fn opponent_health_waits_for_recipient_downstream_and_resets() {
        let mut config = Config {
            expose_opponent_health: true,
            initial_health: [12., 6.],
            ..Config::default()
        };
        config.clients[0].downstream_ns = 0;
        config.clients[1].downstream_ns = 75_000_000;
        let mut duel = Duel::new(config, 4).unwrap();
        duel.server[0].health = 9.;
        duel.server[1].health = 5.;
        let first = duel.step([Action::default(); 2]).unwrap().observations;
        assert_eq!(first[0].opponent_health, Some(5.));
        assert_eq!(first[1].opponent_health, Some(12.));
        assert_eq!(first[1].opponent_health_age_ns, Some(TICK_NS));
        let second = duel.step([Action::default(); 2]).unwrap().observations;
        assert_eq!(second[1].opponent_health, Some(9.));
        assert!(second[1].opponent_health_age_ns.unwrap() < TICK_NS);
        let reset = duel.reset(5).unwrap();
        assert_eq!(reset[1].opponent_health, Some(12.));
        assert_eq!(reset[1].opponent_health_age_ns, Some(0));
        let legacy = Duel::new(Config::default(), 6).unwrap();
        assert_eq!(legacy.observe(0).unwrap().opponent_health, None);
    }

    #[test]
    fn damage_waits_for_server_gate_and_hp_for_downstream() {
        let mut d = Duel::new(near(), 0).unwrap();
        let hit = Action {
            click: true,
            ..Action::default()
        };
        d.step([hit, Action::default()]).unwrap();
        assert_eq!(d.server[1].health, 20.0);
        assert_eq!(d.clients[1].health, 20.0);
        d.step([Action::default(); 2]).unwrap();
        assert!(d.server[1].health < 20.0);
        assert_eq!(d.clients[1].health, d.server[1].health);
        let trace = d.drain_trace();
        assert!(
            trace.iter().any(|r| r.time_ns == TICK_NS
                && matches!(r.event, TraceEvent::Attack { attacker: 0, .. }))
        );
    }
    #[test]
    fn local_miss_does_not_reset_server_charge() {
        let mut d = Duel::new(near(), 0).unwrap();
        let mut sounds = Vec::new();
        d.step_with_sounds(
            [
                Action {
                    look: Look::Angles {
                        yaw: 180.0,
                        pitch: 0.0,
                    },
                    click: true,
                    ..Action::default()
                },
                Action::default(),
            ],
            |sound| sounds.push(sound),
        )
        .unwrap();
        d.step_with_sounds([Action::default(); 2], |sound| sounds.push(sound))
            .unwrap();
        assert!(sounds.is_empty());
        assert!(d.server[0].body.attack_ticks > 20);
        assert!(d.clients[0].body.attack_ticks < 5);
        assert_eq!(d.server[1].health, 20.0);
    }
    #[test]
    fn observation_does_not_see_authoritative_teleport() {
        let mut d = Duel::new(near(), 0).unwrap();
        let before = d.observe(0).unwrap();
        d.server[1].body.position.x += 10.0;
        assert_eq!(d.observe(0).unwrap(), before);
    }
    #[test]
    fn attacks_precede_new_sprint_command() {
        let mut d = Duel::new(near(), 0).unwrap();
        d.step([
            Action {
                click: true,
                keys: Keys {
                    forward: true,
                    ..Keys::default()
                },
                ..Action::default()
            },
            Action::default(),
        ])
        .unwrap();
        d.step([Action::default(); 2]).unwrap();
        let a = d
            .drain_trace()
            .into_iter()
            .find_map(|r| {
                if let TraceEvent::Attack { result, .. } = r.event {
                    Some(result)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(!a.sprint_hit);
    }

    #[test]
    fn releasing_and_repressing_forward_restores_server_sprint_after_a_hit() {
        let mut d = Duel::new(near(), 0).unwrap();
        let idle = Action::default();
        let forward = Action {
            keys: Keys {
                forward: true,
                ..Keys::default()
            },
            ..idle
        };
        d.step([forward, idle]).unwrap();
        assert!(d.observe(0).unwrap().sprinting);
        d.step([
            Action {
                click: true,
                ..forward
            },
            idle,
        ])
        .unwrap();
        d.step([forward, idle]).unwrap();
        assert!(d.drain_trace().iter().any(|r| matches!(r.event, TraceEvent::Attack { result, .. } if result.sprint_hit && result.damage.accepted)));
        assert!(!d.server[0].body.sprinting);
        d.step([idle; 2]).unwrap();
        assert!(!d.observe(0).unwrap().sprinting);
        d.step([forward, idle]).unwrap();
        d.step([forward, idle]).unwrap();
        assert!(d.server[0].body.sprinting);
    }
    #[test]
    fn death_prevents_later_attack() {
        let mut d = Duel::new(near(), 0).unwrap();
        d.server[1].health = 0.1;
        let hit = Action {
            click: true,
            ..Action::default()
        };
        d.step([hit; 2]).unwrap();
        let result = d.step([Action::default(); 2]).unwrap();
        assert_eq!(result.outcome, Outcome::Victory { winner: 0 });
        assert_eq!(d.server[0].health, 20.0);
        assert_eq!(result.rewards, [1.0, -1.0]);
        assert_eq!(
            d.step([Action::default(); 2]),
            Err(SimError::EpisodeFinished)
        );
    }
    #[test]
    fn timeout_is_truncation_not_health_winner() {
        let c = Config {
            max_steps: 2,
            ..Config::default()
        };
        let mut d = Duel::new(c, 1).unwrap();
        d.step([Action::default(); 2]).unwrap();
        let r = d.step([Action::default(); 2]).unwrap();
        assert!(r.truncated);
        assert!(!r.terminated);
        assert_eq!(r.rewards, [0.0; 2]);
    }
    #[test]
    fn clone_restores_pending_packets_and_rng() {
        let mut a = Duel::new(near(), 99).unwrap();
        a.step(
            [Action {
                click: true,
                ..Action::default()
            }; 2],
        )
        .unwrap();
        let mut b = a.clone();
        for _ in 0..30 {
            let actions = [Action {
                keys: Keys {
                    forward: true,
                    jump: true,
                    ..Keys::default()
                },
                ..Action::default()
            }; 2];
            assert_eq!(a.step(actions).unwrap(), b.step(actions).unwrap());
            assert_eq!(a.snapshot(), b.snapshot());
        }
    }
    #[test]
    fn reset_clears_every_queue_and_trace() {
        let mut d = Duel::new(near(), 0).unwrap();
        d.step(
            [Action {
                click: true,
                ..Action::default()
            }; 2],
        )
        .unwrap();
        d.reset(0).unwrap();
        assert_eq!(d.snapshot(), Duel::new(near(), 0).unwrap().snapshot());
        assert!(d.drain_trace().is_empty());
    }
    #[test]
    fn bounded_trace_and_invalid_actions_are_atomic() {
        let mut c = near();
        c.trace_capacity = 2;
        let mut d = Duel::new(c, 0).unwrap();
        let before = d.snapshot();
        assert!(
            d.step(
                [Action {
                    look: Look::Angles {
                        yaw: f32::NAN,
                        pitch: 0.0
                    },
                    ..Action::default()
                }; 2]
            )
            .is_err()
        );
        assert_eq!(d.snapshot(), before);
        d.step([Action::default(); 2]).unwrap();
        assert_eq!(d.drain_trace().len(), 2);
        assert!(d.dropped_trace_records() > 0);
    }
    #[test]
    fn health_dirtiness_triggers_tracking_between_periodic_updates() {
        let mut d = Duel::new(near(), 0).unwrap();
        d.trackers[0].tick = 1;
        d.server[0].invulnerable_ticks = 15;
        d.server[0].last_hurt = 1.0;
        d.server[0].body.position.x += 0.1;
        combat::hurt(&mut d.server[0], 7.0, false, true);
        assert!(!d.server[0].needs_sync);
        d.track(0);
        assert!(d.drain_trace().iter().any(|r| matches!(
            r.event,
            TraceEvent::SendDown {
                envelope: crate::network::Envelope {
                    packet: Downstream::RelativeMove { .. },
                    ..
                },
                ..
            }
        )));
        assert!(!d.server[0].metadata_dirty);
    }
}
