//! Client-only critical-hit effects, based on TrackingEmitter and CritParticle.
//! This RNG and particle clock never modify simulation state or replay actions.
use macroquad::prelude::*;
use pvp_sim::{sim::ClientView, state::Body};

const TICK: f32 = 0.05;
const LIMIT: usize = 512;

struct Particle {
    position: Vec3,
    velocity: Vec3,
    age: u8,
    lifetime: u8,
    brightness: f32,
    size: f32,
}

struct Emitter {
    victim: usize,
    remaining: u8,
}

pub struct CriticalParticles {
    sprite: Image,
    particles: Vec<Particle>,
    emitters: Vec<Emitter>,
    accumulator: f32,
    rng: u64,
}

impl CriticalParticles {
    pub fn new() -> Self {
        Self {
            sprite: Image::from_file_with_format(
                include_bytes!("../assets/hud/copper_spark.png"),
                Some(ImageFormat::Png),
            )
            .expect("embedded critical-hit sprite must be a valid PNG"),
            particles: vec![],
            emitters: vec![],
            accumulator: 0.,
            rng: 0x4352_4954,
        }
    }

    fn random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.rng >> 40) as f32 / (1_u32 << 24) as f32
    }

    pub fn critical_hit(&mut self, attacker: usize, view: ClientView) {
        // CombatSound::Critical is emitted at the attacker, but the visual
        // TrackingEmitter belongs to the attacked entity, for either seat.
        let victim = 1 - attacker;
        self.burst(if victim == 0 {
            view.local
        } else {
            view.opponent
        });
        if self.emitters.len() < 32 {
            self.emitters.push(Emitter {
                victim,
                remaining: 2,
            });
        }
    }

    fn burst(&mut self, body: Body) {
        let center = vec3(
            body.position.x as f32,
            body.position.y as f32,
            body.position.z as f32,
        );
        for _ in 0..16 {
            let direction = vec3(self.random(), self.random(), self.random()) * 2. - Vec3::ONE;
            if direction.length_squared() > 1. {
                continue;
            }
            let position = center
                + vec3(
                    direction.x * 0.15,
                    body.pose.height() as f32 * (0.5 + direction.y * 0.25),
                    direction.z * 0.15,
                );
            let lifetime = (6. / (self.random() * 0.8 + 0.6)).max(1.) as u8;
            let brightness = self.random() * 0.3 + 0.6;
            let size = 0.075 * (0.5 + self.random() * 0.5);
            if self.particles.len() < LIMIT {
                self.particles.push(Particle {
                    position,
                    velocity: (direction + Vec3::Y * 0.2) * 0.4,
                    age: 0,
                    lifetime,
                    brightness,
                    size,
                });
            }
        }
    }

    pub fn advance(&mut self, dt: f32, view: ClientView) {
        self.accumulator += dt.clamp(0., 0.1);
        while self.accumulator >= TICK {
            self.accumulator -= TICK;
            for p in &mut self.particles {
                p.age += 1;
                p.velocity.y -= 0.02; // Particle gravity 0.04 * CritParticle 0.5.
                p.position += p.velocity;
                p.velocity *= 0.7;
            }
            self.particles.retain(|p| p.age < p.lifetime);
            // Move emitters out temporarily to allow the independent visual RNG.
            let mut emitters = std::mem::take(&mut self.emitters);
            for emitter in &mut emitters {
                self.burst(if emitter.victim == 0 {
                    view.local
                } else {
                    view.opponent
                });
                emitter.remaining -= 1;
            }
            emitters.retain(|e| e.remaining > 0);
            self.emitters = emitters;
        }
    }

    pub fn draw(&self, forward: Vec3) {
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward).normalize();
        for p in &self.particles {
            let center = p.position + p.velocity * (self.accumulator / TICK);
            let color = vec3(
                p.brightness,
                p.brightness * 0.96_f32.powi(p.age as i32),
                p.brightness * 0.9_f32.powi(p.age as i32),
            );
            let mut mesh = Mesh {
                vertices: vec![],
                indices: vec![],
                texture: None,
            };
            for y in 0..self.sprite.height as u32 {
                for x in 0..self.sprite.width as u32 {
                    let pixel = self.sprite.get_pixel(x, y);
                    if pixel.a < 0.5 {
                        continue;
                    }
                    let step_x = right * (p.size * 2. / self.sprite.width as f32);
                    let step_y = up * (p.size * 2. / self.sprite.height as f32);
                    let origin = center - right * p.size + up * p.size + step_x * x as f32
                        - step_y * y as f32;
                    let shade =
                        Color::new(pixel.r * color.x, pixel.g * color.y, pixel.b * color.z, 1.);
                    let base = mesh.vertices.len() as u16;
                    mesh.vertices.extend(
                        [
                            origin,
                            origin + step_x,
                            origin + step_x - step_y,
                            origin - step_y,
                        ]
                        .map(|v| Vertex::new2(v, Vec2::ZERO, shade)),
                    );
                    mesh.indices
                        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            draw_mesh(&mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pvp_sim::{Config, Duel};

    #[test]
    fn either_attackers_critical_emits_at_the_victim_and_expires() {
        let duel = Duel::new(Config::default(), 42).unwrap();
        let view = duel.client_view(0).unwrap();
        for attacker in 0..2 {
            let mut fx = CriticalParticles::new();
            fx.critical_hit(attacker, view);
            let victim = if attacker == 0 {
                view.opponent
            } else {
                view.local
            };
            assert!(!fx.particles.is_empty());
            assert!(
                fx.particles
                    .iter()
                    .all(|p| (p.position.z - victim.position.z as f32).abs() < 0.16)
            );
            for _ in 0..20 {
                fx.advance(TICK, view);
            }
            assert!(fx.particles.is_empty());
            assert!(fx.emitters.is_empty());
        }
    }
}
