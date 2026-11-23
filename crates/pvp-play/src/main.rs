#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod audio;
mod clock;
mod game;
mod hud;
mod model;
mod particles;
mod recording;
mod render;
mod storage;

use clock::FixedClock;
use game::Game;
use macroquad::prelude::*;
use pvp_policy::policy::Style;
use pvp_sim::{Config, Keys};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Options {
    config: Config,
    seed: u64,
    record_root: PathBuf,
    opponent: Style,
    smoke: Option<PathBuf>,
    spectate: bool,
    bot_a: Style,
    recurrent_model: Option<PathBuf>,
}
fn options() -> Result<Options, String> {
    let mut o = Options {
        config: Config {
            ping_profile: Some(pvp_sim::ping::PingProfile {
                fixed_ms: [Some(20.), None],
                ..Default::default()
            }),
            ..Default::default()
        },
        seed: 42,
        opponent: Style::Critical,
        record_root: storage::default_record_root().map_err(|e| e.to_string())?,
        smoke: None,
        spectate: false,
        bot_a: Style::Critical,
        recurrent_model: None,
    };
    let mut args = std::env::args().skip(1);
    let mut human_ping = None;
    let mut bot_ping = None;
    let mut asymmetry = None;
    let mut profile_mode = None;
    while let Some(flag) = args.next() {
        if flag == "--spectate" {
            o.spectate = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        match flag.as_str() {
            "--human-ping" => {
                human_ping = Some(value.parse::<f64>().map_err(|_| "Invalid human ping")?)
            }
            "--bot-ping" => bot_ping = Some(value.parse::<f64>().map_err(|_| "Invalid bot ping")?),
            "--ping-asymmetry" => {
                asymmetry = Some(value.parse::<f64>().map_err(|_| "Invalid asymmetry")?)
            }
            "--ping-profile" => profile_mode = Some(value),
            "--config" => {
                o.config = serde_json::from_str(
                    &std::fs::read_to_string(value).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
            }
            "--seed" => o.seed = value.parse().map_err(|_| "Invalid seed")?,
            "--record-dir" => o.record_root = PathBuf::from(value),
            "--opponent" => o.opponent = value.parse()?,
            "--bot-a" => {
                o.bot_a = value.parse()?;
                o.spectate = true;
            }
            "--smoke-test" => o.smoke = Some(PathBuf::from(value)),
            "--recurrent-model" => o.recurrent_model = Some(PathBuf::from(value)),
            _ => {
                return Err(format!(
                    "Unknown option {flag}. Use --spectate, --bot-a NAME, --opponent NAME, --config FILE, --seed N, --record-dir DIR, or --smoke-test IMAGE.png"
                ));
            }
        }
    }
    if let Some(mode) = profile_mode {
        o.config.ping_profile = match mode.as_str() {
            "gamma" => Some(pvp_sim::ping::PingProfile {
                fixed_ms: [Some(20.), None],
                ..Default::default()
            }),
            "fixed" => None,
            _ => return Err("--ping-profile must be gamma or fixed".into()),
        };
    }
    if let Some(a) = asymmetry {
        o.config
            .ping_profile
            .as_mut()
            .ok_or("--ping-asymmetry requires gamma profile")?
            .asymmetry = a;
    }
    for (seat, value) in [human_ping, bot_ping].into_iter().enumerate() {
        if let Some(ms) = value {
            if !ms.is_finite() || !(0.0..=10000.).contains(&ms) {
                return Err("Ping must be 0..10000 ms".into());
            }
            if let Some(p) = &mut o.config.ping_profile {
                p.fixed_ms[seat] = Some(ms);
            } else {
                o.config.clients[seat].upstream_ns = (ms * 500000.).round() as u64;
                o.config.clients[seat].downstream_ns =
                    (ms * 1e6).round() as u64 - o.config.clients[seat].upstream_ns;
            }
        }
    }
    o.config.validate().map_err(|e| e.to_string())?;
    Ok(o)
}
fn window_conf() -> Conf {
    Conf {
        window_title: "Sword PvP | Rust Duel".into(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        sample_count: 4,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            swap_interval: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Focus guard uses only read-only Win32 calls. No global input hooks.
#[cfg(windows)]
fn focused() -> bool {
    use winapi::um::winuser::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0;
    // SAFETY: the window handle comes from Windows; the output pointer is a
    // valid local u32 for the duration of this synchronous call.
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
    }
    pid == std::process::id()
}
#[cfg(not(windows))]
fn focused() -> bool {
    true
}
fn capture(grab: bool) {
    set_cursor_grab(grab);
    show_mouse(!grab);
}
fn update_keys(keys: &mut Keys) {
    // Only edges after resume affect held controls, so Alt-Tab cannot leave
    // stale library key-down state moving the human after returning.
    for (codes, state) in [
        (&[KeyCode::W][..], &mut keys.forward),
        (&[KeyCode::S][..], &mut keys.backward),
        (&[KeyCode::A][..], &mut keys.left),
        (&[KeyCode::D][..], &mut keys.right),
        (&[KeyCode::Space][..], &mut keys.jump),
        (
            &[KeyCode::LeftShift, KeyCode::RightShift][..],
            &mut keys.sneak,
        ),
    ] {
        if codes
            .iter()
            .any(|&k| is_key_pressed(k) || is_key_released(k))
        {
            *state = codes.iter().any(|&k| is_key_down(k));
        }
    }
}
#[macroquad::main(window_conf)]
async fn main() {
    // Release builds have no console when opened from Explorer. Persist panics
    // as well as ordinary errors so a disappearing window is diagnosable.
    std::panic::set_hook(Box::new(|info| {
        let root = storage::default_record_root()
            .unwrap_or_else(|_| std::env::temp_dir().join("pvp-play"));
        storage::report_error(
            &root,
            &format!(
                "Unexpected crash: {info}\n{}",
                std::backtrace::Backtrace::force_capture()
            ),
        );
    }));
    prevent_quit();
    if let Err(message) = run().await {
        capture(false);
        let root = storage::default_record_root()
            .unwrap_or_else(|_| std::env::temp_dir().join("pvp-play"));
        let message = match storage::report_error(&root, &message) {
            Some(path) => format!("{message}\nError log: {}", path.display()),
            None => message,
        };
        let lines: Vec<String> = message
            .lines()
            .flat_map(|line| {
                line.chars()
                    .collect::<Vec<_>>()
                    .chunks(90)
                    .map(|part| part.iter().collect())
                    .collect::<Vec<String>>()
            })
            .collect();
        loop {
            clear_background(Color::from_rgba(16, 25, 29, 255));
            draw_text("Unable to continue", 35.0, 65.0, 32.0, WHITE);
            for (i, line) in lines.iter().enumerate() {
                draw_text(line, 35.0, 110.0 + i as f32 * 22.0, 18.0, ORANGE);
            }
            draw_text(
                "Close this window or press Escape.",
                35.0,
                screen_height() - 35.0,
                20.0,
                WHITE,
            );
            if is_key_pressed(KeyCode::Escape) || is_quit_requested() {
                break;
            }
            next_frame().await;
        }
    }
}
async fn run() -> Result<(), String> {
    let options = options()?;
    let recurrent = options
        .recurrent_model
        .as_deref()
        .map(pvp_policy::recurrent::Model::load)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut game = Game::new(
        options.config,
        options.seed,
        options.record_root,
        options.opponent,
    )
    .map_err(|e| e.to_string())?;
    game.recurrent_model = recurrent.map(std::sync::Arc::new);
    if options.spectate {
        game.toggle_spectating();
        game.select_bot_a(options.bot_a);
    }
    let mut settings = render::Settings::default();
    let mut hud = hud::Hud::new()?;
    let mut models = model::Models::new();
    let audio = audio::CombatAudio::load().await;
    let mut finished_at: Option<Instant> = None;
    let mut clock = FixedClock::default();
    let mut last = Instant::now();
    let mut keys = Keys::default();
    let mut held = false;
    let mut ignore_mouse = 0;
    let mut smoke_frames = 0;
    let mut fullscreen = false;
    if options.smoke.is_some() {
        game.resume();
    }
    loop {
        let now = Instant::now();
        let mut elapsed = now.duration_since(last);
        last = now;
        if is_quit_requested() || (game.paused && is_key_pressed(KeyCode::Q)) {
            game.save("quit");
            capture(false);
            break;
        }
        if is_key_pressed(KeyCode::F11) {
            fullscreen = !fullscreen;
            set_fullscreen(fullscreen);
            // Resizing/cursor recentering must not become a mouse-look jump.
            ignore_mouse = 2;
            clock.reset();
            last = Instant::now();
            elapsed = Duration::ZERO;
        }
        if !game.paused && options.smoke.is_none() && !focused() {
            game.pause("Window focus lost");
            capture(false);
            clock.reset();
            keys = Keys::default();
            held = false;
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::F1) {
            game.pause("Paused");
            capture(false);
            clock.reset();
            keys = Keys::default();
            held = false;
        }
        if is_key_pressed(KeyCode::R) {
            game.restart().map_err(|e| e.to_string())?;
            capture(false);
            clock.reset();
            keys = Keys::default();
            held = false;
        }
        if game.paused && !game.started && focused() {
            if is_key_pressed(KeyCode::V) {
                game.toggle_spectating();
            }
            let direction_a =
                is_key_pressed(KeyCode::D) as isize - is_key_pressed(KeyCode::A) as isize;
            if direction_a != 0 {
                game.cycle_bot_a(direction_a);
            }
            let direction =
                is_key_pressed(KeyCode::Right) as isize - is_key_pressed(KeyCode::Left) as isize;
            if direction != 0 {
                game.cycle_opponent(direction);
            }
            let ping_change =
                is_key_pressed(KeyCode::Up) as i32 - is_key_pressed(KeyCode::Down) as i32;
            if ping_change != 0 && !game.spectating {
                let increment =
                    if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                        10.
                    } else {
                        1.
                    };
                game.change_human_ping(f64::from(ping_change) * increment)
                    .map_err(|e| e.to_string())?;
            }
            if is_key_pressed(KeyCode::N) {
                game.restart().map_err(|e| e.to_string())?;
            }
        }
        if is_key_pressed(KeyCode::Tab) {
            game.switch_view();
        }
        if game.paused && !game.duel.is_finished() && is_key_pressed(KeyCode::Enter) && focused() {
            game.resume();
            clock.reset();
            capture(!game.spectating);
            ignore_mouse = 2;
            keys = Keys::default();
            held = false;
            last = Instant::now();
            elapsed = Duration::ZERO;
        }
        if is_key_pressed(KeyCode::LeftBracket) {
            settings.sensitivity = (settings.sensitivity - 0.025).max(0.025);
        }
        if is_key_pressed(KeyCode::RightBracket) {
            settings.sensitivity = (settings.sensitivity + 0.025).min(1.0);
        }
        if is_key_pressed(KeyCode::Minus) {
            settings.fov = (settings.fov - 5.0).max(40.0);
        }
        if is_key_pressed(KeyCode::Equal) {
            settings.fov = (settings.fov + 5.0).min(110.0);
        }
        if is_key_pressed(KeyCode::F2) {
            settings.smooth = !settings.smooth;
        }
        if !game.paused {
            if game.spectating {
                // Both actions and aim come from the bots while observing.
            } else if options.smoke.is_none() {
                update_keys(&mut keys);
                if is_mouse_button_pressed(MouseButton::Left) {
                    game.pending_click = true;
                    held = true;
                }
                if is_mouse_button_released(MouseButton::Left) {
                    held = false;
                }
                if ignore_mouse > 0 {
                    ignore_mouse -= 1;
                } else {
                    let delta = mouse_delta_position();
                    game.look(
                        -delta.x * screen_width() * 0.5,
                        -delta.y * screen_height() * 0.5,
                        settings.sensitivity,
                    );
                }
            } else {
                keys = Keys {
                    forward: game.duel.decision_step() < 12,
                    ..Default::default()
                };
                game.pending_click = game.duel.decision_step() % 13 == 0;
            }
            match clock.advance(elapsed) {
                Ok(ticks) => {
                    for _ in 0..ticks {
                        game.tick(keys, held).map_err(|e| e.to_string())?;
                        if game.paused {
                            capture(false);
                            clock.reset();
                            keys = Keys::default();
                            held = false;
                            break;
                        }
                    }
                }
                Err(()) => {
                    game.pause("Rendering stalled. Resume when ready.");
                    capture(false);
                    keys = Keys::default();
                    held = false;
                }
            }
        }
        let dt = elapsed.as_secs_f32().min(0.1);
        if !game.paused || game.duel.is_finished() {
            game.particles.advance(dt, game.current);
            game.click_flash = game.click_flash.map(|t| (t - dt).max(0.));
        }
        // The final hit can still be in flight when a lethal server tick
        // ends the simulation. Let that sound arrive on the result screen.
        let audio_time = if game.duel.is_finished() {
            let end = *finished_at.get_or_insert(now);
            Some(game.duel.time_ns() + now.duration_since(end).as_nanos() as u64)
        } else {
            finished_at = None;
            (!game.paused)
                .then(|| game.duel.time_ns() + (clock.alpha() * pvp_sim::TICK_NS as f64) as u64)
        };
        if let Some(time) = audio_time {
            while game.sounds.front().is_some_and(|s| s.time_ns <= time) {
                let event = game.sounds.pop_front().unwrap();
                let source = event.source ^ game.view_seat;
                if event.sound == pvp_sim::sim::CombatSound::Critical {
                    game.particles.critical_hit(source, game.current);
                }
                if source == 1
                    && matches!(
                        event.sound,
                        pvp_sim::sim::CombatSound::Hurt | pvp_sim::sim::CombatSound::Death
                    )
                {
                    // Use the already delayed, client-visible hurt event;
                    // do not inspect authoritative opponent health.
                    game.opponent_damage_flash = 0.5;
                }
                if source == 1
                    && matches!(
                        event.sound,
                        pvp_sim::sim::CombatSound::Weak
                            | pvp_sim::sim::CombatSound::Strong
                            | pvp_sim::sim::CombatSound::Critical
                            | pvp_sim::sim::CombatSound::Sweep
                            | pvp_sim::sim::CombatSound::NoDamage
                    )
                    && game.opponent_swing <= 0.15
                {
                    game.opponent_swing = 0.3;
                }
                let distance = if source == 0 {
                    0.0
                } else {
                    (game.current.opponent.position - game.current.local.position)
                        .length_squared()
                        .sqrt() as f32
                };
                audio.play(event.sound, distance);
            }
        }
        game.damage_flash = (game.damage_flash - dt).max(0.0);
        if !game.paused || game.duel.is_finished() {
            game.opponent_damage_flash = (game.opponent_damage_flash - dt).max(0.0);
            game.opponent_swing = (game.opponent_swing - dt).max(0.0);
        }
        render::draw(&game, &mut hud, &mut models, clock.alpha(), &settings);
        if let Some(path) = &options.smoke {
            smoke_frames += 1;
            if smoke_frames >= 120 {
                if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                get_screen_data().export_png(&path.to_string_lossy());
                game.save("smoke_test");
                break;
            }
        }
        next_frame().await;
        let frame_time = last.elapsed();
        if frame_time < Duration::from_millis(2) {
            std::thread::sleep(Duration::from_millis(2) - frame_time);
        }
    }
    Ok(())
}
