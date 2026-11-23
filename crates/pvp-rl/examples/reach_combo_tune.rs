use pvp_policy::policy::baseline;
use pvp_sim::{Action, Config, Duel, Keys, Outcome};
use rayon::prelude::*;
fn main() {
    let mut cases = vec![];
    for forward in [1., 2., 2.5, 2.9, 3.2] {
        for back in [0., 2., 2.8] {
            for hold in [0, 1, 2, 3, 4, 6, 8, 10, 12, 16, 20] {
                for charge in [0.2, 0.5, 0.7, 0.8, 0.85, 0.9, 0.93, 0.99] {
                    for side in [0, 1, 2, 3, 4, 5, 6, 7, 8] {
                        for retreat in [0, 1, 2, 3] {
                            cases.push((forward, back, hold, charge, side, retreat));
                        }
                    }
                }
            }
        }
    }
    let mut wins: Vec<_> = cases
        .into_par_iter()
        .map(|(forward, back, hold, charge, side, retreat)| {
            let mut d = Duel::new(
                Config {
                    initial_health: [10., 20.],
                    ..Config::default()
                },
                42,
            )
            .unwrap();
            let mut last = None;
            let mut active = false;
            while !d.is_finished() {
                let o = d.observations();
                let r = &o[0];
                active |= o[1].health < 20.;
                let tick = r.time_ns / pvp_sim::TICK_NS;
                let mut keys = Keys {
                    forward: r.eye_to_target > forward && r.eye_to_target >= back,
                    backward: r.eye_to_target < back,
                    ..Keys::default()
                };
                if last.is_some_and(|t| tick - t <= hold) {
                    keys.forward = false;
                    keys.backward = retreat == 1;
                }
                let mut click = r.eye_to_target < 2.99 && r.attack_charge > charge;
                if last.is_some() {
                    click &= match side {
                        5 => r.opponent_rising,
                        6 => r.opponent_falling,
                        7 => r.opponent_on_ground,
                        8 => r.falling,
                        _ => true,
                    };
                }
                if retreat < 3 && click && !r.sprinting {
                    keys.forward = true;
                    keys.backward = false;
                    click = false;
                }
                if click {
                    keys.forward = retreat == 2;
                    keys.backward = retreat == 1;
                    last = Some(tick);
                }
                if r.horizontal_distance < 5. && side > 0 {
                    let left = match side {
                        1 => true,
                        2 => tick % 10 < 5,
                        3 => tick % 20 < 10,
                        _ => tick % 30 < 15,
                    };
                    keys.left = left;
                    keys.right = !left;
                }
                let mut b = baseline(&o[1]);
                b.click = active && o[1].attack_charge >= 1. && o[1].eye_to_target < 3.;
                b.keys.forward = o[1].horizontal_distance > 2.4;
                b.keys.backward = o[1].horizontal_distance < 1.5;
                let look = pvp_sim::Look::TrackTarget;
                d.step([
                    Action {
                        keys,
                        click,
                        look,
                        ..Action::default()
                    },
                    b,
                ])
                .unwrap();
            }
            (
                d.snapshot().server[0].health - d.snapshot().server[1].health,
                forward,
                back,
                hold,
                charge,
                side,
                retreat,
                d.outcome() == Outcome::Victory { winner: 0 },
            )
        })
        .collect();
    wins.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"candidates":wins.len(),"winning_candidates":wins.iter().filter(|c|c.7).count(),"best_hp_difference":wins.first().map(|c|c.0),"reach_hp":10,"baseline_hp":20,"baseline_reach":3.0,"baseline_waits_for_first_hit":true,"seed":42,"jump_allowed":false,"strafe_allowed":true,"scope":"candidate screening; one seed, seat A, default client order; not a proof of impossibility"})).unwrap());
}
