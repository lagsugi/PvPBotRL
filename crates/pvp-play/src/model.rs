//! Render-only humanoid and extruded pixel item. Dimensions/animation anchors
//! follow 26.1.2 HumanoidModel, PlayerModel and ItemInHandRenderer. The duelist
//! and armor use the project's original pixel art in assets/player.
use crate::{game::Game, hud::SWORD};
use macroquad::prelude::*;
use pvp_sim::state::{Body, Pose};
use std::f32::consts::PI;

fn translation(p: Vec3) -> Mat4 {
    Mat4::from_translation(p)
}

fn quad(mesh: &mut Mesh, points: [Vec3; 4], color: Color) {
    let base = mesh.vertices.len() as u16;
    mesh.vertices
        .extend(points.map(|p| Vertex::new2(p, Vec2::ZERO, color)));
    mesh.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn empty_mesh() -> Mesh {
    Mesh {
        vertices: vec![],
        indices: vec![],
        texture: None,
    }
}

fn tint(c: Color, light: f32) -> Color {
    Color::new(c.r * light, c.g * light, c.b * light, c.a)
}

// Bake the standard Minecraft box UV unwrap into colored pixel quads. This
// keeps exact nearest-neighbor pixels, transparent armor openings and shading
// without adding a texture/material dependency to the draw path.
// Local +Z is the face; local +X is the player's left.
fn textured_box(image: &Image, uv: [usize; 2], size: [usize; 3], inflate: f32) -> Mesh {
    let mut mesh = empty_mesh();
    let [w, h, d] = size;
    let [tx, ty] = uv;
    for (origin, u, v, cols, rows, light, uv_x, uv_y) in [
        (
            vec3(-0.5, -1., 0.5),
            Vec3::X,
            Vec3::Y,
            w,
            h,
            1.0,
            tx + d,
            ty + d,
        ),
        (
            vec3(0.5, -1., -0.5),
            -Vec3::X,
            Vec3::Y,
            w,
            h,
            0.72,
            tx + d * 2 + w,
            ty + d,
        ),
        (
            vec3(-0.5, -1., -0.5),
            Vec3::Z,
            Vec3::Y,
            d,
            h,
            0.8,
            tx,
            ty + d,
        ),
        (
            vec3(0.5, -1., 0.5),
            -Vec3::Z,
            Vec3::Y,
            d,
            h,
            0.8,
            tx + d + w,
            ty + d,
        ),
        (
            vec3(-0.5, 0., 0.5),
            Vec3::X,
            -Vec3::Z,
            w,
            d,
            1.0,
            tx + d,
            ty,
        ),
        (
            vec3(-0.5, -1., -0.5),
            Vec3::X,
            Vec3::Z,
            w,
            d,
            0.6,
            tx + d + w,
            ty,
        ),
    ] {
        for y in 0..rows {
            for x in 0..cols {
                let color = image.get_pixel((uv_x + x) as u32, (uv_y + rows - 1 - y) as u32);
                if color.a < 0.5 {
                    continue;
                }
                let color = tint(color, light);
                let p = origin + u * (x as f32 / cols as f32) + v * (y as f32 / rows as f32);
                let du = u / cols as f32;
                let dv = v / rows as f32;
                let scale = vec3(w as f32, h as f32, d as f32) + Vec3::splat(inflate * 2.);
                let offset = Vec3::Y * inflate;
                quad(
                    &mut mesh,
                    [
                        p * scale + offset,
                        (p + du) * scale + offset,
                        (p + du + dv) * scale + offset,
                        (p + dv) * scale + offset,
                    ],
                    color,
                );
            }
        }
    }
    mesh
}

fn append_mesh(target: &mut Mesh, source: Mesh) {
    let base = target.vertices.len() as u16;
    target
        .indices
        .extend(source.indices.into_iter().map(|i| i + base));
    target.vertices.extend(source.vertices);
}

fn player_image(bytes: &[u8]) -> Image {
    Image::from_file_with_format(bytes, Some(ImageFormat::Png))
        .expect("embedded player texture must be a valid PNG")
}

fn sword() -> Mesh {
    let mut mesh = empty_mesh();
    for (y, row) in SWORD.iter().enumerate() {
        for (x, pixel) in row.bytes().enumerate() {
            let color = match pixel {
                b'#' => Color::from_rgba(18, 56, 57, 255),
                b't' => Color::from_rgba(116, 242, 220, 255),
                b'c' => Color::from_rgba(34, 155, 157, 255),
                b'b' => Color::from_rgba(164, 122, 74, 255),
                _ => continue,
            };
            // ItemModelGenerator: full sprite occupies [0,16] on X/Y,
            // with Z from 7.5 to 8.5. ItemTransform centers it afterwards.
            let p = vec3(x as f32 / 16., (15. - y as f32) / 16., 7.5 / 16.);
            let dx = Vec3::X / 16.;
            let dy = Vec3::Y / 16.;
            let dz = Vec3::Z / 16.;
            for (o, u, v, light) in [
                (p, dx, dy, 0.85),
                (p + dz, dx, dy, 1.),
                (p, dx, dz, 0.55),
                (p + dy, dx, dz, 1.),
                (p, dy, dz, 0.65),
                (p + dx, dy, dz, 0.75),
            ] {
                quad(&mut mesh, [o, o + u, o + u + v, o + v], tint(color, light));
            }
        }
    }
    mesh
}

fn right_hand_sword_transform(attack: f32, inverse_arm_height: f32) -> Mat4 {
    let attack = attack.clamp(0., 1.);
    let arc = (attack.sqrt() * PI).sin();
    // 26.1.2 ItemInHandRenderer.applyItemArmTransform + swingArm +
    // applyItemArmAttackTransform, in PoseStack multiplication order.
    let hand = translation(vec3(0.56, -0.52 - inverse_arm_height * 0.6, -0.72))
        * translation(vec3(
            -0.4 * arc,
            0.2 * (attack.sqrt() * 2. * PI).sin(),
            -0.2 * (attack * PI).sin(),
        ))
        * Mat4::from_rotation_y((45. - 20. * (attack * attack * PI).sin()).to_radians())
        * Mat4::from_rotation_z((-20. * arc).to_radians())
        * Mat4::from_rotation_x((-80. * arc).to_radians())
        * Mat4::from_rotation_y(-45_f32.to_radians());
    // diamond_sword.json inherits handheld.json's firstperson_righthand.
    // ItemTransform.apply: translate, rotationXYZ, scale, center model.
    let item = translation(vec3(1.13, 3.2, 1.13) / 16.)
        * Mat4::from_rotation_y(-90_f32.to_radians())
        * Mat4::from_rotation_z(25_f32.to_radians())
        * Mat4::from_scale(Vec3::splat(0.68))
        * translation(Vec3::splat(-0.5));
    hand * item
}

fn draw_transformed(mesh: &Mesh, transform: Mat4) {
    draw_transformed_tinted(mesh, transform, false);
}

fn draw_transformed_tinted(mesh: &Mesh, transform: Mat4, hurt: bool) {
    let vertices = mesh
        .vertices
        .iter()
        .map(|v| Vertex {
            position: transform.transform_point3(v.position),
            color: if hurt {
                // A red overlay retains the original pixel shading.
                [
                    128 + v.color[0] / 2,
                    v.color[1] / 2,
                    v.color[2] / 2,
                    v.color[3],
                ]
            } else {
                v.color
            },
            ..*v
        })
        .collect();
    draw_mesh(&Mesh {
        vertices,
        indices: mesh.indices.clone(),
        texture: None,
    });
}

pub struct Models {
    head: Mesh,
    torso: Mesh,
    arm: Mesh,
    left_arm: Mesh,
    leg: Mesh,
    left_leg: Mesh,
    helmet: Mesh,
    chest: Mesh,
    armor_arm: Mesh,
    armor_leg: Mesh,
    waist: Mesh,
    sword: Mesh,
    walk_phase: f32,
    last_step: u64,
}

impl Models {
    pub fn new() -> Self {
        let skin = player_image(include_bytes!("../assets/player/duelist.png"));
        let armor = player_image(include_bytes!("../assets/player/duelist_plate.png"));
        let leggings = player_image(include_bytes!("../assets/player/duelist_greaves.png"));
        let mut head = textured_box(&skin, [0, 0], [8, 8, 8], 0.);
        append_mesh(&mut head, textured_box(&skin, [32, 0], [8, 8, 8], 0.5));
        let mut torso = textured_box(&skin, [16, 16], [8, 12, 4], 0.);
        append_mesh(&mut torso, textured_box(&skin, [16, 32], [8, 12, 4], 0.25));
        let mut arm = textured_box(&skin, [40, 16], [4, 12, 4], 0.);
        append_mesh(&mut arm, textured_box(&skin, [40, 32], [4, 12, 4], 0.25));
        let mut left_arm = textured_box(&skin, [32, 48], [4, 12, 4], 0.);
        append_mesh(
            &mut left_arm,
            textured_box(&skin, [48, 48], [4, 12, 4], 0.25),
        );
        let mut leg = textured_box(&skin, [0, 16], [4, 12, 4], 0.);
        append_mesh(&mut leg, textured_box(&skin, [0, 32], [4, 12, 4], 0.25));
        let mut left_leg = textured_box(&skin, [16, 48], [4, 12, 4], 0.);
        append_mesh(
            &mut left_leg,
            textured_box(&skin, [0, 48], [4, 12, 4], 0.25),
        );
        let mut armor_leg = textured_box(&leggings, [0, 16], [4, 12, 4], 0.4);
        append_mesh(
            &mut armor_leg,
            textured_box(&armor, [0, 16], [4, 12, 4], 0.9),
        );
        Self {
            head,
            torso,
            arm,
            left_arm,
            leg,
            left_leg,
            helmet: textured_box(&armor, [0, 0], [8, 8, 8], 1.),
            chest: textured_box(&armor, [16, 16], [8, 12, 4], 1.),
            armor_arm: textured_box(&armor, [40, 16], [4, 12, 4], 1.),
            armor_leg,
            waist: textured_box(&leggings, [16, 16], [8, 12, 4], 0.5),
            sword: sword(),
            walk_phase: 0.,
            last_step: 0,
        }
    }

    pub fn opponent(&mut self, game: &Game, body: Body, alpha: f32) {
        let step = game.duel.decision_step();
        if step < self.last_step {
            self.walk_phase = 0.;
        }
        if step != self.last_step {
            let delta = game.current.opponent.position - game.previous.opponent.position;
            self.walk_phase += delta.horizontal_squared().sqrt() as f32 * 16. * 0.6662;
            self.last_step = step;
        }
        let delta = game.current.opponent.position - game.previous.opponent.position;
        let stride = (delta.horizontal_squared().sqrt() as f32 * 4.).min(0.8);
        let phase = self.walk_phase
            - (1. - alpha) * delta.horizontal_squared().sqrt() as f32 * 16. * 0.6662;
        let walk = phase.cos() * stride;
        let draw_part = |mesh, transform| {
            draw_transformed_tinted(mesh, transform, game.opponent_damage_flash > 0.);
        };
        let crouch = body.pose == Pose::Crouching;
        let root = translation(vec3(
            body.position.x as f32,
            body.position.y as f32,
            body.position.z as f32,
        )) * Mat4::from_rotation_y(-body.yaw.to_radians())
            * Mat4::from_scale(Vec3::splat(0.9375 / 16.));
        let neck = if crouch { 19.8 } else { 24. };
        let head = root
            * translation(vec3(0., neck, if crouch { 2. } else { 0. }))
            * Mat4::from_rotation_x(body.pitch.to_radians())
            * translation(vec3(0., 8., 0.));
        draw_part(&self.head, head);
        draw_part(&self.helmet, head);
        let torso = root
            * translation(vec3(0., if crouch { 20.8 } else { 24. }, 0.))
            * Mat4::from_rotation_x(if crouch { -0.5 } else { 0. });
        draw_part(&self.torso, torso);
        draw_part(&self.waist, torso);
        draw_part(&self.chest, torso);
        for (x, angle, skin) in [
            (-1.9, walk * 1.4, &self.leg),
            (1.9, -walk * 1.4, &self.left_leg),
        ] {
            let leg = root
                * translation(vec3(x, 12., if crouch { -4. } else { 0. }))
                * Mat4::from_rotation_x(angle);
            draw_part(skin, leg);
            draw_part(&self.armor_leg, leg);
        }
        let time = game.duel.time_ns() as f32 / 50_000_000. + alpha;
        let idle = (time * 0.067).sin() * 0.05;
        let spread = (time * 0.09).cos() * 0.05 + 0.05;
        let attack = if game.opponent_swing > 0. {
            1. - game.opponent_swing / 0.3
        } else {
            0.
        };
        let slash = (attack.sqrt() * PI).sin();
        for (x, side, skin) in [(-5., -1., &self.arm), (5., 1., &self.left_arm)] {
            let right = side < 0.;
            // Held-item arm pose (half walking amplitude and PI/10 raised),
            // plus HumanoidModel's gentle idle bob and an observed swing.
            let angle = if right {
                -walk * 0.5 - PI / 10. - slash * 1.5
            } else {
                walk
            } + side * idle
                + if crouch { 0.4 } else { 0. };
            let shoulder = root
                * translation(vec3(x, if crouch { 18.8 } else { 22. }, 0.))
                * Mat4::from_rotation_y(if right { -slash * 0.4 } else { 0. })
                * Mat4::from_rotation_z(side * spread)
                * Mat4::from_rotation_x(angle);
            let arm = shoulder * translation(vec3(side, 2., 0.));
            draw_part(skin, arm);
            draw_part(&self.armor_arm, arm);
            if right {
                // ItemInHandLayer followed by handheld.json's third-person
                // transform. Convert MC's model Y-down/Z-back to our Y-up/Z-front.
                let item = shoulder
                    * Mat4::from_scale(Vec3::splat(16.))
                    * Mat4::from_scale(vec3(1., -1., -1.))
                    * Mat4::from_rotation_x(-90_f32.to_radians())
                    * Mat4::from_rotation_y(PI)
                    * translation(vec3(1., 2., -10.) / 16.)
                    * translation(vec3(0., 4., 0.5) / 16.)
                    * Mat4::from_rotation_y(-90_f32.to_radians())
                    * Mat4::from_rotation_z(55_f32.to_radians())
                    * Mat4::from_scale(Vec3::splat(0.85))
                    * translation(Vec3::splat(-0.5));
                draw_transformed(&self.sword, item);
            }
        }
    }

    pub fn hand(&self, game: &Game, alpha: f32) {
        // A separate camera gives the hand its vanilla 70-degree projection
        // and prevents nearby world geometry from clipping through the sword.
        set_camera(&Camera3D {
            position: Vec3::ZERO,
            target: -Vec3::Z,
            up: Vec3::Y,
            fovy: 70_f32.to_radians(),
            z_near: 0.01,
            z_far: 10.,
            ..Default::default()
        });
        // set_camera flushes the world batch. Only its depth is discarded;
        // keep the world color and let the item occlude itself normally.
        // SAFETY: called on Macroquad's render thread, with no other context
        // references retained across this scope.
        unsafe {
            get_internal_gl().quad_context.clear(None, Some(1.0), None);
        }
        let ticks = game.current.local.attack_ticks;
        let attack = if ticks < 6 {
            (ticks as f32 + alpha) / 6.
        } else {
            0.
        };
        if game.observation.sword_present {
            draw_transformed(&self.sword, right_hand_sword_transform(attack, 0.));
        } else {
            draw_transformed(
                &self.arm,
                translation(vec3(0.56, -0.52, -0.72))
                    * Mat4::from_scale(Vec3::splat(0.055))
                    * Mat4::from_rotation_x(-0.3),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textured_player_parts_fit_renderer_batches_and_keep_helmet_opening() {
        // Exercise embedded PNG decoding and all UV ranges without opening a
        // window. An oversized mesh would be truncated by Macroquad.
        let models = Models::new();
        for mesh in [
            &models.head,
            &models.torso,
            &models.arm,
            &models.left_arm,
            &models.leg,
            &models.left_leg,
            &models.helmet,
            &models.chest,
            &models.armor_arm,
            &models.armor_leg,
            &models.waist,
        ] {
            assert!(!mesh.vertices.is_empty());
            assert!(mesh.vertices.len() < 10000);
            assert!(mesh.indices.len() < 5000);
            assert!(
                mesh.indices
                    .iter()
                    .all(|&i| (i as usize) < mesh.vertices.len())
            );
            assert!(mesh.vertices.iter().all(|v| v.position.is_finite()));
        }
        // The visor leaves the duelist's eyes visible under its solid brow.
        let armor = player_image(include_bytes!("../assets/player/duelist_plate.png"));
        assert!(armor.get_pixel(9, 12).a < 0.5);
        assert!(armor.get_pixel(11, 8).a >= 0.5);
    }

    fn near(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length() < 0.00001,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn held_sword_matches_vanilla_model_center_and_axes() {
        // Independent numeric fixtures from handheld.json and the resting
        // right-hand offset. Catch grip-origin, scale and Euler-order errors.
        let m = right_hand_sword_transform(0., 0.);
        near(
            m.transform_point3(Vec3::splat(0.5)),
            vec3(0.630625, -0.32, -0.649375),
        );
        near(
            m.transform_vector3(Vec3::X),
            vec3(0., 0.28738043, 0.6162893),
        );
        near(
            m.transform_vector3(Vec3::Y),
            vec3(0., 0.6162893, -0.28738043),
        );
        near(m.transform_vector3(Vec3::Z), vec3(-0.68, 0., 0.));
        near(
            right_hand_sword_transform(0., 1.).transform_point3(Vec3::splat(0.5)),
            vec3(0.630625, -0.92, -0.649375),
        );
    }

    #[test]
    fn sword_geometry_uses_generated_item_coordinates() {
        let mesh = sword();
        assert!(!mesh.vertices.is_empty());
        for v in &mesh.vertices {
            assert!((0.0..=1.0).contains(&v.position.x));
            assert!((0.0..=1.0).contains(&v.position.y));
            assert!(v.position.z == 7.5 / 16. || v.position.z == 8.5 / 16.);
        }
        for i in &mesh.indices {
            assert!((*i as usize) < mesh.vertices.len());
        }
    }
}
