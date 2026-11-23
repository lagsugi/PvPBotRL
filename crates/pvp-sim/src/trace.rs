use crate::{
    action::{Action, AppliedAction},
    combat::AttackResult,
    config::{Config, SimError},
    network::{Downstream, Envelope, Upstream},
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TraceEvent {
    SendUp {
        player: usize,
        envelope: Envelope<Upstream>,
    },
    ApplyUp {
        player: usize,
        sequence: u64,
    },
    SendDown {
        player: usize,
        envelope: Envelope<Downstream>,
    },
    ApplyDown {
        player: usize,
        sequence: u64,
    },
    Attack {
        attacker: usize,
        result: AttackResult,
    },
    AttackRejected {
        attacker: usize,
        reason: String,
    },
    FallDamage {
        player: usize,
        damage: crate::combat::DamageResult,
    },
    Correction {
        player: usize,
    },
    ServerTick,
    ClientTick {
        player: usize,
    },
    /// Privileged state after a well-defined phase, opt-in separately from packet tracing.
    State {
        phase: String,
        player: Option<usize>,
        snapshot: Box<crate::sim::Snapshot>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TraceRecord {
    pub time_ns: u64,
    pub event: TraceEvent,
}
#[derive(Clone, Debug)]
pub(crate) struct Trace {
    capacity: usize,
    records: VecDeque<TraceRecord>,
    pub dropped: u64,
}
impl Trace {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            records: VecDeque::new(),
            dropped: 0,
        }
    }
    pub fn enabled(&self) -> bool {
        self.capacity > 0
    }
    pub fn push(&mut self, time_ns: u64, event: TraceEvent) {
        if self.capacity == 0 {
            return;
        }
        if self.records.len() == self.capacity {
            self.records.pop_front();
            self.dropped += 1;
        }
        self.records.push_back(TraceRecord { time_ns, event });
    }
    pub fn drain(&mut self) -> Vec<TraceRecord> {
        self.records.drain(..).collect()
    }
}
/// Portable action recording, not an arbitrary mutable internal-state import.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replay {
    pub version: u32,
    pub config: Config,
    pub seed: u64,
    pub actions: Vec<[AppliedAction; 2]>,
}
impl Replay {
    pub fn new(config: Config, seed: u64) -> Self {
        Self {
            version: 1,
            config,
            seed,
            actions: Vec::new(),
        }
    }
    pub fn record(&mut self, decisions: [Action; 2]) {
        self.actions.push(decisions.map(Action::applied));
    }
    pub fn run(&self) -> Result<crate::sim::Duel, SimError> {
        if self.version != 1 {
            return Err(SimError::Replay("unsupported version".into()));
        }
        let mut duel = crate::sim::Duel::new(self.config.clone(), self.seed)?;
        for action in &self.actions {
            duel.step_recorded(*action)?;
        }
        Ok(duel)
    }
}
