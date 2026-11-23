use std::time::Duration;

/// Wall time drives the existing 50 ms decision API, never variable-dt physics.
#[derive(Default)]
pub struct FixedClock {
    remainder: Duration,
}
impl FixedClock {
    pub fn reset(&mut self) {
        self.remainder = Duration::ZERO;
    }
    /// Pause on long stalls instead of replaying missed human input at high speed.
    pub fn advance(&mut self, elapsed: Duration) -> Result<usize, ()> {
        if elapsed > Duration::from_millis(250) {
            self.reset();
            return Err(());
        }
        self.remainder += elapsed;
        let tick = Duration::from_nanos(pvp_sim::TICK_NS);
        let mut n = 0;
        while self.remainder >= tick {
            self.remainder -= tick;
            n += 1;
        }
        Ok(n)
    }
    pub fn alpha(&self) -> f64 {
        self.remainder.as_secs_f64() / 0.05
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn render_rates_produce_twenty_physics_ticks_per_second() {
        for fps in [30u64, 60, 144, 240] {
            let mut c = FixedClock::default();
            let mut n = 0;
            for f in 1..=fps * 10 {
                let now = f * 1_000_000_000 / fps;
                let prev = (f - 1) * 1_000_000_000 / fps;
                n += c.advance(Duration::from_nanos(now - prev)).unwrap();
            }
            assert_eq!(n, 200);
            assert_eq!(c.alpha(), 0.0);
        }
    }
    #[test]
    fn pause_and_stall_do_not_create_catchup_steps() {
        let mut c = FixedClock::default();
        assert_eq!(c.advance(Duration::from_millis(30)), Ok(0));
        c.reset();
        assert_eq!(c.advance(Duration::from_millis(20)), Ok(0));
        assert_eq!(c.advance(Duration::from_secs(5)), Err(()));
        assert_eq!(c.advance(Duration::from_millis(49)), Ok(0));
    }
}
