use serde::{Deserialize, Serialize};
use std::{
    ops::{Add, Mul, Sub},
    sync::OnceLock,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }
    pub fn horizontal_squared(self) -> f64 {
        self.x * self.x + self.z * self.z
    }
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }
    pub fn normalize(self) -> Self {
        let n = self.length();
        if n < 1e-5 {
            Self::ZERO
        } else {
            self * (1.0 / n)
        }
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
    pub fn components(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }
    pub fn horizontal_scale(self, s: f64) -> Self {
        Self::new(self.x * s, self.y, self.z * s)
    }
}
impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}
impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}
impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}
impl Aabb {
    pub fn contains(self, p: Vec3) -> bool {
        p.x >= self.min.x
            && p.x < self.max.x
            && p.y >= self.min.y
            && p.y < self.max.y
            && p.z >= self.min.z
            && p.z < self.max.z
    }
    pub fn nearest(self, p: Vec3) -> Vec3 {
        Vec3::new(
            p.x.clamp(self.min.x, self.max.x),
            p.y.clamp(self.min.y, self.max.y),
            p.z.clamp(self.min.z, self.max.z),
        )
    }
    pub fn distance_squared(self, p: Vec3) -> f64 {
        (self.nearest(p) - p).length_squared()
    }
    /// Minecraft's entrance-plane clip: half-open segment, 1e-7 face tolerance.
    pub fn clip(self, from: Vec3, to: Vec3) -> Option<Vec3> {
        let f = from.components();
        let d = (to - from).components();
        let lo = self.min.components();
        let hi = self.max.components();
        let mut best = 1.0;
        let mut hit = false;
        for a in 0..3 {
            if d[a].abs() <= 1e-7 {
                continue;
            }
            let face = if d[a] > 0.0 { lo[a] } else { hi[a] };
            let s = (face - f[a]) / d[a];
            let b = (a + 1) % 3;
            let c = (a + 2) % 3;
            let pb = f[b] + s * d[b];
            let pc = f[c] + s * d[c];
            if s > 0.0
                && s < best
                && pb > lo[b] - 1e-7
                && pb < hi[b] + 1e-7
                && pc > lo[c] - 1e-7
                && pc < hi[c] + 1e-7
            {
                best = s;
                hit = true;
            }
        }
        hit.then(|| from + (to - from) * best)
    }
    pub fn aim_point(self, eye: Vec3) -> Vec3 {
        let inset = 1e-4;
        Vec3::new(
            eye.x.clamp(self.min.x + inset, self.max.x - inset),
            eye.y.clamp(self.min.y + inset, self.max.y - inset),
            eye.z.clamp(self.min.z + inset, self.max.z - inset),
        )
    }
}

// The table structure/indexing matches Mth. Table generation uses host libm;
// a cross-platform, Java-generated bit-for-bit table is a deferred validation item.
static SINE: OnceLock<Box<[f32]>> = OnceLock::new();
fn sine_table() -> &'static [f32] {
    SINE.get_or_init(|| {
        (0..65536)
            .map(|i| ((i as f64) / 10430.378350470453).sin() as f32)
            .collect()
    })
}
pub fn sin(radians: f64) -> f32 {
    sine_table()[((radians * 10430.378350470453) as i64 & 65535) as usize]
}
pub fn cos(radians: f64) -> f32 {
    sine_table()[((radians * 10430.378350470453 + 16384.0) as i64 & 65535) as usize]
}
pub fn radians(degrees: f32) -> f32 {
    degrees * (std::f64::consts::PI / 180.0) as f32
}
pub fn wrap_degrees(value: f32) -> f32 {
    let v = value % 360.0;
    if v >= 180.0 {
        v - 360.0
    } else if v < -180.0 {
        v + 360.0
    } else {
        v
    }
}
pub fn wrap_degrees_f64(value: f64) -> f64 {
    let v = value % 360.0;
    if v >= 180.0 {
        v - 360.0
    } else if v < -180.0 {
        v + 360.0
    } else {
        v
    }
}
pub fn look_vector(yaw: f32, pitch: f32) -> Vec3 {
    let yr = radians(yaw);
    let pr = radians(pitch);
    let cy = cos(-(yr as f64));
    let sy = sin(-(yr as f64));
    let cp = cos(pr as f64);
    let sp = sin(pr as f64);
    Vec3::new((sy * cp) as f64, (-sp) as f64, (cy * cp) as f64)
}
pub fn aim_angles(eye: Vec3, target: Vec3, old_yaw: f32) -> (f32, f32) {
    let d = target - eye;
    if d.length_squared() < 1e-16 {
        return (old_yaw, 0.0);
    }
    (
        wrap_degrees((-d.x.atan2(d.z)).to_degrees() as f32),
        (-d.y.atan2(d.horizontal_squared().sqrt())).to_degrees() as f32,
    )
}
/// Java Math.round(double): nearest integer, ties toward positive infinity.
pub fn java_round(v: f64) -> i64 {
    if v.is_nan() {
        return 0;
    }
    let floor = v.floor();
    (floor + if v - floor >= 0.5 { 1.0 } else { 0.0 }) as i64
}
pub fn pack_angle(angle: f32) -> i8 {
    (angle * 256.0 / 360.0).floor() as i32 as i8
}
pub fn unpack_angle(angle: i8) -> f32 {
    (angle as i32 * 360) as f32 / 256.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_negative_rounding() {
        assert_eq!(java_round(-0.5), 0);
        assert_eq!(java_round(-1.5), -1);
        assert_eq!(java_round(0.49999999999999994), 0);
    }
    #[test]
    fn ray_boundaries() {
        let b = Aabb {
            min: Vec3::new(-0.3, 0.0, 3.0),
            max: Vec3::new(0.3, 1.8, 3.6),
        };
        let eye = Vec3::new(0.0, 1.62, 0.0);
        assert_eq!(b.clip(eye, Vec3::new(0.0, 1.62, 4.5)).unwrap().z, 3.0);
        assert!(b.clip(eye, Vec3::new(0.0, 1.62, 3.0)).is_none());
        assert!(!b.contains(b.max));
        assert!(b.contains(b.min));
        assert_eq!(b.distance_squared(eye), 9.0);
    }
    #[test]
    fn view_axes() {
        assert!((look_vector(0.0, 0.0).z - 1.0).abs() < 1e-6);
        assert!((look_vector(90.0, 0.0).x + 1.0).abs() < 1e-6);
    }
}
