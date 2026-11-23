use pvp_sim::{Action, Config, Duel, Keys, trace::TraceEvent};

#[test]
fn diagnostic_phase_snapshots_do_not_change_dynamics() {
    let config = Config {
        trace_capacity: 100_000,
        trace_states: true,
        ..Config::default()
    };
    let mut measured = Duel::new(config.clone(), 17).unwrap();
    let mut ordinary = Duel::new(
        Config {
            trace_capacity: 0,
            trace_states: false,
            ..config
        },
        17,
    )
    .unwrap();
    for i in 0..50 {
        let input = [Action {
            keys: Keys {
                forward: true,
                left: i % 3 == 0,
                jump: i % 17 == 0,
                ..Keys::default()
            },
            click: i % 13 == 0,
            ..Action::default()
        }; 2];
        assert_eq!(measured.step(input).unwrap(), ordinary.step(input).unwrap());
        assert_eq!(measured.snapshot(), ordinary.snapshot());
    }
    let records = measured.drain_trace();
    assert!(
        records.iter().any(
            |r| matches!(&r.event, TraceEvent::State { phase, .. } if phase == "after_upstream")
        )
    );
    assert!(records.iter().any(
        |r| matches!(&r.event, TraceEvent::State { phase, .. } if phase == "after_client_tick")
    ));
    assert_eq!(measured.dropped_trace_records(), 0);
}
#[test]
fn absent_server_rules_field_keeps_legacy_replays() {
    use pvp_sim::{Config, config::ServerRules};

    let profile = Config {
        server_rules: ServerRules::PaperFlatV1,
        ..Config::default()
    };
    let mut old = serde_json::to_value(&profile).unwrap();
    assert_eq!(old["server_rules"], "paper_flat_v1");
    old.as_object_mut().unwrap().remove("server_rules");
    let config: Config = serde_json::from_value(old).unwrap();
    assert_eq!(config.server_rules, ServerRules::Legacy);
}

#[test]
fn measured_paper_attack_replay() {
    use pvp_sim::{config::ServerRules, sim::server_replay, trace::TraceEvent};

    let charged =
        include_str!("../../../tests/fixtures/parity/combat/server_schedule/charged.json");
    let measured: server_replay::Capture = serde_json::from_str(charged).unwrap();
    assert_eq!(measured.config.server_rules, ServerRules::PaperFlatV1);
    let replay = server_replay::run(measured).unwrap();
    let attack = replay
        .iter()
        .find(|row| {
            row.trace
                .iter()
                .any(|r| matches!(r.event, TraceEvent::Attack { .. }))
        })
        .unwrap();
    let result = attack
        .trace
        .iter()
        .find_map(|r| match &r.event {
            TraceEvent::Attack { result, .. } => Some(result),
            _ => None,
        })
        .unwrap();
    assert!(result.damage.accepted);
    assert!((f64::from(result.damage.final_damage) - 0.6803998947143555).abs() <= 1e-6);
    assert_eq!((result.motion.unwrap().y - 0.36080000519752503).abs(), 0.0);
    assert_eq!(attack.server[1].health, 19.319599151611328_f64 as f32);
}
