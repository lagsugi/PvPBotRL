use pvp_policy::policy::{Bot, Style};
use pvp_sim::{
    Action, Config, Duel, Outcome, TICK_NS,
    trace::{Replay, TraceEvent},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Capture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    rtt_ms: u64,
    models: [String; 2],
    winner: usize,
    terminal_tick: u64,
    final_hp: [f32; 2],
    actions_by_seat: Option<[String; 2]>,
    accepted_attacks: Option<Vec<(u64, usize, bool, f32)>>,
}

fn action_code(action: Action) -> char {
    let k = action.keys;
    let value = [k.forward, k.backward, k.left, k.right, k.jump, action.click]
        .into_iter()
        .enumerate()
        .map(|(bit, enabled)| usize::from(enabled) << bit)
        .sum::<usize>();
    char::from(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"[value])
}

#[test]
fn paper_practice_matches_captured_actions_attacks_and_results() {
    let capture: Capture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/parity/g3-paper-citizens.json"
    ))
    .unwrap();
    let practice: Config = serde_json::from_str(include_str!(
        "../../../configs/paper_citizens_practice.json"
    ))
    .unwrap();
    for case in capture.cases {
        let label = format!("{} ms, {:?}", case.rtt_ms, case.models);
        let mut config = practice.clone();
        config.expose_opponent_health = true;
        config.trace_capacity = 256;
        for client in &mut config.clients {
            client.upstream_ns = case.rtt_ms * 500_000;
            client.downstream_ns = client.upstream_ns;
        }
        let seed = 938765123;
        let mut duel = Duel::new(config.clone(), seed).unwrap();
        let mut replay = Replay::new(config, seed);
        let mut bots = case
            .models
            .each_ref()
            .map(|name| Bot::new(name.parse::<Style>().unwrap()));
        let mut decisions = [String::new(), String::new()];
        let mut attacks = Vec::new();
        while !duel.is_finished() {
            let mut codes = ['.'; 2];
            let (_, actions) = duel
                .step_with_policy(|duel, seat, obs| {
                    let action = bots[seat].act_in_duel(duel, seat, obs, None)?;
                    codes[seat] = action_code(action);
                    Ok(action)
                })
                .unwrap();
            for (seq, code) in decisions.iter_mut().zip(codes) {
                seq.push(code);
            }
            replay.record(actions);
            for entry in duel.drain_trace() {
                if let TraceEvent::Attack { attacker, result } = entry.event
                    && result.damage.accepted
                {
                    attacks.push((
                        entry.time_ns / TICK_NS,
                        attacker,
                        result.critical,
                        result.damage.final_damage,
                    ));
                }
            }
        }
        assert_eq!(
            duel.outcome(),
            Outcome::Victory {
                winner: case.winner
            },
            "{label}"
        );
        assert_eq!(duel.decision_step() - 1, case.terminal_tick, "{label}");
        for (a, b) in duel.server_health().into_iter().zip(case.final_hp) {
            assert!((a - b).abs() < 1e-5, "{label} final HP {a} != {b}");
        }
        if let Some(expected) = case.actions_by_seat {
            for (seat, (actual, expected)) in decisions.iter().zip(expected).enumerate() {
                assert_eq!(
                    actual.len(),
                    expected.len(),
                    "{label} seat {seat} decision count"
                );
                for (tick, (a, b)) in actual.bytes().zip(expected.bytes()).enumerate() {
                    assert_eq!(a, b, "{label} decision {tick}, seat {seat}");
                }
            }
        }
        if let Some(expected) = case.accepted_attacks {
            attacks.sort_by_key(|&(tick, seat, _, _)| (tick, seat));
            assert_eq!(
                attacks.len(),
                expected.len(),
                "{label} accepted attack count"
            );
            for ((tick, seat, crit, damage), (pt, ps, pc, pd)) in attacks.into_iter().zip(expected)
            {
                assert_eq!((tick, seat, crit), (pt, ps, pc), "{label} accepted attack");
                assert!(
                    (damage - pd).abs() < 1e-5,
                    "{label} tick {tick}, seat {seat} damage"
                );
            }
        }
        assert_eq!(
            duel.snapshot(),
            replay.run().unwrap().snapshot(),
            "{label} replay"
        );
    }
}
