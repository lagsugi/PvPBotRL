use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use pvp_sim::{Observation, sim::ClientView, state::Equipment};

// Dimensions and anchors are in GUI pixels, following 26.1.2 Gui.
const WIDTH: u16 = 184;
const HEIGHT: u16 = 50;
const HEART: [&str; 9] = [
    ".##.##...",
    "#++##++#.",
    "#+rrrrr#.",
    "#rrrrrr#.",
    ".#rrrr#..",
    "..#rr#...",
    "...##....",
    ".........",
    ".........",
];
const ARMOR: [&str; 9] = [
    ".##..##..",
    "#++##++#.",
    "#+wwww+#.",
    "##wwww##.",
    ".#wwww#..",
    ".#wwww#..",
    ".#wwww#..",
    ".######..",
    ".........",
];
const FOOD: [&str; 9] = [
    "....###..",
    "...#++o#.",
    "..#oooo#.",
    "..#ooo#..",
    "..#oo#...",
    ".#b##....",
    "#bb#.....",
    ".##......",
    ".........",
];
pub(crate) const SWORD: [&str; 16] = [
    ".............###",
    "............#tc#",
    "...........#tc#.",
    "..........#tc#..",
    ".........#tc#...",
    "........#tc#....",
    ".......#tc#.....",
    "......#tc#......",
    "...## #tc#......",
    "...#cc tc#......",
    "....#ccc#.......",
    "....#bcc#.......",
    "...#b#.#c#......",
    "..#b#...##......",
    ".#b#............",
    ".##.............",
];
fn grey(n: u8) -> Color {
    Color::from_rgba(n, n, n, 255)
}

/// Spectator HUD shows the actual last 20 Hz policy action for each bot.
/// CTRL is the always-held sprint key; SPRINT is the resulting client state.
pub fn draw_keystrokes(game: &crate::game::Game) {
    let scale = (screen_width() / 1000.)
        .min(screen_height() / 650.)
        .clamp(0.7, 1.5);
    for seat in 0..2 {
        let width = 156. * scale;
        let x = if seat == 0 {
            14. * scale
        } else {
            screen_width() - width - 14. * scale
        };
        let y = 18. * scale;
        let style = if seat == 0 {
            game.bot_a_style()
        } else {
            game.opponent()
        };
        let name = format!(
            "BOT {}: {}",
            if seat == 0 { "A" } else { "B" },
            style.name()
        );
        let watched = seat == game.view_seat;
        draw_rectangle(
            x - 5. * scale,
            y - 5. * scale,
            width + 10. * scale,
            218. * scale,
            Color::new(0.03, 0.05, 0.07, 0.82),
        );
        draw_text(
            &name,
            x,
            y + 16. * scale,
            18. * scale,
            if watched { YELLOW } else { WHITE },
        );
        draw_text(
            if watched {
                "POV  [Tab to switch]"
            } else {
                "INPUT"
            },
            x,
            y + 34. * scale,
            13. * scale,
            GRAY,
        );
        let key = |label: &str, col: f32, row: f32, span: f32, pressed: bool| {
            let px = x + col * 52. * scale;
            let py = y + (44. + row * 34.) * scale;
            let w = (span * 52. - 4.) * scale;
            let h = 30. * scale;
            let fill = if pressed {
                Color::from_rgba(125, 219, 153, 255)
            } else {
                Color::from_rgba(39, 48, 55, 255)
            };
            draw_rectangle(px, py, w, h, fill);
            let size = 15. * scale;
            let text = measure_text(label, None, size as u16, 1.);
            draw_text(
                label,
                px + (w - text.width) * 0.5,
                py + 21. * scale,
                size,
                if pressed { BLACK } else { WHITE },
            );
        };
        let action = game.last_actions[seat];
        let keys = action.keys;
        key("W", 1., 0., 1., keys.forward);
        key("A", 0., 1., 1., keys.left);
        key("S", 1., 1., 1., keys.backward);
        key("D", 2., 1., 1., keys.right);
        key("SPACE", 0., 2., 3., keys.jump);
        key("SHIFT", 0., 3., 1., keys.sneak);
        key("CTRL", 1., 3., 1., game.started);
        key(
            "LMB",
            2.,
            3.,
            1.,
            game.click_flash[seat] > 0. || action.attack_held,
        );
        draw_text(
            if game.sprint_states[seat] {
                "SPRINT: ON"
            } else {
                "SPRINT: OFF"
            },
            x,
            y + 198. * scale,
            14. * scale,
            if game.sprint_states[seat] {
                GREEN
            } else {
                GRAY
            },
        );
    }
}
fn rect(img: &mut Image, x: u32, y: u32, w: u32, h: u32, color: Color) {
    for yy in y..(y + h).min(HEIGHT.into()) {
        for xx in x..(x + w).min(WIDTH.into()) {
            img.set_pixel(xx, yy, color);
        }
    }
}
fn sprite(img: &mut Image, x: u32, y: u32, rows: &[&str], fill: u8, flash: bool) {
    for (dy, row) in rows.iter().enumerate() {
        for (dx, pixel) in row.bytes().enumerate() {
            let color = match pixel {
                b'.' | b' ' => continue,
                b'#' => {
                    if flash {
                        WHITE
                    } else {
                        grey(21)
                    }
                }
                _ if fill == 0 || (fill == 1 && dx >= 4) => grey(52),
                b'+' => Color::from_rgba(250, 228, 214, 255),
                b'r' => Color::from_rgba(235, 39, 46, 255),
                b'w' => grey(202),
                b'o' => Color::from_rgba(174, 96, 53, 255),
                b'b' => Color::from_rgba(164, 122, 74, 255),
                b't' => Color::from_rgba(116, 242, 220, 255),
                b'c' => Color::from_rgba(34, 155, 157, 255),
                _ => unreachable!(),
            };
            img.set_pixel(x + dx as u32, y + dy as u32, color);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Status {
    hp: u8,
    armor: u8,
    sword: bool,
    flash: bool,
}
impl Status {
    fn image(self) -> Image {
        let mut img = Image::gen_image_color(WIDTH, HEIGHT, Color::new(0., 0., 0., 0.));
        // Hotbar, one selected slot and the empty experience bar.
        rect(&mut img, 1, 28, 182, 22, grey(17));
        rect(&mut img, 2, 29, 180, 20, grey(112));
        for i in 0..9 {
            let x = 3 + 20 * i;
            rect(&mut img, x, 30, 18, 18, grey(40));
            rect(
                &mut img,
                x + 1,
                31,
                17,
                17,
                Color::from_rgba(57, 57, 59, 198),
            );
        }
        rect(&mut img, 0, 27, 24, 2, grey(237));
        rect(&mut img, 0, 29, 2, 21, grey(237));
        rect(&mut img, 22, 29, 2, 21, grey(237));
        rect(&mut img, 0, 48, 24, 2, grey(237));
        if self.sword {
            sprite(&mut img, 4, 31, &SWORD, 2, false);
        }
        rect(&mut img, 1, 21, 182, 5, grey(24));
        rect(&mut img, 2, 22, 180, 2, Color::from_rgba(53, 69, 42, 255));
        for i in 0..10u8 {
            let offset = u32::from(i) * 8;
            let fill = |points: u8| points.saturating_sub(2 * i).min(2);
            sprite(&mut img, 1 + offset, 11, &HEART, fill(self.hp), self.flash);
            if self.armor > 0 {
                sprite(&mut img, 1 + offset, 1, &ARMOR, fill(self.armor), false);
            }
            sprite(&mut img, 174 - offset, 11, &FOOD, 2, false);
        }
        img
    }
}

pub struct Hud {
    status: Option<Status>,
    texture: Texture2D,
    crosshair: Texture2D,
    attack_full: Texture2D,
    attack_background: Texture2D,
    attack_progress: Texture2D,
    invert: Material,
}

const INVERT_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
uniform mat4 Model;
uniform mat4 Projection;
varying lowp vec2 uv;
varying lowp vec4 color;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
    color = color0 / 255.0;
}"#;
const INVERT_FRAGMENT: &str = r#"#version 100
precision mediump float;
uniform sampler2D Texture;
varying lowp vec2 uv;
varying lowp vec4 color;
void main() {
    vec4 pixel = texture2D(Texture, uv) * color;
    if (pixel.a < 0.5) discard;
    gl_FragColor = pixel;
}"#;

fn hud_sprite(bytes: &[u8]) -> Texture2D {
    let texture = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
    texture.set_filter(FilterMode::Nearest);
    texture
}

impl Hud {
    pub fn new() -> Result<Self, String> {
        let texture = Texture2D::from_image(&Image::gen_image_color(WIDTH, HEIGHT, BLACK));
        texture.set_filter(FilterMode::Nearest);
        // Minecraft BlendFunction.INVERT: src * (1-dst) + dst * (1-src).
        // White sprite pixels invert the actual framebuffer, without readback.
        let invert = load_material(
            ShaderSource::Glsl {
                vertex: INVERT_VERTEX,
                fragment: INVERT_FRAGMENT,
            },
            MaterialParams {
                pipeline_params: PipelineParams {
                    color_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::OneMinusValue(BlendValue::DestinationColor),
                        BlendFactor::OneMinusValue(BlendValue::SourceColor),
                    )),
                    alpha_blend: Some(BlendState::new(
                        Equation::Add,
                        BlendFactor::One,
                        BlendFactor::Zero,
                    )),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(|e| format!("Crosshair inversion shader: {e}"))?;
        Ok(Self {
            status: None,
            texture,
            crosshair: hud_sprite(include_bytes!("../assets/hud/bracket_sight.png")),
            attack_full: hud_sprite(include_bytes!("../assets/hud/ready_mark.png")),
            attack_background: hud_sprite(include_bytes!("../assets/hud/charge_rails.png")),
            attack_progress: hud_sprite(include_bytes!("../assets/hud/charge_fill.png")),
            invert,
        })
    }
    pub fn draw(&mut self, o: &Observation, view: ClientView, targeted: bool, hurt: bool) {
        let equipment = Equipment {
            armor: o.armor_present.map(u16::from),
            sword: u16::from(o.sword_present),
        };
        let status = Status {
            hp: o.health.ceil().clamp(0., 20.) as u8,
            armor: equipment.armor_value() as u8,
            sword: o.sword_present,
            flash: hurt,
        };
        if self.status != Some(status) {
            self.texture.update(&status.image());
            self.status = Some(status);
        }
        let w = screen_width();
        let h = screen_height();
        let scale = (w / 320.).min(h / 240.).floor().max(1.);
        let cx = (w / 2.).floor();
        let cy = (h / 2.).floor();
        draw_texture_ex(
            &self.texture,
            cx - f32::from(WIDTH) * scale / 2.,
            h - f32::from(HEIGHT) * scale,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(f32::from(WIDTH), f32::from(HEIGHT)) * scale),
                ..Default::default()
            },
        );

        gl_use_material(&self.invert);
        draw_texture_ex(
            &self.crosshair,
            cx - 7. * scale,
            cy - 7. * scale,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(15., 15.) * scale),
                ..Default::default()
            },
        );
        // Gui.extractCrosshair samples with offset 0; combat uses offset 0.5.
        let delay = (20. / equipment.attack_speed()) as f32;
        let charge = (view.local.attack_ticks as f32 / delay).min(1.);
        if charge < 1. {
            let x = cx - 8. * scale;
            let y = cy + 9. * scale;
            draw_texture_ex(
                &self.attack_background,
                x,
                y,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(16., 4.) * scale),
                    ..Default::default()
                },
            );
            let progress = (charge * 17.).floor().min(16.);
            if progress > 0. {
                draw_texture_ex(
                    &self.attack_progress,
                    x,
                    y,
                    WHITE,
                    DrawTextureParams {
                        source: Some(Rect::new(0., 0., progress, 4.)),
                        dest_size: Some(vec2(progress, 4.) * scale),
                        ..Default::default()
                    },
                );
            }
        } else if targeted && delay > 5. {
            draw_texture_ex(
                &self.attack_full,
                cx - 8. * scale,
                cy + 9. * scale,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(16., 16.) * scale),
                    ..Default::default()
                },
            );
        }
        gl_use_default_material();
    }
}
