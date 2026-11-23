use crate::{game::Game, hud::Hud, model::Models};
use macroquad::prelude::*;
use pvp_sim::{
    math::Vec3 as SimVec,
    observation::{Pick, pick},
};

const MUTED: Color = Color::new(0.65, 0.74, 0.74, 1.0);
fn v(p: SimVec) -> Vec3 {
    vec3(p.x as f32, p.y as f32, p.z as f32)
}
fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(text, x, y, size, color);
}
fn centered(text: &str, y: f32, size: f32, color: Color) {
    let m = measure_text(text, None, size as u16, 1.0);
    label(text, (screen_width() - m.width) / 2.0, y, size, color);
}

pub struct Settings {
    pub sensitivity: f32,
    pub fov: f32,
    pub smooth: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: 0.15,
            fov: 100.0,
            smooth: true,
        }
    }
}

pub fn draw(game: &Game, hud: &mut Hud, models: &mut Models, alpha: f64, settings: &Settings) {
    clear_background(Color::from_rgba(158, 197, 216, 255));
    let a = if settings.smooth && !game.paused {
        alpha
    } else {
        1.0
    };
    let mut local = game.current.local;
    local.position =
        game.previous.local.position + (local.position - game.previous.local.position) * a;
    let mut remote = game.current.opponent;
    remote.position =
        game.previous.opponent.position + (remote.position - game.previous.opponent.position) * a;
    local.yaw = game.yaw;
    local.pitch = game.pitch;
    let eye = v(local.eye());
    let pitch = game.pitch.clamp(-89.99, 89.99).to_radians();
    let yaw = game.yaw.to_radians();
    let forward = vec3(
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    );
    set_camera(&Camera3D {
        position: eye,
        target: eye + forward,
        up: Vec3::Y,
        fovy: settings.fov.to_radians(),
        z_near: 0.025,
        z_far: 180.0,
        ..Default::default()
    });
    floor(local.position, game.duel.config().floor_y);
    let targeted = pick(local, remote, game.duel.config().floor_y) == Pick::Entity;
    models.opponent(game, remote, a as f32);
    let bounds = remote.bounds();
    let center = v((bounds.min + bounds.max) * 0.5);
    let size = v(bounds.max - bounds.min);
    let line = if targeted {
        Color::from_rgba(184, 240, 97, 255)
    } else {
        Color::from_rgba(255, 185, 84, 255)
    };
    draw_cube_wires(center, size, line);
    let remote_eye = v(remote.eye());
    draw_line_3d(
        remote_eye - vec3(0.3, 0.0, 0.0),
        remote_eye + vec3(0.3, 0.0, 0.0),
        Color::from_rgba(255, 242, 204, 255),
    );
    let angle = remote.yaw.to_radians();
    draw_line_3d(
        remote_eye,
        remote_eye + vec3(-angle.sin(), 0.0, angle.cos()) * 0.55,
        line,
    );
    game.particles.draw(forward);
    models.hand(game, a as f32);
    set_default_camera();
    if game.damage_flash > 0.0 {
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            Color::new(1., 0., 0., 0.12 * (game.damage_flash / 0.25).min(1.)),
        );
    }
    hud.draw(
        &game.observation,
        game.current,
        targeted,
        game.damage_flash > 0.12,
    );
    if game.spectating {
        crate::hud::draw_keystrokes(game);
    }
    if game.paused {
        overlay(game);
    } else if game.recording_error.is_some() {
        label("REC OFF", 10.0, 20.0, 16.0, ORANGE);
    }
}
fn floor(pos: SimVec, height: f64) {
    let cx = pos.x.floor() as f32;
    let cz = pos.z.floor() as f32;
    let y = height as f32;
    draw_plane(
        vec3(cx, y, cz),
        vec2(256.0, 256.0),
        None,
        Color::from_rgba(83, 140, 70, 255),
    );
    for i in -64..=64 {
        let x = cx + i as f32;
        let z = cz + i as f32;
        let color_x = if (x as i32).rem_euclid(5) == 0 {
            Color::from_rgba(45, 97, 49, 255)
        } else {
            Color::from_rgba(63, 117, 57, 255)
        };
        let color_z = if (z as i32).rem_euclid(5) == 0 {
            Color::from_rgba(45, 97, 49, 255)
        } else {
            Color::from_rgba(63, 117, 57, 255)
        };
        draw_line_3d(
            vec3(x, y + 0.002, cz - 64.0),
            vec3(x, y + 0.002, cz + 64.0),
            color_x,
        );
        draw_line_3d(
            vec3(cx - 64.0, y + 0.002, z),
            vec3(cx + 64.0, y + 0.002, z),
            color_z,
        );
    }
}
fn overlay(game: &Game) {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0., 0., w, h, Color::new(0., 0., 0., 0.48));
    let y = h * 0.4;
    centered(game.outcome_title(), y, 32., WHITE);
    let name = game.opponent().name();
    let opponent = if game.spectating {
        format!("A: {}       B: {}", game.bot_a_style().name(), name)
    } else if game.started {
        format!("vs  {name}")
    } else {
        format!("<   {name}   >")
    };
    centered(&opponent, y + 35., 23., WHITE);
    let c = &game.duel.config().clients;
    let ms = |v: u64| v as f64 / 1e6;
    centered(
        &format!(
            "{} {:.1} ms (up {:.1} / down {:.1})    B {:.1} ms (up {:.1} / down {:.1})",
            if game.spectating { "A" } else { "YOU" },
            ms(c[0].upstream_ns + c[0].downstream_ns),
            ms(c[0].upstream_ns),
            ms(c[0].downstream_ns),
            ms(c[1].upstream_ns + c[1].downstream_ns),
            ms(c[1].upstream_ns),
            ms(c[1].downstream_ns)
        ),
        y + 132.,
        17.,
        WHITE,
    );
    if !game.started {
        centered(
            if game.spectating {
                "N: Draw new ping"
            } else {
                "Up/Down: Your ping +/-1 ms (Shift: 10 ms)    N: Draw new bot ping"
            },
            y + 158.,
            17.,
            MUTED,
        );
    }
    let controls = if game.duel.is_finished() {
        "R   Rematch       Q   Quit"
    } else if game.started {
        "Enter   Resume       R   Restart       Q   Quit"
    } else if game.spectating {
        "Enter   Watch       Q   Quit"
    } else {
        "Enter   Play       Q   Quit"
    };
    centered(controls, y + 69., 20., WHITE);
    if !game.started {
        centered(
            if game.spectating {
                "V: Human / Spectate    A/D: Bot A    Left/Right: Bot B    Tab: POV"
            } else {
                "V: Spectate bots    Left/Right: Opponent"
            },
            y + 99.,
            17.,
            MUTED,
        );
    }
    if game.spectating {
        centered("SPECTATE  |  BOT vs BOT", y - 38., 19., MUTED);
    }
    if game.pause_reason == "Window focus lost"
        || game.pause_reason.starts_with("Rendering stalled")
    {
        centered(&game.pause_reason, y + 99., 16., MUTED);
    }
    if game.recording_error.is_some() {
        centered(&game.status, h - 12., 15., ORANGE);
    }
}
