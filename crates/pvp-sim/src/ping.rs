//! Population RTT model, not a TCP stack or packet-loss/jitter simulation.
use crate::{
    config::{ClientTiming, SimError},
    rng::derive_seed,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PingProfile {
    /// None draws Gamma(k=2, theta=35ms) conditional on 5 <= RTT <= 150ms.
    pub fixed_ms: [Option<f64>; 2],
    /// Triangular upstream fraction centered on 0.5, with this half-width.
    /// Default 0.1 gives 40:60..60:40. This is a modeling assumption.
    pub asymmetry: f64,
}
impl Default for PingProfile {
    fn default() -> Self {
        Self {
            fixed_ms: [None; 2],
            asymmetry: 0.1,
        }
    }
}
fn cdf(ms: f64) -> f64 {
    1. - (-ms / 35.).exp() * (1. + ms / 35.)
}
pub fn gamma_rtt_ms(unit: f64) -> f64 {
    let target = cdf(5.) + unit.clamp(0., 1.) * (cdf(150.) - cdf(5.));
    let (mut lo, mut hi) = (5., 150.);
    for _ in 0..56 {
        let mid = (lo + hi) / 2.;
        if cdf(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.
}
impl PingProfile {
    pub fn validate(&self) -> Result<(), SimError> {
        if !self.asymmetry.is_finite()
            || !(0.0..=0.5).contains(&self.asymmetry)
            || self
                .fixed_ms
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0.0..=10000.).contains(v))
        {
            return Err(SimError::InvalidConfig(
                "ping: RTT must be 0..10000 ms; asymmetry must be 0..0.5".into(),
            ));
        }
        Ok(())
    }
    pub fn apply(&self, clients: &mut [ClientTiming; 2], seed: u64) {
        for (seat, client) in clients.iter_mut().enumerate() {
            let unit = |i| {
                (derive_seed(seed ^ 0x5049_4e47, seat as u64 * 4 + i) >> 11) as f64
                    / (1u64 << 53) as f64
            };
            let rtt = self.fixed_ms[seat].unwrap_or_else(|| gamma_rtt_ms(unit(0)));
            let fraction = 0.5 + self.asymmetry * (unit(1) - unit(2));
            let total = (rtt * 1e6).round() as u64;
            client.upstream_ns = (total as f64 * fraction).round() as u64;
            client.downstream_ns = total - client.upstream_ns;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampled_network_replays_and_resets_exactly() {
        use crate::{Action, Config, Duel, trace::Replay};
        let config = Config {
            ping_profile: Some(PingProfile::default()),
            ..Default::default()
        };
        let mut d = Duel::new(config, 65).unwrap();
        let mut replay = Replay::new(d.config().clone(), 65);
        for _ in 0..40 {
            let actions = [Action::default(); 2];
            replay.record(actions);
            d.step(actions).unwrap();
        }
        let encoded = serde_json::to_string(&replay).unwrap();
        let decoded: Replay = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.run().unwrap().snapshot(), d.snapshot());
        d.reset(66).unwrap();
        assert_ne!(d.config().clients, replay.config.clients);
        let reference = Duel::new(replay.config, 66).unwrap();
        assert_eq!(d.config().clients, reference.config().clients);
    }
    #[test]
    fn truncated_gamma_population_and_reproducibility() {
        let p = PingProfile::default();
        let mut counts = [0; 3];
        for seed in 0..10000 {
            let mut a = std::array::from_fn(|_| ClientTiming::default());
            let mut b = a.clone();
            p.apply(&mut a, seed);
            p.apply(&mut b, seed);
            assert_eq!(a, b);
            let rtt = (a[0].upstream_ns + a[0].downstream_ns) as f64 / 1e6;
            assert!((5.0..=150.0).contains(&rtt));
            let ratio = a[0].upstream_ns as f64 / (a[0].upstream_ns + a[0].downstream_ns) as f64;
            assert!((0.399999..=0.600001).contains(&ratio));
            for (i, t) in [20., 50., 100.].iter().enumerate() {
                counts[i] += usize::from(rtt <= *t);
            }
        }
        for (i, t) in [20., 50., 100.].iter().enumerate() {
            let expected = (cdf(*t) - cdf(5.)) / (cdf(150.) - cdf(5.));
            assert!((counts[i] as f64 / 10000. - expected).abs() < 0.015);
        }
    }
    #[test]
    fn fixed_human_and_invalid_profiles() {
        let mut p = PingProfile {
            fixed_ms: [Some(20.), None],
            ..Default::default()
        };
        let mut clients = std::array::from_fn(|_| ClientTiming::default());
        for seed in 0..100 {
            p.apply(&mut clients, seed);
            assert_eq!(
                clients[0].upstream_ns + clients[0].downstream_ns,
                20_000_000
            );
        }
        p.fixed_ms[0] = Some(f64::NAN);
        assert!(p.validate().is_err());
        p.fixed_ms[0] = Some(20.);
        p.asymmetry = 0.51;
        assert!(p.validate().is_err());
    }
}
