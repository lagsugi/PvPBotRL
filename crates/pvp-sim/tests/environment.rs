use pvp_sim::{Action, Config, Duel, Feature, Keys, Look, ObservationSchema, trace::Replay};

#[test]
fn custom_observation_schema_is_ordered_and_finite() {
    let d = Duel::new(Config::default(), 0).unwrap();
    let observation = d.observe(0).unwrap();
    let schema = ObservationSchema {
        version: 1,
        features: vec![Feature::RelativeZ, Feature::Health, Feature::AttackCharge],
    };
    assert_eq!(schema.project(&observation).unwrap(), vec![20.0, 1.0, 1.0]);
    let schema = ObservationSchema::default();
    assert_eq!(schema.project(&observation).unwrap().len(), 42);
    assert!(
        schema
            .project(&observation)
            .unwrap()
            .iter()
            .all(|x| x.is_finite())
    );
    let bad = ObservationSchema {
        version: 1,
        features: vec![Feature::Health; 2],
    };
    assert!(bad.project(&observation).is_err());
}

#[test]
fn timing_matrix_with_all_keys_stays_finite_and_above_floor() {
    for (fps, delay, phase) in [
        (20, 0, 0),
        (60, 10_000_000, 7_000_000),
        (144, 73_000_000, 29_000_000),
        (37, 230_000_000, 49_000_000),
    ] {
        let mut c = Config {
            max_steps: 500,
            server_phase_ns: phase,
            ..Config::default()
        };
        for t in &mut c.clients {
            t.fps = fps;
            t.upstream_ns = delay;
            t.downstream_ns = delay;
            t.tick_phase_ns = phase;
        }
        let mut d = Duel::new(c, 42).unwrap();
        for i in 0..500 {
            let actions = std::array::from_fn(|p| Action {
                keys: Keys {
                    forward: i % 7 < 5,
                    backward: i % 13 == 0,
                    left: (i + p) % 5 < 2,
                    right: i % 11 < 3,
                    jump: i % 23 < 12,
                    sneak: i % 19 < 5,
                },
                look: if i % 31 < 6 {
                    Look::Turn {
                        yaw_delta: 15.0,
                        pitch_delta: 1.0,
                    }
                } else {
                    Look::TrackTarget
                },
                click: i % 8 == 0,
                attack_held: i % 29 < 3,
            });
            let result = d.step(actions).unwrap();
            for o in &result.observations {
                assert!(
                    ObservationSchema::default()
                        .project(o)
                        .unwrap()
                        .iter()
                        .all(|v| v.is_finite())
                );
            }
            let s = d.snapshot();
            for p in s.server {
                assert!(p.body.position.finite());
                assert!(p.body.velocity.finite());
                assert!(p.body.position.y >= 0.0);
                assert!((0.0..=20.0).contains(&p.health));
            }
            for p in s.clients {
                assert!(p.body.position.y >= 0.0);
            }
            if d.is_finished() {
                break;
            }
        }
    }
}

#[test]
fn latency_changes_state_without_changing_physics_tick_rate() {
    let c = Config::default();
    let mut delayed = c.clone();
    for t in &mut delayed.clients {
        t.upstream_ns = 200_000_000;
        t.downstream_ns = 200_000_000;
    }
    let mut fast = Duel::new(c, 1).unwrap();
    let mut slow = Duel::new(delayed, 1).unwrap();
    let a = Action {
        look: Look::Keep,
        keys: Keys {
            forward: true,
            ..Keys::default()
        },
        ..Action::default()
    };
    for _ in 0..3 {
        fast.step([a; 2]).unwrap();
        slow.step([a; 2]).unwrap();
    }
    assert_eq!(
        fast.snapshot().clients[0].body.position,
        slow.snapshot().clients[0].body.position
    );
    assert_ne!(
        fast.snapshot().server[0].body.position,
        slow.snapshot().server[0].body.position
    );
}

#[test]
fn click_and_hold_are_distinct_and_floor_release_resets_charge() {
    let mut d = Duel::new(Config::default(), 0).unwrap();
    let held = Action {
        look: Look::Angles {
            yaw: 0.0,
            pitch: 90.0,
        },
        click: true,
        attack_held: true,
        ..Action::default()
    };
    d.step([held, Action::default()]).unwrap();
    assert!(d.snapshot().clients[0].body.attack_ticks > 20);
    assert!(d.snapshot().clients[0].destroying_floor);
    d.step([
        Action {
            look: Look::Keep,
            ..Action::default()
        },
        Action::default(),
    ])
    .unwrap();
    assert_eq!(d.snapshot().clients[0].body.attack_ticks, 1);
    assert!(!d.snapshot().clients[0].destroying_floor);
    assert_eq!(d.snapshot().server[0].body.attack_ticks, 22);
}

#[test]
fn observation_detects_walking_from_interpolated_positions() {
    let mut d = Duel::new(Config::default(), 0).unwrap();
    for _ in 0..12 {
        d.step([
            Action::default(),
            Action {
                look: Look::Keep,
                keys: Keys {
                    forward: true,
                    ..Keys::default()
                },
                ..Action::default()
            },
        ])
        .unwrap();
    }
    let o = d.observe(0).unwrap();
    assert!(o.opponent_walking);
    assert!(o.opponent_displacement.length_squared() > 0.0);
    assert_eq!(o.opponent_velocity, pvp_sim::math::Vec3::ZERO);
}

#[test]
fn rejects_unknown_config_and_replay_versions() {
    assert!(serde_json::from_str::<Config>(r#"{"ping":20}"#).is_err());
    let mut replay = Replay::new(Config::default(), 0);
    replay.version = 99;
    assert!(replay.run().is_err());
}
#[test]
fn fall_damage_bypasses_armor_but_keeps_protection() {
    let mut config = Config::default();
    config.spawns[0].position.y = 5.0;
    let mut d = Duel::new(config, 0).unwrap();
    for _ in 0..50 {
        d.step([Action::default(); 2]).unwrap();
    }
    let s = d.snapshot();
    assert!(s.server[0].body.on_ground);
    assert!((s.server[0].health - 19.28).abs() < 1e-5);
    assert_eq!(s.server[0].equipment.armor, [363, 528, 495, 429]);
}
