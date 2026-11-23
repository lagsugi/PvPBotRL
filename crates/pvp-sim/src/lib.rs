//! Virtual-time 1v1 simulation. Minecraft differential validation is pending.
//! See `docs/simulator/README.md` for the supported profile and deliberate boundaries.
#![forbid(unsafe_code)]

pub mod action;
pub mod batch;
pub mod combat;
pub mod config;
pub mod math;
pub mod network;
pub mod observation;
pub mod physics;
pub mod ping;
pub mod rng;
pub mod sim;
pub mod state;
pub mod trace;

pub use action::{Action, Keys, Look};
pub use batch::Batch;
pub use config::{Config, SimError, TICK_NS};
pub use observation::{Feature, Observation, ObservationSchema};
pub use sim::{Duel, Outcome, StepResult};
