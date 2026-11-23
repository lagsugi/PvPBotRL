//! The isolated charged-hit arithmetic used by the live single-hit probe.
use pvp_sim::{combat, config::Spawn, math::Vec3, rng::JavaRandom, state::ServerPlayer};

fn main() {
    let mut attacker = ServerPlayer::new(
        Spawn {
            position: Vec3::ZERO,
            yaw: 0.,
            pitch: 0.,
        },
        0.,
    );
    let mut victim = ServerPlayer::new(
        Spawn {
            position: Vec3::new(0., 0., 2.5),
            yaw: 180.,
            pitch: 0.,
        },
        0.,
    );
    let result = combat::attack(&mut attacker, &mut victim, &mut JavaRandom::new(0), false);
    println!(
        "{}",
        serde_json::json!({"schema":1,"scope":"charged-hit-arithmetic-only",
        "result":result,"health_after":victim.health,"sword_after":attacker.equipment.sword,
        "armor_after":victim.equipment.armor})
    );
}
