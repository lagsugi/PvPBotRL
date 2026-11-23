//! Ground-only Reach at 10 HP against a pursuing 20 HP Baseline.
//! Args: match count (default 10000), Baseline reach (default 2.0), optional
//! "wait" to disable Baseline attacks until its own received health first drops.
use pvp_policy::policy::{Bot, Style, baseline};
use pvp_sim::{Config, Duel, Outcome, rng::derive_seed};
use rayon::prelude::*;
use std::time::Instant;

fn main() {
    let count = std::env::args()
        .nth(1)
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(10000);
    let reach = std::env::args()
        .nth(2)
        .map(|s| s.parse::<f64>().unwrap())
        .unwrap_or(2.0);
    assert!(count > 0 && reach.is_finite() && (1.5..=3.0).contains(&reach));
    let forward_distance = reach - 0.55;
    let wait_for_hit = std::env::args().nth(3).as_deref() == Some("wait");
    let backward_distance = reach - 1.45;
    let start = Instant::now();
    let results: Vec<_> = (0..count)
        .into_par_iter()
        .map(|i| {
            let seat = i % 2;
            let first = (i / 2) % 2;
            let mut config = Config {
                first_client: Some(first),
                ..Config::default()
            };
            config.initial_health[seat] = 10.;
            let mut duel = Duel::new(config, derive_seed(42, i as u64)).unwrap();
            assert_eq!(duel.observe(seat).unwrap().health, 10.);
            let mut bot = Bot::new(Style::Reach);
            let mut activated = !wait_for_hit;
            while !duel.is_finished() {
                let obs = duel.observations();
                activated |= obs[1 - seat].health < 20.;
                let actions = std::array::from_fn(|p| {
                    if p == seat {
                        let action = bot.act(&obs[p]);
                        assert!(!action.keys.jump && !action.keys.left && !action.keys.right);
                        action
                    } else {
                        let mut action = baseline(&obs[p]);
                        action.click =
                            activated && obs[p].attack_charge >= 1. && obs[p].eye_to_target < reach;
                        // Move the original Baseline's preferred distance inward by
                        // the reach reduction, so it actually pursues into range.
                        action.keys.forward = obs[p].horizontal_distance > forward_distance;
                        action.keys.backward = obs[p].horizontal_distance < backward_distance;
                        action
                    }
                });
                duel.step(actions).unwrap();
            }
            let hp = duel.snapshot().server[seat].health;
            (
                duel.outcome() == Outcome::Victory { winner: seat },
                duel.outcome() == Outcome::Truncated,
                hp,
            )
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "matches":count,"reach_start_hp":10,"baseline_start_hp":20,"baseline_reach":reach,
            "baseline_waits_for_received_first_hit":wait_for_hit,
            "baseline_forward_distance":forward_distance,"baseline_backward_distance":backward_distance,
            "reach_wins":results.iter().filter(|r|r.0).count(),
            "timeouts":results.iter().filter(|r|r.1).count(),
            "reach_losses":results.iter().filter(|r|!r.0 && !r.1).count(),
            "mean_remaining_hp":results.iter().map(|r|r.2 as f64).sum::<f64>()/count as f64,
            "minimum_remaining_hp":results.iter().map(|r|r.2).fold(f32::INFINITY,f32::min),
            "elapsed_seconds":start.elapsed().as_secs_f64(),"seed":42,
            "seats_and_first_client":"alternating across all four combinations",
            "jump_or_strafe_inputs":0
        }))
        .unwrap()
    );
}
