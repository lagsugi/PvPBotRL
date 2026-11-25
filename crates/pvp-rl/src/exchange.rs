use pvp_sim::Action;
use serde::Serialize;

/// A window begins at the first damage and spans eight consecutive decisions.
/// Closing a terminal window includes the killing blow.
#[derive(Default)]
pub(crate) struct Exchanges {
    age: u8,
    damage: [f32; 2], // dealt, taken in actor coordinates
    opening: usize,
    consecutive_even: u32,
    pub totals: ExchangeTotals,
}

#[derive(Clone, Copy, Default, Serialize)]
pub struct ExchangeTotals {
    pub windows: u32,
    pub even: u32,
    pub repeated_even: u32,
    pub longest_even_streak: u32,
    pub clean: u32,
    pub dealt: f32,
    pub taken: f32,
    /// Longitudinal intent (back/neutral/forward) x lateral intent at first damage.
    pub profitable_openings: [u32; 6],
}

impl Exchanges {
    pub fn record(&mut self, dealt: f32, taken: f32, action: Action, terminal: bool) -> f32 {
        if self.age == 0 {
            if dealt == 0. && taken == 0. {
                return 0.;
            }
            let motion = 1 + i8::from(action.keys.forward) - i8::from(action.keys.backward);
            self.opening = motion as usize * 2 + usize::from(action.keys.left || action.keys.right);
        }
        self.age += 1;
        self.damage[0] += dealt;
        self.damage[1] += taken;
        if self.age < 8 && !terminal {
            return 0.;
        }
        let [out, incoming] = self.damage;
        let even = out.min(incoming) >= 0.75 * out.max(incoming);
        self.totals.windows += 1;
        self.totals.even += u32::from(even);
        self.consecutive_even = if even { self.consecutive_even + 1 } else { 0 };
        self.totals.repeated_even += u32::from(self.consecutive_even >= 2);
        self.totals.longest_even_streak =
            self.totals.longest_even_streak.max(self.consecutive_even);
        self.totals.clean += u32::from(out > 0. && incoming == 0.);
        self.totals.dealt += out;
        self.totals.taken += incoming;
        if out > incoming {
            self.totals.profitable_openings[self.opening] += 1;
        }
        self.age = 0;
        self.damage = [0.; 2];
        // Continuous overlap cost avoids a discontinuity at the reporting threshold.
        out.min(incoming)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_reply_and_terminal_flush() {
        let mut ex = Exchanges::default();
        let idle = Action::default();
        assert_eq!(ex.record(3., 0., idle, false), 0.);
        for _ in 0..6 {
            assert_eq!(ex.record(0., 0., idle, false), 0.);
        }
        assert_eq!(ex.record(0., 2.5, idle, false), 2.5);
        assert_eq!(ex.record(4., 0., idle, true), 0.);
        assert_eq!(
            (ex.totals.windows, ex.totals.even, ex.totals.clean),
            (2, 1, 1)
        );
        assert_eq!(ex.totals.profitable_openings.iter().sum::<u32>(), 2);
    }

    #[test]
    fn counts_repeated_even_windows_across_idle_decisions() {
        let mut ex = Exchanges::default();
        for _ in 0..3 {
            ex.record(2., 2., Action::default(), false);
            for _ in 0..12 {
                ex.record(0., 0., Action::default(), false);
            }
        }
        assert_eq!(
            (
                ex.totals.even,
                ex.totals.repeated_even,
                ex.totals.longest_even_streak
            ),
            (3, 2, 3)
        );
    }
}
