use pvp_sim::Outcome;

/// Reward-only server state. Never used to construct a policy input.
pub fn reward(
    before: [f32; 2],
    after: [f32; 2],
    initial: [f32; 2],
    seat: usize,
    outcome: Outcome,
    beta: f32,
) -> f32 {
    let hp = |health: f32, player: usize| (health / initial[player]).clamp(0., 1.);
    let potential =
        |health: [f32; 2]| 0.25 * (hp(health[seat], seat) - hp(health[1 - seat], 1 - seat));
    let end = match outcome {
        Outcome::Ongoing => potential(after),
        Outcome::Victory { winner } if winner == seat => 1. + beta * hp(after[seat], seat),
        Outcome::Victory { .. } | Outcome::Truncated => -1.,
    };
    end - potential(before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_rewards_keep_hp_bonus_and_cancel_damage_shaping() {
        let initial = [20., 20.];
        let middle = [19., 8.];
        let prefix = reward(initial, middle, initial, 0, Outcome::Ongoing, 0.1);
        for (outcome, after, expected) in [
            (Outcome::Victory { winner: 0 }, [10., 0.], 1.05),
            (Outcome::Victory { winner: 1 }, [0., 4.], -1.),
            (Outcome::Truncated, [19., 8.], -1.),
        ] {
            assert!(
                (prefix + reward(middle, after, initial, 0, outcome, 0.1) - expected).abs() < 1e-6
            );
        }
    }
}
