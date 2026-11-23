use crate::config::SimError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Keys {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
}

/// Physical keys after applying the sprint preference. Replays keep this
/// representation so existing recordings retain their original Ctrl inputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppliedKeys {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub sprint: bool,
    pub jump: bool,
    pub sneak: bool,
}
impl AppliedKeys {
    pub fn axes(self) -> (f32, f32) {
        (
            (self.left as i8 - self.right as i8) as f32,
            (self.forward as i8 - self.backward as i8) as f32,
        )
    }
    pub fn forward_impulse(self) -> bool {
        self.forward && !self.backward
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum Look {
    #[default]
    TrackTarget,
    Keep,
    Angles {
        yaw: f32,
        pitch: f32,
    },
    Turn {
        yaw_delta: f32,
        pitch_delta: f32,
    },
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Action<K: Default = Keys> {
    pub keys: K,
    pub look: Look,
    /// One click event, consumed on the next local logical tick.
    pub click: bool,
    /// Held mouse button is separate from clicking (not automatic entity attacks).
    pub attack_held: bool,
}
pub type AppliedAction = Action<AppliedKeys>;

impl Action {
    pub fn applied(self) -> AppliedAction {
        let Keys {
            forward,
            backward,
            left,
            right,
            jump,
            sneak,
        } = self.keys;
        AppliedAction {
            keys: AppliedKeys {
                forward,
                backward,
                left,
                right,
                jump,
                sneak,
                sprint: true,
            },
            look: self.look,
            click: self.click,
            attack_held: self.attack_held,
        }
    }
}
impl<K: Default> Action<K> {
    pub fn validate(self) -> Result<(), SimError> {
        let angles = match self.look {
            Look::Angles { yaw, pitch } => (yaw, pitch),
            Look::Turn {
                yaw_delta,
                pitch_delta,
            } => (yaw_delta, pitch_delta),
            _ => (0.0, 0.0),
        };
        if !angles.0.is_finite()
            || !angles.1.is_finite()
            || angles.0.abs() > 360_000.0
            || angles.1.abs() > 360_000.0
        {
            return Err(SimError::InvalidAction(
                "look angles must be finite and at most 360000 degrees".into(),
            ));
        }
        Ok(())
    }
}
