// ABOUTME: Python bindings for the diceman library using PyO3.
// ABOUTME: Exposes roll, parse, and simulate functions to Python.

use ::diceman as core;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::HashMap;

/// Result of a dice roll.
#[pyclass]
#[derive(Clone)]
pub struct RollResult {
    #[pyo3(get)]
    pub total: i64,
    #[pyo3(get)]
    pub expression: String,
}

#[pymethods]
impl RollResult {
    fn __repr__(&self) -> String {
        self.expression.clone()
    }

    fn __str__(&self) -> String {
        self.expression.clone()
    }
}

/// Result of a Monte Carlo simulation.
#[pyclass]
#[derive(Clone)]
pub struct SimResult {
    #[pyo3(get)]
    pub distribution: HashMap<i64, usize>,
    #[pyo3(get)]
    pub min: i64,
    #[pyo3(get)]
    pub max: i64,
    #[pyo3(get)]
    pub mean: f64,
    #[pyo3(get)]
    pub std_dev: f64,
    #[pyo3(get)]
    pub n: usize,
}

#[pymethods]
impl SimResult {
    /// Get the mode (most common outcome).
    fn mode(&self) -> Option<i64> {
        self.distribution
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&value, _)| value)
    }

    /// Get outcomes sorted by value (for plotting).
    fn sorted_outcomes(&self) -> Vec<(i64, usize)> {
        let mut outcomes: Vec<_> = self.distribution.iter().map(|(&k, &v)| (k, v)).collect();
        outcomes.sort_by_key(|(k, _)| *k);
        outcomes
    }

    /// Get probability of each outcome.
    fn probabilities(&self) -> HashMap<i64, f64> {
        self.distribution
            .iter()
            .map(|(&k, &v)| (k, v as f64 / self.n as f64))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "SimResult(n={}, mean={:.2}, std_dev={:.2}, min={}, max={})",
            self.n, self.mean, self.std_dev, self.min, self.max
        )
    }
}

/// Roll dice using the given expression.
///
/// Args:
///     expr: A dice expression like "4d6kh3" or "2d6 + 5"
///
/// Returns:
///     RollResult with total and formatted expression
///
/// Example:
///     >>> result = roll("4d6kh3")
///     >>> print(result.total)
///     15
///     >>> print(result)
///     4d6kh3[6, 5, 4, (1)] = 15
#[pyfunction]
fn roll(expr: &str) -> PyResult<RollResult> {
    core::roll(expr)
        .map(|r| RollResult {
            total: r.total,
            expression: r.expression,
        })
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Simulate rolling dice many times to get probability distribution.
///
/// Args:
///     expr: A dice expression like "2d6"
///     n: Number of trials to run (default: 10000)
///
/// Returns:
///     SimResult with distribution and statistics
///
/// Example:
///     >>> sim = simulate("2d6", n=100000)
///     >>> print(sim.mean)  # ~7.0
///     >>> print(sim.distribution)  # {2: 2789, 3: 5521, ...}
#[pyfunction]
#[pyo3(signature = (expr, n=10000))]
fn simulate(expr: &str, n: usize) -> PyResult<SimResult> {
    core::simulate(expr, n)
        .map(|r| SimResult {
            distribution: r.distribution,
            min: r.min,
            max: r.max,
            mean: r.mean,
            std_dev: r.std_dev,
            n: r.n,
        })
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Python module for diceman.
#[pymodule]
fn diceman(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(roll, m)?)?;
    m.add_function(wrap_pyfunction!(simulate, m)?)?;
    m.add_class::<RollResult>()?;
    m.add_class::<SimResult>()?;
    Ok(())
}
