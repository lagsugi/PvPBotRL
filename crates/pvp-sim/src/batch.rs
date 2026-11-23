use crate::{Action, Config, Duel, Observation, SimError, StepResult, rng::derive_seed};
use rayon::prelude::*;
/// Persistent worker pool, independent match state, stable result ordering.
pub struct Batch {
    duels: Vec<Duel>,
    pool: rayon::ThreadPool,
}
impl Batch {
    pub fn new(config: Config, count: usize, seed: u64, threads: usize) -> Result<Self, SimError> {
        config.validate()?;
        if count == 0 || threads == 0 {
            return Err(SimError::InvalidBatch(
                "count and threads must be positive".into(),
            ));
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| SimError::InvalidBatch(e.to_string()))?;
        let duels = (0..count)
            .map(|i| Duel::new(config.clone(), derive_seed(seed, i as u64)))
            .collect::<Result<_, _>>()?;
        Ok(Self { duels, pool })
    }
    pub fn len(&self) -> usize {
        self.duels.len()
    }
    pub fn is_empty(&self) -> bool {
        self.duels.is_empty()
    }
    pub fn observe(&self) -> Vec<[Observation; 2]> {
        self.duels.iter().map(Duel::observations).collect()
    }
    pub fn duel(&self, index: usize) -> Option<&Duel> {
        self.duels.get(index)
    }
    pub fn reset_at(&mut self, index: usize, seed: u64) -> Result<[Observation; 2], SimError> {
        self.duels
            .get_mut(index)
            .ok_or_else(|| SimError::InvalidBatch("index out of bounds".into()))?
            .reset(seed)
    }
    /// All inputs are checked before any match advances. Finished lanes must be
    /// reset explicitly; final observations are never silently replaced.
    pub fn step(&mut self, actions: &[[Action; 2]]) -> Result<Vec<StepResult>, SimError> {
        if actions.len() != self.len() {
            return Err(SimError::InvalidBatch(
                "action count differs from match count".into(),
            ));
        }
        for (duel, actions) in self.duels.iter().zip(actions) {
            if duel.is_finished() {
                return Err(SimError::EpisodeFinished);
            }
            for a in actions {
                a.validate()?;
            }
        }
        self.pool.install(|| {
            self.duels
                .par_iter_mut()
                .zip(actions)
                .map(|(d, a)| d.step(*a))
                .collect()
        })
    }
    /// Runs one operation per lane in parallel and returns snapshots in lane order.
    /// The operation owns the progression of its lane. An error does not undo
    /// changes already made in that lane or in other lanes.
    pub fn run_with<F>(&mut self, run: F) -> Result<Vec<crate::sim::Snapshot>, SimError>
    where
        F: Fn(&mut Duel) -> Result<(), SimError> + Sync,
    {
        self.pool.install(|| {
            self.duels
                .par_iter_mut()
                .map(|duel| {
                    run(duel)?;
                    Ok(duel.snapshot())
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_batch_is_atomic() {
        let mut b = Batch::new(Config::default(), 2, 1, 2).unwrap();
        let before = b.observe();
        assert!(b.step(&[[Action::default(); 2]]).is_err());
        assert_eq!(b.observe(), before);
        let bad = Action {
            look: crate::Look::Turn {
                yaw_delta: f32::INFINITY,
                pitch_delta: 0.0,
            },
            ..Action::default()
        };
        assert!(b.step(&[[Action::default(); 2], [bad; 2]]).is_err());
        assert_eq!(b.observe(), before);
    }
}
