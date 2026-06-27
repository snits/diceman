// ABOUTME: Monte Carlo simulation for dice expressions.
// ABOUTME: Runs many trials to compute probability distributions and statistics.

use crate::ast::{EdgePolicy, Expr, RollOutcome};
use crate::error::{Error, Result};
use crate::parser;
use crate::roller::{evaluate_outcome, evaluate_total, marvel_plan, marvel_verdict, FastRng, Rng};
use std::collections::HashMap;

/// Result of a Monte Carlo simulation.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

    /// Returns cumulative probability of rolling >= each value, sorted by value.
    pub fn cumulative_gte(&self) -> Vec<(i64, f64)> {
        let outcomes = self.sorted_outcomes();
        let mut result = Vec::with_capacity(outcomes.len());
        let mut remaining = self.n;

        for (value, count) in &outcomes {
            result.push((*value, remaining as f64 / self.n as f64));
            remaining -= count;
        }

        result
    }

    /// Returns cumulative probability of rolling <= each value, sorted by value.
    pub fn cumulative_lte(&self) -> Vec<(i64, f64)> {
        let outcomes = self.sorted_outcomes();
        let mut result = Vec::with_capacity(outcomes.len());
        let mut accumulated = 0usize;

        for (value, count) in &outcomes {
            accumulated += count;
            result.push((*value, accumulated as f64 / self.n as f64));
        }

        result
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
        if values.len().is_multiple_of(2) {
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
    simulate_with_rng(expr, n, &mut FastRng::new())
}

/// Run a simulation with a seeded RNG for reproducibility.
pub fn simulate_seeded(expr: &str, n: usize, seed: u64) -> Result<SimResult> {
    simulate_with_rng(expr, n, &mut FastRng::with_seed(seed))
}

fn simulate_with_rng(expr: &str, n: usize, rng: &mut impl Rng) -> Result<SimResult> {
    validate_trial_count(n)?;
    let parsed = parser::parse(expr)?;

    let mut distribution: HashMap<i64, usize> = HashMap::new();
    let mut sum: i64 = 0;
    let mut sum_sq: i64 = 0;
    let mut min = i64::MAX;
    let mut max = i64::MIN;

    for _ in 0..n {
        let total = evaluate_total(&parsed, rng)?;
        *distribution.entry(total).or_insert(0) += 1;
        sum += total;
        sum_sq += total * total;
        min = min.min(total);
        max = max.max(total);
    }

    Ok(finalize_sim_result(distribution, sum, sum_sq, min, max, n))
}

fn validate_trial_count(n: usize) -> Result<()> {
    if n == 0 {
        return Err(Error::InvalidTrialCount(n));
    }
    Ok(())
}

/// Assemble a `SimResult` from streaming accumulators.
///
/// Computes `mean` and `std_dev` (population variance: `sum_sq/n - mean²`)
/// from the running sums. Shared by `simulate_with_rng` and
/// `simulate_marvel_with_rng` so both use identical statistics math.
fn finalize_sim_result(
    distribution: HashMap<i64, usize>,
    sum: i64,
    sum_sq: i64,
    min: i64,
    max: i64,
    n: usize,
) -> SimResult {
    let mean = sum as f64 / n as f64;
    let variance = (sum_sq as f64 / n as f64) - (mean * mean);
    let std_dev = variance.sqrt();

    SimResult {
        distribution,
        min,
        max,
        mean,
        std_dev,
        n,
    }
}

/// Result of a Marvel Multiverse Monte Carlo simulation.
///
/// Per-trial booleans (success, fantastic, auto_fail, m_shown) are aggregated
/// directly from each trial's `MarvelOutcome` — they cannot be recovered from
/// the total histogram alone, because `auto_fail` and `m_shown` depend on the
/// middle die face, not on the total.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MarvelSimResult {
    /// Number of trials run.
    pub n: usize,
    /// Target value the check is compared against.
    pub target: i64,
    /// Modifier added to the total before comparison.
    pub modifier: i64,
    /// Fraction of trials that succeeded: `(total + modifier >= target) && !auto_fail`.
    pub success_rate: f64,
    /// Fraction of trials that were Fantastic successes (m_shown && success).
    pub fantastic_success_rate: f64,
    /// Fraction of trials that were Fantastic failures (m_shown && !success).
    pub fantastic_failure_rate: f64,
    /// Fraction of trials that auto-failed (raw `1 / M / 1`).
    pub auto_fail_rate: f64,
    /// Fraction of trials where the Marvel die showed M.
    pub m_shown_rate: f64,
    /// Distribution and statistics of the Marvel totals.
    pub total: SimResult,
}

/// Run a Marvel Multiverse Monte Carlo simulation with a custom RNG.
///
/// Each trial builds the canonical Marvel `RollPlan` (with the given Edge and
/// Trouble counts and Edge policy), evaluates it via `evaluate_outcome`, and
/// aggregates per-trial booleans alongside the total histogram.
pub(crate) fn simulate_marvel_with_rng(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: EdgePolicy,
    n: usize,
    rng: &mut impl Rng,
) -> Result<MarvelSimResult> {
    validate_trial_count(n)?;
    let plan = marvel_plan(edges, troubles, policy);
    let expr = Expr::Roll(plan);

    let mut distribution: HashMap<i64, usize> = HashMap::new();
    let mut sum: i64 = 0;
    let mut sum_sq: i64 = 0;
    let mut min = i64::MAX;
    let mut max = i64::MIN;
    let mut success_count = 0usize;
    let mut fantastic_success_count = 0usize;
    let mut fantastic_failure_count = 0usize;
    let mut auto_fail_count = 0usize;
    let mut m_shown_count = 0usize;

    for _ in 0..n {
        let outcome = evaluate_outcome(&expr, rng)?;
        let o = match outcome {
            RollOutcome::Marvel(o) => o,
            RollOutcome::Numeric(_) | RollOutcome::Successes(_) => {
                return Err(Error::InvalidMarvelRoll(
                    "Marvel plan produced a non-Marvel outcome".to_string(),
                ));
            }
        };
        let (success, _) = marvel_verdict(&o, target, modifier);
        if success {
            success_count += 1;
        }
        if o.m_shown {
            m_shown_count += 1;
            if success {
                fantastic_success_count += 1;
            } else {
                fantastic_failure_count += 1;
            }
        }
        if o.auto_fail {
            auto_fail_count += 1;
        }
        sum += o.total;
        sum_sq += o.total * o.total;
        min = min.min(o.total);
        max = max.max(o.total);
        *distribution.entry(o.total).or_insert(0) += 1;
    }

    let total = finalize_sim_result(distribution, sum, sum_sq, min, max, n);
    let n_f = n as f64;
    Ok(MarvelSimResult {
        n,
        target,
        modifier,
        success_rate: success_count as f64 / n_f,
        fantastic_success_rate: fantastic_success_count as f64 / n_f,
        fantastic_failure_rate: fantastic_failure_count as f64 / n_f,
        auto_fail_rate: auto_fail_count as f64 / n_f,
        m_shown_rate: m_shown_count as f64 / n_f,
        total,
    })
}

/// Run a Marvel Multiverse Monte Carlo simulation with the default RNG.
pub fn simulate_marvel(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: EdgePolicy,
    n: usize,
) -> Result<MarvelSimResult> {
    simulate_marvel_with_rng(
        edges,
        troubles,
        target,
        modifier,
        policy,
        n,
        &mut FastRng::new(),
    )
}

/// Run a Marvel Multiverse Monte Carlo simulation with a seeded RNG.
pub fn simulate_marvel_seeded(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: EdgePolicy,
    n: usize,
    seed: u64,
) -> Result<MarvelSimResult> {
    simulate_marvel_with_rng(
        edges,
        troubles,
        target,
        modifier,
        policy,
        n,
        &mut FastRng::with_seed(seed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

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
    fn simulate_rejects_zero_trials() {
        let err = simulate("1d6", 0).unwrap_err();
        assert!(matches!(err, Error::InvalidTrialCount(0)));
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

    #[test]
    fn test_cumulative_gte() {
        // Use a constant expression so distribution is deterministic
        let result = simulate("5", 100).unwrap();
        let cum = result.cumulative_gte();

        // Single value: 100% chance of rolling >= 5
        assert_eq!(cum.len(), 1);
        assert_eq!(cum[0], (5, 1.0));
    }

    #[test]
    fn test_cumulative_gte_multiple_values() {
        let result = simulate_seeded("1d4", 10000, 42).unwrap();
        let cum = result.cumulative_gte();

        // First entry (lowest value) should be ~100%
        assert!((cum[0].1 - 1.0).abs() < 0.001);
        // Last entry (highest value) should be its own probability
        let last = cum.last().unwrap();
        assert!(last.1 > 0.0 && last.1 < 0.5);
        // Each entry should be >= the next (monotonically decreasing)
        for i in 1..cum.len() {
            assert!(cum[i - 1].1 >= cum[i].1);
        }
    }

    #[test]
    fn test_cumulative_lte() {
        let result = simulate("5", 100).unwrap();
        let cum = result.cumulative_lte();

        assert_eq!(cum.len(), 1);
        assert_eq!(cum[0], (5, 1.0));
    }

    #[test]
    fn test_cumulative_lte_multiple_values() {
        let result = simulate_seeded("1d4", 10000, 42).unwrap();
        let cum = result.cumulative_lte();

        // First entry (lowest value) should be its own probability
        assert!(cum[0].1 > 0.0 && cum[0].1 < 0.5);
        // Last entry (highest value) should be ~100%
        let last = cum.last().unwrap();
        assert!((last.1 - 1.0).abs() < 0.001);
        // Each entry should be >= the previous (monotonically increasing)
        for i in 1..cum.len() {
            assert!(cum[i].1 >= cum[i - 1].1);
        }
    }

    // --- Marvel simulate API tests ---

    use crate::ast::EdgePolicy;
    use crate::roller::{roll_marvel_with_rng, Rng};

    /// A deterministic `Rng` for testing that cycles through its value vec.
    struct TestRng {
        values: Vec<u32>,
        index: usize,
    }

    impl TestRng {
        fn new(values: Vec<u32>) -> Self {
            Self { values, index: 0 }
        }
    }

    impl Rng for TestRng {
        fn roll(&mut self, _max: u32) -> u32 {
            let value = self.values[self.index % self.values.len()];
            self.index += 1;
            value
        }
    }

    /// Build a `TestRng` that cycles through all 216 ordered Marvel triples
    /// (3 RNG draws per trial, 216 trials).
    fn marvel_cycle_216_rng() -> TestRng {
        let mut values = Vec::with_capacity(216 * 3);
        for l in 1..=6u32 {
            for m in 1..=6u32 {
                for r in 1..=6u32 {
                    values.push(l);
                    values.push(m);
                    values.push(r);
                }
            }
        }
        TestRng::new(values)
    }

    #[test]
    fn simulate_marvel_rates_match_exhaustive_enumeration() {
        let mut rng = marvel_cycle_216_rng();
        let result =
            simulate_marvel_with_rng(0, 0, 12, 0, EdgePolicy::RerollLowest, 216, &mut rng).unwrap();

        assert!((result.m_shown_rate - 36.0 / 216.0).abs() < 1e-12);
        assert!((result.auto_fail_rate - 1.0 / 216.0).abs() < 1e-12);

        // Compute the expected success count by directly enumerating the 216
        // triples through `roll_marvel_with_rng` with the same target/modifier.
        // Per-trial aggregation matters: success is (total + modifier >= target
        // && !auto_fail), which cannot be recovered from the total histogram
        // alone (auto_fail triples have total=3 which would otherwise pass).
        let mut expected_success = 0usize;
        for l in 1..=6u32 {
            for m in 1..=6u32 {
                for r in 1..=6u32 {
                    let mut trial_rng = TestRng::new(vec![l, m, r]);
                    let check =
                        roll_marvel_with_rng(0, 0, 12, 0, EdgePolicy::RerollLowest, &mut trial_rng)
                            .unwrap();
                    if check.success {
                        expected_success += 1;
                    }
                }
            }
        }
        let expected_success_rate = expected_success as f64 / 216.0;
        assert!(
            (result.success_rate - expected_success_rate).abs() < 1e-12,
            "success_rate {} must match direct enumeration {}",
            result.success_rate,
            expected_success_rate
        );

        // The total histogram must match the 216 base enumeration.
        let expected = [
            0, 0, 0, 1, 1, 3, 6, 10, 15, 22, 26, 28, 28, 26, 20, 14, 9, 5, 2,
        ];
        for (total, &expected_count) in expected.iter().enumerate().skip(3) {
            let count = *result.total.distribution.get(&(total as i64)).unwrap_or(&0);
            assert_eq!(
                count, expected_count,
                "total {total}: got {count}, expected {expected_count}"
            );
        }
    }

    #[test]
    fn simulate_marvel_seeded_reproducible_smoke() {
        let a = simulate_marvel_seeded(0, 0, 12, 0, EdgePolicy::RerollLowest, 1000, 42).unwrap();
        let b = simulate_marvel_seeded(0, 0, 12, 0, EdgePolicy::RerollLowest, 1000, 42).unwrap();
        assert_eq!(a.success_rate, b.success_rate);
        assert_eq!(a.m_shown_rate, b.m_shown_rate);
        assert_eq!(a.auto_fail_rate, b.auto_fail_rate);
        assert_eq!(a.total.distribution, b.total.distribution);
    }

    #[test]
    fn simulate_marvel_rejects_zero_trials() {
        let err = simulate_marvel(0, 0, 12, 0, EdgePolicy::RerollLowest, 0).unwrap_err();
        assert!(matches!(err, Error::InvalidTrialCount(0)));
    }

    #[test]
    fn simulate_marvel_all_ones_aggregates_auto_fail_and_fantastic_failure() {
        // All-ones rolls: auto-fail, m_shown, total=3, success=false (auto_fail),
        // fantastic=Failure. Exercises the sim per-trial boolean aggregation.
        let mut rng = TestRng::new(vec![1; 600]);
        let result =
            simulate_marvel_with_rng(0, 0, 12, 0, EdgePolicy::RerollLowest, 200, &mut rng).unwrap();
        assert_eq!(result.n, 200);
        assert_eq!(result.target, 12);
        assert_eq!(result.modifier, 0);
        assert!((result.auto_fail_rate - 1.0).abs() < 1e-12);
        assert!((result.m_shown_rate - 1.0).abs() < 1e-12);
        assert!((result.success_rate - 0.0).abs() < 1e-12);
        assert!((result.fantastic_success_rate - 0.0).abs() < 1e-12);
        assert!((result.fantastic_failure_rate - 1.0).abs() < 1e-12);
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::{MarvelSimResult, SimResult};
    use crate::ast::{EdgePolicy, Fantastic};
    use crate::roller::Rng;
    use std::collections::HashMap;

    /// A deterministic `Rng` for testing that cycles through its value vec.
    struct TestRng {
        values: Vec<u32>,
        index: usize,
    }

    impl TestRng {
        fn new(values: Vec<u32>) -> Self {
            Self { values, index: 0 }
        }
    }

    impl Rng for TestRng {
        fn roll(&mut self, _max: u32) -> u32 {
            let value = self.values[self.index % self.values.len()];
            self.index += 1;
            value
        }
    }

    #[test]
    fn sim_result_serializes_to_json() {
        let mut distribution: HashMap<i64, usize> = HashMap::new();
        distribution.insert(7, 60);
        distribution.insert(2, 40);
        let result = SimResult {
            distribution,
            min: 2,
            max: 7,
            mean: 5.0,
            std_dev: 2.5,
            n: 100,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["min"], 2);
        assert_eq!(json["max"], 7);
        assert_eq!(json["mean"], 5.0);
        assert_eq!(json["std_dev"], 2.5);
        assert_eq!(json["n"], 100);
        assert_eq!(json["distribution"]["7"], 60);
        assert_eq!(json["distribution"]["2"], 40);
    }

    #[test]
    fn marvel_sim_result_serializes_to_json() {
        let mut distribution: HashMap<i64, usize> = HashMap::new();
        distribution.insert(3, 200);
        let total = SimResult {
            distribution,
            min: 3,
            max: 3,
            mean: 3.0,
            std_dev: 0.0,
            n: 200,
        };
        let result = MarvelSimResult {
            n: 200,
            target: 12,
            modifier: 0,
            success_rate: 0.0,
            fantastic_success_rate: 0.0,
            fantastic_failure_rate: 1.0,
            auto_fail_rate: 1.0,
            m_shown_rate: 1.0,
            total,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["n"], 200);
        assert_eq!(json["target"], 12);
        assert_eq!(json["modifier"], 0);
        assert_eq!(json["success_rate"], 0.0);
        assert_eq!(json["fantastic_success_rate"], 0.0);
        assert_eq!(json["fantastic_failure_rate"], 1.0);
        assert_eq!(json["auto_fail_rate"], 1.0);
        assert_eq!(json["m_shown_rate"], 1.0);
        assert_eq!(json["total"]["n"], 200);
        assert_eq!(json["total"]["min"], 3);
    }

    #[test]
    fn marvel_sim_result_serializes_with_fantastic_success_field() {
        // Drive a small Marvel sim that produces at least one fantastic success
        // so the `fantastic_success_rate` field is exercised through serde.
        // Rolls [6,1,6] produce total=18, m_shown, !auto_fail, success vs target=12.
        let mut values = Vec::with_capacity(60);
        for _ in 0..20 {
            values.extend_from_slice(&[6, 1, 6]);
        }
        let mut rng = TestRng::new(values);
        let result =
            super::simulate_marvel_with_rng(0, 0, 12, 0, EdgePolicy::RerollLowest, 20, &mut rng)
                .unwrap();
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["fantastic_success_rate"], 1.0);
        assert_eq!(json["m_shown_rate"], 1.0);
        // Round-trip the Fantastic value through the serialized JSON shape.
        let fantastic_json = serde_json::to_string(&Fantastic::Success).unwrap();
        assert_eq!(fantastic_json, r#""Success""#);
    }
}
