// ABOUTME: Monte Carlo simulation for dice expressions.
// ABOUTME: Runs many trials to compute probability distributions and statistics.

use crate::error::Result;
use crate::parser;
use crate::roller::{evaluate_with_rng, FastRng};
use std::collections::HashMap;

/// Result of a Monte Carlo simulation.
#[derive(Debug, Clone)]
pub struct SimResult {
    /// Distribution of outcomes: value -> count.
    pub distribution: HashMap<i64, usize>,
    /// Minimum value observed.
    pub min: i64,
    /// Maximum value observed.
    pub max: i64,
    /// Mean (average) value.
    pub mean: f64,
    /// Standard deviation.
    pub std_dev: f64,
    /// Number of trials run.
    pub n: usize,
}

impl SimResult {
    /// Returns outcomes sorted by value for iteration.
    pub fn sorted_outcomes(&self) -> Vec<(i64, usize)> {
        let mut outcomes: Vec<_> = self.distribution.iter().map(|(&k, &v)| (k, v)).collect();
        outcomes.sort_by_key(|(k, _)| *k);
        outcomes
    }

    /// Returns the probability of each outcome.
    pub fn probabilities(&self) -> HashMap<i64, f64> {
        self.distribution
            .iter()
            .map(|(&k, &v)| (k, v as f64 / self.n as f64))
            .collect()
    }

    /// Returns the mode (most common outcome).
    pub fn mode(&self) -> Option<i64> {
        self.distribution
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&value, _)| value)
    }

    /// Returns the median value.
    pub fn median(&self) -> f64 {
        let mut values: Vec<i64> = Vec::with_capacity(self.n);
        for (&value, &count) in &self.distribution {
            for _ in 0..count {
                values.push(value);
            }
        }
        values.sort();

        if values.is_empty() {
            return 0.0;
        }

        let mid = values.len() / 2;
        if values.len() % 2 == 0 {
            (values[mid - 1] + values[mid]) as f64 / 2.0
        } else {
            values[mid] as f64
        }
    }
}

/// Run a Monte Carlo simulation on a dice expression.
///
/// # Arguments
/// * `expr` - The dice expression to simulate (e.g., "4d6kh3")
/// * `n` - Number of trials to run
///
/// # Returns
/// A `SimResult` containing the distribution and statistics.
pub fn simulate(expr: &str, n: usize) -> Result<SimResult> {
    let parsed = parser::parse(expr)?;
    let mut rng = FastRng::new();

    let mut distribution: HashMap<i64, usize> = HashMap::new();
    let mut sum: i64 = 0;
    let mut sum_sq: i64 = 0;
    let mut min = i64::MAX;
    let mut max = i64::MIN;

    for _ in 0..n {
        let result = evaluate_with_rng(&parsed, &mut rng)?;
        let total = result.total;

        *distribution.entry(total).or_insert(0) += 1;
        sum += total;
        sum_sq += total * total;
        min = min.min(total);
        max = max.max(total);
    }

    let mean = sum as f64 / n as f64;
    let variance = (sum_sq as f64 / n as f64) - (mean * mean);
    let std_dev = variance.sqrt();

    Ok(SimResult {
        distribution,
        min,
        max,
        mean,
        std_dev,
        n,
    })
}

/// Run a simulation with a seeded RNG for reproducibility.
pub fn simulate_seeded(expr: &str, n: usize, seed: u64) -> Result<SimResult> {
    let parsed = parser::parse(expr)?;
    let mut rng = FastRng::with_seed(seed);

    let mut distribution: HashMap<i64, usize> = HashMap::new();
    let mut sum: i64 = 0;
    let mut sum_sq: i64 = 0;
    let mut min = i64::MAX;
    let mut max = i64::MIN;

    for _ in 0..n {
        let result = evaluate_with_rng(&parsed, &mut rng)?;
        let total = result.total;

        *distribution.entry(total).or_insert(0) += 1;
        sum += total;
        sum_sq += total * total;
        min = min.min(total);
        max = max.max(total);
    }

    let mean = sum as f64 / n as f64;
    let variance = (sum_sq as f64 / n as f64) - (mean * mean);
    let std_dev = variance.sqrt();

    Ok(SimResult {
        distribution,
        min,
        max,
        mean,
        std_dev,
        n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulate_basic() {
        let result = simulate("1d6", 1000).unwrap();

        // Should have outcomes 1-6
        assert!(result.min >= 1);
        assert!(result.max <= 6);
        assert_eq!(result.n, 1000);

        // Mean should be close to 3.5
        assert!((result.mean - 3.5).abs() < 0.5);
    }

    #[test]
    fn test_simulate_constant() {
        let result = simulate("5", 100).unwrap();

        assert_eq!(result.min, 5);
        assert_eq!(result.max, 5);
        assert_eq!(result.mean, 5.0);
        assert_eq!(result.std_dev, 0.0);
        assert_eq!(result.distribution.len(), 1);
        assert_eq!(result.distribution[&5], 100);
    }

    #[test]
    fn test_simulate_seeded_reproducible() {
        let result1 = simulate_seeded("2d6", 1000, 42).unwrap();
        let result2 = simulate_seeded("2d6", 1000, 42).unwrap();

        assert_eq!(result1.distribution, result2.distribution);
        assert_eq!(result1.mean, result2.mean);
    }

    #[test]
    fn test_simulate_2d6_range() {
        let result = simulate("2d6", 10000).unwrap();

        // 2d6 ranges from 2 to 12
        assert!(result.min >= 2);
        assert!(result.max <= 12);

        // Mean should be close to 7
        assert!((result.mean - 7.0).abs() < 0.3);
    }

    #[test]
    fn test_sorted_outcomes() {
        let result = simulate_seeded("1d6", 600, 123).unwrap();
        let sorted = result.sorted_outcomes();

        // Should be sorted by value
        for i in 1..sorted.len() {
            assert!(sorted[i - 1].0 < sorted[i].0);
        }
    }

    #[test]
    fn test_probabilities() {
        let result = simulate("5", 100).unwrap();
        let probs = result.probabilities();

        assert_eq!(probs[&5], 1.0);
    }

    #[test]
    fn test_mode() {
        let result = simulate("5", 100).unwrap();
        assert_eq!(result.mode(), Some(5));
    }

    #[test]
    fn test_median() {
        let result = simulate("5", 100).unwrap();
        assert_eq!(result.median(), 5.0);
    }
}
