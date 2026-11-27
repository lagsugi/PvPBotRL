use crate::{
    policy::{Bot, Style},
    recurrent::Model,
};
use pvp_sim::{Batch, SimError, sim::Snapshot};

pub fn run_baselines(batch: &mut Batch) -> Result<Vec<Snapshot>, SimError> {
    run_matchup(batch, [Style::Baseline; 2])
}

pub fn run_matchup(batch: &mut Batch, styles: [Style; 2]) -> Result<Vec<Snapshot>, SimError> {
    run_matchup_with_model(batch, styles, None)
}

pub fn run_matchup_with_model(
    batch: &mut Batch,
    styles: [Style; 2],
    model: Option<&Model>,
) -> Result<Vec<Snapshot>, SimError> {
    batch.run_with(|duel| {
        let mut bots = styles.map(Bot::new);
        while !duel.is_finished() {
            duel.step_with_policy(|duel, seat, obs| {
                bots[seat].act_in_duel(duel, seat, obs, model)
            })?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pvp_sim::Config;

    #[test]
    fn worker_count_does_not_change_results() {
        let c = Config {
            max_steps: 200,
            ..Config::default()
        };
        let mut a = Batch::new(c.clone(), 8, 12, 1).unwrap();
        let mut b = Batch::new(c, 8, 12, 4).unwrap();
        assert_eq!(
            run_baselines(&mut a).unwrap(),
            run_baselines(&mut b).unwrap()
        );
    }
}
