// ABOUTME: Python bindings for the diceman library using PyO3.
// ABOUTME: Exposes roll, parse, and simulate functions to Python.

use ::diceman as core;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::HashMap;

/// The scored outcome of a dice roll.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct RollOutcome {
    #[pyo3(get)]
    pub kind: String,
    #[pyo3(get)]
    pub value: i64,
}

#[pymethods]
impl RollOutcome {
    fn __repr__(&self) -> String {
        format!("RollOutcome(kind={}, value={})", self.kind, self.value)
    }
}

/// Result of a dice roll.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct RollResult {
    #[pyo3(get)]
    pub outcome: RollOutcome,
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
#[pyclass(skip_from_py_object)]
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

impl SimResult {
    fn to_core(&self) -> core::SimResult {
        core::SimResult {
            distribution: self.distribution.clone(),
            min: self.min,
            max: self.max,
            mean: self.mean,
            std_dev: self.std_dev,
            n: self.n,
        }
    }
}

#[pymethods]
impl SimResult {
    /// Get the mode (most common outcome).
    fn mode(&self) -> Option<i64> {
        self.to_core().mode()
    }

    /// Get outcomes sorted by value (for plotting).
    fn sorted_outcomes(&self) -> Vec<(i64, usize)> {
        self.to_core().sorted_outcomes()
    }

    /// Get probability of each outcome.
    fn probabilities(&self) -> HashMap<i64, f64> {
        self.to_core().probabilities()
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
///     RollResult with outcome and formatted expression
///
/// Example:
///     >>> result = roll("4d6kh3")
///     >>> print(result.outcome.value)
///     15
///     >>> print(result)
///     4d6kh3[6, 5, 4, (1)] = 15
#[pyfunction]
fn roll(expr: &str) -> PyResult<RollResult> {
    core::roll(expr)
        .map(|r| {
            let (kind, value) = match r.outcome {
                core::RollOutcome::Numeric(n) => ("numeric", n),
                core::RollOutcome::Successes(n) => ("successes", n),
            };
            RollResult {
                outcome: RollOutcome {
                    kind: kind.to_string(),
                    value,
                },
                expression: r.expression,
            }
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
    m.add_class::<RollOutcome>()?;
    m.add_class::<RollResult>()?;
    m.add_class::<SimResult>()?;
    Ok(())
}
