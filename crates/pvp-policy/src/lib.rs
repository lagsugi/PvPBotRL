//! Bot decisions and model inference over simulator observations.
#![forbid(unsafe_code)]

pub mod learned;
mod matchup;
pub mod policy;
pub mod recurrent;

pub use matchup::{run_baselines, run_matchup, run_matchup_with_model};
