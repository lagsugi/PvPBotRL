//! Server-tick driver for the paired Citizens bots used by PvPBotRL on Paper.
use super::*;
use crate::{action::AppliedKeys, state::Body};
use std::{collections::VecDeque, f64::consts::TAU};

#[derive(Clone, Copy, Debug)]
struct PendingAction {
    ready_ns: u64,
    keys: AppliedKeys,
    yaw: f32,
    pitch: f32,
    click: bool,
    selected: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Driver {
    pending: [VecDeque<PendingAction>; 2],
    pub(super) hurt_this_tick: [bool; 2],
}

impl Driver {
    pub(super) fn pending_count(&self, seat: usize) -> usize {
        self.pending[seat].len()
    }
}

impl Duel {
    pub(super) fn step_citizens(
        &mut self,
        mut decide: impl FnMut(&Self, usize) -> Result<AppliedAction, SimError>,
        emit: &mut impl FnMut(SoundEvent),
    ) -> Result<StepResult, SimError> {
        let old_health = self.server_health();
        let end = self.time_ns + TICK_NS;
        let first = self.first_client ^ (self.decision_step as usize % 2);
        self.record_state("before_server_tick", None);
        if self.trace.enabled() {
            self.record(TraceEvent::ServerTick);
        }
        for seat in [first, 1 - first] {
            self.apply_citizens_pending(seat, emit);
            self.check_death();
            if self.is_finished() {
                break;
            }
            if self.time_ns < self.config.clients[seat].downstream_ns {
                continue;
            }
            self.sync_citizens_observation(seat);
            self.record_state("before_citizens_decision", Some(seat));
            let action = decide(self, seat)?;
            self.queue_citizens_action(seat, action);
            self.apply_citizens_pending(seat, emit);
            self.check_death();
            if self.is_finished() {
                break;
            }
        }
        if !self.is_finished() {
            for seat in 0..2 {
                self.move_citizens(seat);
            }
            self.check_death();
            self.track_citizens();
            self.record_state("after_server_tick", None);
            // The next decision consumes tracker updates emitted after movement.
            self.time_ns = end;
            for seat in 0..2 {
                if self.time_ns >= self.config.clients[seat].downstream_ns {
                    if self.time_ns - TICK_NS < self.config.clients[seat].downstream_ns {
                        let r = &mut self.replicas[seat];
                        r.position_received_ns = self.time_ns;
                        r.metadata_received_ns = self.time_ns;
                        r.velocity_received_ns = self.time_ns;
                        if r.health.is_some() {
                            r.health_received_ns = Some(self.time_ns);
                        }
                        self.clients[seat].health_received_ns = self.time_ns;
                    }
                    self.drain_downstream(seat);
                    self.replicas[seat].tick();
                }
            }
        }
        for seat in 0..2 {
            self.sync_citizens_observation(seat);
        }
        Ok(self.finish_step(old_health, end))
    }

    fn sync_citizens_observation(&mut self, seat: usize) {
        let server = self.server[seat];
        let local = &mut self.clients[seat];
        if local.health != server.health {
            local.health_received_ns = self.time_ns;
        }
        local.body = server.body;
        // The adapter exposes its input sprint latch. A successful sprint hit
        // can clear the server sprint flag without clearing this latch.
        local.body.sprinting = local.sent_sprint;
        local.health = server.health;
        local.equipment = server.equipment;
    }

    fn queue_citizens_action(&mut self, seat: usize, action: AppliedAction) {
        let mut body = self.server[seat].body;
        let enemy = self.replicas[seat].body;
        let (yaw, pitch) = match action.look {
            Look::TrackTarget => aim(body, enemy),
            Look::Keep => (body.yaw, body.pitch),
            Look::Angles { yaw, pitch } => (yaw, pitch),
            Look::Turn {
                yaw_delta,
                pitch_delta,
            } => (body.yaw + yaw_delta, body.pitch + pitch_delta),
        };
        body.yaw = yaw;
        body.pitch = pitch.clamp(-90., 90.);
        let selected = action.click && can_hit(body, enemy, self.config.floor_y);
        self.citizens.as_mut().unwrap().pending[seat].push_back(PendingAction {
            ready_ns: self.time_ns + self.config.clients[seat].upstream_ns,
            keys: action.keys,
            yaw: body.yaw,
            pitch: body.pitch,
            click: action.click,
            selected,
        });
    }

    fn apply_citizens_pending(&mut self, seat: usize, emit: &mut impl FnMut(SoundEvent)) {
        while self.server[1 - seat].loaded {
            let pending = &mut self.citizens.as_mut().unwrap().pending[seat];
            if pending.front().is_none_or(|p| p.ready_ns > self.time_ns) {
                break;
            }
            let action = pending.pop_front().unwrap();
            self.server[seat].body.yaw = action.yaw;
            self.server[seat].body.pitch = action.pitch;
            self.server[seat].input = action.keys;
            if action.selected {
                self.record_state("before_citizens_attack", Some(seat));
                self.apply_up(seat, Upstream::Attack, emit);
            }
            if action.click {
                self.apply_up(seat, Upstream::Swing, emit);
            }
        }
    }

    fn move_citizens(&mut self, seat: usize) {
        let server = &mut self.server[seat];
        let local = &mut self.clients[seat];
        let before = server.body;
        let keys = server.input;
        let old_sprint = local.sent_sprint;
        let forward = keys.forward_impulse();
        local.sent_sprint = forward && (old_sprint || !local.input.sneak);
        if local.sent_sprint != old_sprint && server.body.sprinting != local.sent_sprint {
            server.body.sprinting = local.sent_sprint;
            server.metadata_dirty = true;
        }
        local.crouching = local.input.sneak;
        local.input = keys;
        local.body = server.body;
        local.equipment = server.equipment;
        local.body.sprinting |= local.sent_sprint;
        let speed = physics::movement_speed(local.body.sprinting);
        physics::travel(
            local,
            keys,
            self.config.floor_y,
            self.config.floor_friction,
            speed,
        );
        if local.jump_delay == 10 {
            server.needs_sync = true;
        }
        local.body.sprinting = server.body.sprinting;
        server.body = local.body;
        server.known_movement = server.body.position - before.position;
        server.invulnerable_ticks = server.invulnerable_ticks.saturating_sub(1);
        server.hurt_ticks = server.hurt_ticks.saturating_sub(1);
        if server.body.on_ground {
            let distance = before.fall_distance - server.known_movement.y.min(0.) as f32 as f64;
            let damage = physics::fall_damage(distance);
            if damage > 0. {
                let damage = combat::hurt(server, damage, true, self.config.unbreakable);
                self.trace.push(
                    self.time_ns,
                    TraceEvent::FallDamage {
                        player: seat,
                        damage,
                    },
                );
            }
        }
        server.refresh_attributes();
    }

    fn track_citizens(&mut self) {
        for seat in 0..2 {
            self.track(seat);
            let server = self.server[seat];
            let tracker = &mut self.trackers[seat];
            if tracker.sent_health != server.health {
                tracker.sent_health = server.health;
                if self.config.expose_opponent_health {
                    self.send_down(
                        1 - seat,
                        Downstream::OpponentHealth {
                            entity: seat,
                            health: server.health,
                        },
                    );
                }
            }
            if std::mem::take(&mut self.citizens.as_mut().unwrap().hurt_this_tick[seat]) {
                // Citizens restores the hurt flag before the same tick's tracker
                // flush, after the NPC traveled with its retained velocity.
                self.send_down(
                    1 - seat,
                    Downstream::Motion {
                        entity: seat,
                        velocity: quantize_motion(server.body.velocity),
                    },
                );
            }
        }
    }
}

fn aim(body: Body, enemy: Body) -> (f32, f32) {
    let eye = body.eye();
    let toward = enemy.bounds().aim_point(eye) - eye;
    let horizontal = toward.horizontal_squared().sqrt();
    if toward.length_squared() <= 1e-12 {
        return (body.yaw, body.pitch);
    }
    let yaw = if horizontal == 0. {
        body.yaw
    } else {
        ((-toward.x).atan2(toward.z).rem_euclid(TAU)).to_degrees() as f32
    };
    (yaw, (-toward.y.atan2(horizontal)).to_degrees() as f32)
}

pub(super) fn can_hit(body: Body, enemy: Body, floor: f64) -> bool {
    let eye = body.eye();
    let bounds = enemy.bounds();
    if bounds.contains(eye) {
        return true;
    }
    let (sin_y, cos_y) = f64::from(body.yaw).to_radians().sin_cos();
    let (sin_p, cos_p) = f64::from(body.pitch).to_radians().sin_cos();
    let direction = Vec3::new(-cos_p * sin_y, -sin_p, cos_p * cos_y);
    bounds
        .clip(eye, eye + direction * 3.)
        .is_some_and(|hit| (hit - eye).length_squared() < 9. && hit.y > floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Keys, trace::Replay};

    fn close_duel() -> Duel {
        let mut config = Config {
            duel_driver: DuelDriver::PaperCitizens,
            expose_opponent_health: true,
            ..Config::default()
        }
        .with_fixed_equipment();
        config.spawns[0].position.z = -1.25;
        config.spawns[1].position.z = 1.25;
        for timing in &mut config.clients {
            timing.upstream_ns = 0;
            timing.downstream_ns = 0;
        }
        Duel::new(config, 13).unwrap()
    }

    #[test]
    fn second_decision_sees_same_tick_impulse_but_keeps_delivered_opponent_hp() {
        let mut duel = close_duel();
        let mut replay = Replay::new(duel.config().clone(), 13);
        let mut order = Vec::new();
        let (_, actions) = duel
            .step_with_policy(|_, seat, obs| {
                order.push(seat);
                assert_eq!(obs.opponent_health, Some(20.));
                assert!(!obs.opponent_on_ground);
                if seat == 1 {
                    assert!(obs.health < 20.);
                    assert!(obs.velocity.z > 0.39);
                }
                Ok(Action {
                    click: seat == 0,
                    ..Action::default()
                })
            })
            .unwrap();
        replay.record(actions);
        assert_eq!(order, [0, 1]);
        assert!(duel.observe(0).unwrap().opponent_health.unwrap() < 20.);
        assert!(duel.observe(1).unwrap().opponent_on_ground);
        order.clear();
        let (_, actions) = duel
            .step_with_policy(|_, seat, _| {
                order.push(seat);
                Ok(Action {
                    keys: Keys {
                        forward: true,
                        ..Keys::default()
                    },
                    ..Action::default()
                })
            })
            .unwrap();
        replay.record(actions);
        assert_eq!(order, [1, 0]);
        assert_eq!(duel.snapshot(), replay.run().unwrap().snapshot());
    }
}
