use pvp_policy::policy::{self, Bot, Style};
use pvp_sim::{Action, Config, Duel, Look, Observation, Outcome, trace::Replay};

#[test]
fn league_g6_matches_shared_policy_fixture() {
    #[derive(serde::Deserialize)]
    struct Decision {
        observation: Observation,
        action: Action,
    }

    let fixture = include_str!("../../../tests/fixtures/policy/league-g6-rust.ndjson");
    let mut bot = Bot::new(Style::LeagueG6);
    for (frame, line) in fixture.lines().enumerate() {
        let decision: Decision = serde_json::from_str(line).unwrap();
        assert_eq!(
            bot.act(&decision.observation),
            decision.action,
            "frame {frame}"
        );
    }
}

#[test]
fn json_replay_reproduces_full_diagnostic_state_and_trace() {
    let mut c = Config {
        trace_capacity: 50_000,
        max_steps: 200,
        ..Config::default()
    };
    c.clients[0].fps = 144;
    c.clients[0].tick_phase_ns = 17_000_000;
    c.clients[1].fps = 37;
    c.clients[1].upstream_ns = 31_000_000;
    c.clients[1].downstream_ns = 7_000_000;
    let mut replay = Replay::new(c.clone(), 123);
    let mut original = Duel::new(c, 123).unwrap();
    for i in 0..200 {
        let actions = original.observations().map(|o| {
            let mut a = policy::baseline(&o);
            a.keys.jump = i % 27 < 9;
            a.keys.left = i % 71 < 17;
            a.keys.sneak = i % 59 < 5;
            if i % 23 == 0 {
                a.look = Look::Turn {
                    yaw_delta: 17.25,
                    pitch_delta: -2.5,
                };
            }
            a
        });
        replay.record(actions);
        original.step(actions).unwrap();
        if original.is_finished() {
            break;
        }
    }
    let json = serde_json::to_string(&replay).unwrap();
    let decoded: Replay = serde_json::from_str(&json).unwrap();
    assert_eq!(replay, decoded);
    let mut restored = decoded.run().unwrap();
    assert_eq!(original.snapshot(), restored.snapshot());
    assert_eq!(original.drain_trace(), restored.drain_trace());
}

#[test]
fn baseline_finishes_by_death_and_seed_controls_same_time_priority() {
    let mut winners = Vec::new();
    for seed in [0, 1] {
        let mut d = Duel::new(Config::default(), seed).unwrap();
        while !d.is_finished() {
            d.step(d.observations().map(|o| policy::baseline(&o)))
                .unwrap();
        }
        let Outcome::Victory { winner } = d.outcome() else {
            panic!("baseline timed out")
        };
        winners.push(winner);
        assert_eq!(d.snapshot().server[1 - winner].health, 0.0);
    }
    assert_ne!(winners[0], winners[1]);
}
