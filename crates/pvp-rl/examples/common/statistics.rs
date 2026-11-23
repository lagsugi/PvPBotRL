// Each randomized condition contributes exactly one Bernoulli trial. Timeouts are non-wins.
pub fn statistics(wins: usize, n: usize) -> (f64, [f64; 2]) {
    assert!(n > 0 && n <= 1023 && wins <= n);
    let p = wins as f64 / n as f64;
    let z = 1.959963984540054;
    let m = n as f64;
    let center = p + z * z / (2.0 * m);
    let margin = z * (p * (1.0 - p) / m + z * z / (4.0 * m * m)).sqrt();
    let divisor = 1.0 + z * z / m;
    let mut term = 0.5_f64.powi(n as i32);
    let mut tail = 0.0;
    for k in (wins..=n).rev() {
        tail += term;
        term *= k as f64 / (n - k + 1) as f64;
    }
    (
        tail,
        [(center - margin) / divisor, (center + margin) / divisor],
    )
}
