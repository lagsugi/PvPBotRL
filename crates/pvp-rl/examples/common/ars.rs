use pvp_policy::learned::OUTPUTS;
pub use pvp_rl::league::Random;
use rayon::prelude::*;

/// ARS update from positive/negative perturbations scored in the same matches.
pub fn update<const N: usize>(
    weights: &mut [[f64; N]; OUTPUTS],
    rng: &mut Random,
    directions: usize,
    top: usize,
    noise: f64,
    rate: f64,
    reward: impl Fn([[f64; N]; OUTPUTS]) -> f64 + Sync,
) {
    let directions: Vec<[[f64; N]; OUTPUTS]> = (0..directions)
        .map(|_| std::array::from_fn(|_| std::array::from_fn(|_| rng.normal())))
        .collect();
    let mut pairs: Vec<_> = directions
        .par_iter()
        .map(|delta| {
            let returns = [noise, -noise].map(|scale| {
                reward(std::array::from_fn(|i| {
                    std::array::from_fn(|j| weights[i][j] + scale * delta[i][j])
                }))
            });
            (returns, delta)
        })
        .collect();
    pairs.sort_by(|(a, _), (b, _)| b[0].max(b[1]).total_cmp(&a[0].max(a[1])));
    pairs.truncate(top);
    let mean = pairs.iter().map(|(r, _)| r[0] + r[1]).sum::<f64>() / (2 * top) as f64;
    let sd = (pairs
        .iter()
        .flat_map(|(r, _)| r)
        .map(|r| (r - mean).powi(2))
        .sum::<f64>()
        / (2 * top) as f64)
        .sqrt();
    for (returns, delta) in pairs {
        let step = rate * (returns[0] - returns[1]) / (top as f64 * sd.max(1e-8));
        for (row, direction) in weights.iter_mut().zip(delta) {
            for (w, d) in row.iter_mut().zip(direction) {
                *w += step * d;
            }
        }
    }
}
