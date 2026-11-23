//! Combat-relevant packet payloads and numeric wire codecs; no real sockets.
use crate::{
    action::AppliedKeys as Keys,
    math::{Vec3, java_round},
    state::{Equipment, Pose},
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Upstream {
    Attack,
    Swing,
    Input {
        keys: Keys,
    },
    Sprint {
        enabled: bool,
    },
    Move {
        position: Option<Vec3>,
        rotation: Option<[f32; 2]>,
        on_ground: bool,
    },
    TickEnd,
    TeleportAck {
        id: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Downstream {
    Motion {
        entity: usize,
        velocity: Vec3,
    },
    RelativeMove {
        entity: usize,
        delta: Option<[i16; 3]>,
        rotation: Option<[i8; 2]>,
        on_ground: bool,
    },
    PositionSync {
        entity: usize,
        position: Vec3,
        velocity: Vec3,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
    Metadata {
        entity: usize,
        pose: Pose,
        sprinting: bool,
    },
    Health {
        health: f32,
    },
    SelfSprint {
        sprinting: bool,
    },
    SelfMovementSpeed {
        speed: f32,
    },
    OpponentHealth {
        entity: usize,
        health: f32,
    },
    Equipment {
        equipment: Equipment,
    },
    Correction {
        position: Vec3,
        yaw: f32,
        pitch: f32,
        id: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub sent_ns: u64,
    pub arrival_ns: u64,
    pub sequence: u64,
    pub packet: T,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct Wire {
    pub up: [VecDeque<Envelope<Upstream>>; 2],
    pub down: [VecDeque<Envelope<Downstream>>; 2],
    pub sequence: u64,
}
impl Wire {
    pub fn send_up(
        &mut self,
        player: usize,
        now: u64,
        delay: u64,
        packet: Upstream,
    ) -> Envelope<Upstream> {
        let e = Envelope {
            sent_ns: now,
            arrival_ns: now + delay,
            sequence: self.sequence,
            packet,
        };
        self.sequence += 1;
        self.up[player].push_back(e.clone());
        e
    }
    pub fn send_down(
        &mut self,
        player: usize,
        now: u64,
        delay: u64,
        packet: Downstream,
    ) -> Envelope<Downstream> {
        let e = Envelope {
            sent_ns: now,
            arrival_ns: now + delay,
            sequence: self.sequence,
            packet,
        };
        self.sequence += 1;
        self.down[player].push_back(e.clone());
        e
    }
    pub fn pop_up(&mut self, now: u64) -> Option<(usize, Envelope<Upstream>)> {
        let p = (0..2)
            .filter_map(|p| {
                self.up[p]
                    .front()
                    .filter(|e| e.arrival_ns <= now)
                    .map(|e| (p, e.arrival_ns, e.sequence))
            })
            .min_by_key(|&(_, t, s)| (t, s))?
            .0;
        Some((p, self.up[p].pop_front().unwrap()))
    }
}
pub fn encode_delta(base: Vec3, pos: Vec3) -> [i64; 3] {
    let b = base.components();
    let p = pos.components();
    std::array::from_fn(|i| java_round(p[i] * 4096.0) - java_round(b[i] * 4096.0))
}
pub fn decode_delta(base: Vec3, delta: [i16; 3]) -> Vec3 {
    let b = base.components();
    let p: [f64; 3] = std::array::from_fn(|i| {
        if delta[i] == 0 {
            b[i]
        } else {
            (java_round(b[i] * 4096.0) + delta[i] as i64) as f64 / 4096.0
        }
    });
    Vec3::new(p[0], p[1], p[2])
}

/// Exact LpVec3 byte layout, including continuation VarInt and sanitization.
pub fn encode_motion(v: Vec3) -> Vec<u8> {
    let a = v.components().map(|v| {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(-17179869183.0, 17179869183.0)
        }
    });
    let m = a.into_iter().map(f64::abs).fold(0.0, f64::max);
    if m < 3.051944088384301e-5 {
        return vec![0];
    }
    let scale = m.ceil() as u64;
    let more = scale > 3;
    let markers = if more { scale & 3 | 4 } else { scale };
    let packed = a.map(|v| java_round((v / scale as f64 * 0.5 + 0.5) * 32766.0) as u64);
    let bits = markers | packed[0] << 3 | packed[1] << 18 | packed[2] << 33;
    let mut out = Vec::with_capacity(11);
    out.push(bits as u8);
    out.push((bits >> 8) as u8);
    out.extend_from_slice(&((bits >> 16) as u32).to_be_bytes());
    if more {
        let mut n = (scale >> 2) as u32;
        loop {
            if n & !127 == 0 {
                out.push(n as u8);
                break;
            }
            out.push((n as u8 & 127) | 128);
            n >>= 7;
        }
    }
    out
}
pub fn decode_motion(bytes: &[u8]) -> Result<(Vec3, usize), &'static str> {
    let low = *bytes.first().ok_or("empty LpVec3")?;
    if low == 0 {
        return Ok((Vec3::ZERO, 1));
    }
    if bytes.len() < 6 {
        return Err("truncated LpVec3");
    }
    let bits = (u32::from_be_bytes(bytes[2..6].try_into().unwrap()) as u64) << 16
        | (bytes[1] as u64) << 8
        | low as u64;
    let mut used = 6;
    let mut scale = (low & 3) as u64;
    if low & 4 != 0 {
        let mut n = 0u32;
        let mut done = false;
        for i in 0..5 {
            let b = *bytes.get(used).ok_or("truncated VarInt")?;
            used += 1;
            n |= ((b & 127) as u32) << (i * 7);
            if b & 128 == 0 {
                done = true;
                break;
            }
        }
        if !done {
            return Err("VarInt too long");
        }
        scale |= (n as u64) << 2;
    }
    let unpack = |offset: u32| {
        (((bits >> offset) & 32767u64).min(32766) as f64 * 2.0 / 32766.0 - 1.0) * scale as f64
    };
    Ok((Vec3::new(unpack(3), unpack(18), unpack(33)), used))
}
pub fn quantize_motion(v: Vec3) -> Vec3 {
    decode_motion(&encode_motion(v))
        .expect("encoder produces valid bytes")
        .0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_phase1_motion_vectors() {
        for (v, expected, len) in [
            (
                Vec3::new(0.4, 0.4, 0.0),
                Vec3::new(0.39998779222364655, 0.39998779222364655, 0.0),
                6,
            ),
            (
                Vec3::new(0.7, 0.4, -0.2),
                Vec3::new(0.6999938961118233, 0.39998779222364655, -0.2000244155527071),
                6,
            ),
            (
                Vec3::new(4.0, -3.9, 0.123),
                Vec3::new(4.0, -3.899896233900995, 0.12305438564365545),
                7,
            ),
            (Vec3::new(1e-5, 0.0, 0.0), Vec3::ZERO, 1),
        ] {
            let bytes = encode_motion(v);
            assert_eq!(bytes.len(), len);
            assert_eq!(decode_motion(&bytes), Ok((expected, len)));
        }
    }
    #[test]
    fn invalid_payloads() {
        assert!(decode_motion(&[]).is_err());
        assert!(decode_motion(&[4, 0, 0, 0, 0, 0]).is_err());
        assert!(decode_motion(&[4, 0, 0, 0, 0, 0, 128, 128, 128, 128, 128]).is_err());
    }
    #[test]
    fn motion_sanitizes() {
        let v = quantize_motion(Vec3::new(f64::NAN, f64::INFINITY, f64::NEG_INFINITY));
        assert_eq!(v, Vec3::new(0.0, 17179869183.0, -17179869183.0));
    }
    #[test]
    fn delta_zero_preserves_base() {
        let base = Vec3::new(0.00001, 1.2, 0.0);
        assert_eq!(decode_delta(base, [0, 0, 0]), base);
        assert_eq!(decode_delta(base, [0, 0, 2867]).z, 0.699951171875);
    }
    #[test]
    fn connections_preserve_order_and_arrival_wins() {
        let mut w = Wire::default();
        w.send_up(0, 0, 20, Upstream::Attack);
        w.send_up(1, 0, 10, Upstream::Swing);
        w.send_up(1, 0, 10, Upstream::Attack);
        assert!(w.pop_up(9).is_none());
        assert_eq!(w.pop_up(20).unwrap().0, 1);
        assert_eq!(w.pop_up(20).unwrap().1.packet, Upstream::Attack);
        assert_eq!(w.pop_up(20).unwrap().0, 0);
    }
}
