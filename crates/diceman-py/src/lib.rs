// ABOUTME: Python bindings for the diceman library using PyO3.
// ABOUTME: Exposes roll, parse, and simulate functions to Python.

use ::diceman as core;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::HashMap;

/// The scored outcome of a dice roll.
///
/// `successes`, `advantages`, `triumphs`, `despairs`, `light`, and `dark`
/// are populated only for the `"symbols"` kind (narrative Genesys/Star Wars
/// rolls); they are `None` for every other kind.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct RollOutcome {
    #[pyo3(get)]
    pub kind: String,
    #[pyo3(get)]
    pub value: i64,
    #[pyo3(get)]
    pub successes: Option<i64>,
    #[pyo3(get)]
    pub advantages: Option<i64>,
    #[pyo3(get)]
    pub triumphs: Option<u8>,
    #[pyo3(get)]
    pub despairs: Option<u8>,
    #[pyo3(get)]
    pub light: Option<u8>,
    #[pyo3(get)]
    pub dark: Option<u8>,
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

/// The scored outcome of a Marvel Multiverse roll.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct MarvelOutcome {
    #[pyo3(get)]
    pub total: i64,
    #[pyo3(get)]
    pub auto_fail: bool,
    #[pyo3(get)]
    pub m_shown: bool,
}

#[pymethods]
impl MarvelOutcome {
    fn __repr__(&self) -> String {
        format!(
            "MarvelOutcome(total={}, auto_fail={}, m_shown={})",
            self.total, self.auto_fail, self.m_shown
        )
    }
}

/// Result of a Marvel Multiverse target check.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct MarvelCheck {
    #[pyo3(get)]
    pub outcome: MarvelOutcome,
    #[pyo3(get)]
    pub target: i64,
    #[pyo3(get)]
    pub modifier: i64,
    #[pyo3(get)]
    pub success: bool,
    #[pyo3(get)]
    pub fantastic: Option<String>,
    #[pyo3(get)]
    pub expression: String,
}

#[pymethods]
impl MarvelCheck {
    fn __repr__(&self) -> String {
        self.expression.clone()
    }

    fn __str__(&self) -> String {
        self.expression.clone()
    }
}

/// Result of a Marvel Multiverse Monte Carlo simulation.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct MarvelSimResult {
    #[pyo3(get)]
    pub n: usize,
    #[pyo3(get)]
    pub target: i64,
    #[pyo3(get)]
    pub modifier: i64,
    #[pyo3(get)]
    pub success_rate: f64,
    #[pyo3(get)]
    pub fantastic_success_rate: f64,
    #[pyo3(get)]
    pub fantastic_failure_rate: f64,
    #[pyo3(get)]
    pub auto_fail_rate: f64,
    #[pyo3(get)]
    pub m_shown_rate: f64,
    #[pyo3(get)]
    pub total: SimResult,
}

#[pymethods]
impl MarvelSimResult {
    fn __repr__(&self) -> String {
        format!(
            "MarvelSimResult(n={}, target={}, modifier={}, success_rate={:.4})",
            self.n, self.target, self.modifier, self.success_rate
        )
    }
}

fn parse_marvel_policy(policy: &str) -> PyResult<core::EdgePolicy> {
    match policy {
        "reroll_lowest" => Ok(core::EdgePolicy::RerollLowest),
        "chase_fantastic" => Ok(core::EdgePolicy::ChaseFantastic),
        other => Err(PyValueError::new_err(format!(
            "invalid Marvel policy '{other}'; valid choices are 'reroll_lowest' and 'chase_fantastic'"
        ))),
    }
}

fn fantastic_label(fantastic: Option<core::Fantastic>) -> Option<String> {
    match fantastic {
        Some(core::Fantastic::Success) => Some("success".to_string()),
        Some(core::Fantastic::Failure) => Some("failure".to_string()),
        None => None,
    }
}

fn sim_result(result: core::SimResult) -> SimResult {
    SimResult {
        distribution: result.distribution,
        min: result.min,
        max: result.max,
        mean: result.mean,
        std_dev: result.std_dev,
        n: result.n,
    }
}

fn roll_outcome(outcome: core::RollOutcome) -> RollOutcome {
    let numeric = |kind: &str, value: i64| RollOutcome {
        kind: kind.to_string(),
        value,
        successes: None,
        advantages: None,
        triumphs: None,
        despairs: None,
        light: None,
        dark: None,
    };

    match outcome {
        core::RollOutcome::Numeric(n) => numeric("numeric", n),
        core::RollOutcome::Successes(n) => numeric("successes", n),
        core::RollOutcome::Marvel(o) => numeric("marvel", o.total),
        core::RollOutcome::Symbols(o) => RollOutcome {
            kind: "symbols".to_string(),
            value: o.successes,
            successes: Some(o.successes),
            advantages: Some(o.advantages),
            triumphs: Some(o.triumphs),
            despairs: Some(o.despairs),
            light: Some(o.light),
            dark: Some(o.dark),
        },
    }
}

fn marvel_outcome(outcome: core::MarvelOutcome) -> MarvelOutcome {
    MarvelOutcome {
        total: outcome.total,
        auto_fail: outcome.auto_fail,
        m_shown: outcome.m_shown,
    }
}

fn marvel_check(check: core::MarvelCheck) -> MarvelCheck {
    MarvelCheck {
        outcome: marvel_outcome(check.outcome),
        target: check.target,
        modifier: check.modifier,
        success: check.success,
        fantastic: fantastic_label(check.fantastic),
        expression: check.expression,
    }
}

fn marvel_sim_result(result: core::MarvelSimResult) -> MarvelSimResult {
    MarvelSimResult {
        n: result.n,
        target: result.target,
        modifier: result.modifier,
        success_rate: result.success_rate,
        fantastic_success_rate: result.fantastic_success_rate,
        fantastic_failure_rate: result.fantastic_failure_rate,
        auto_fail_rate: result.auto_fail_rate,
        m_shown_rate: result.m_shown_rate,
        total: sim_result(result.total),
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
        .map(|r| RollResult {
            outcome: roll_outcome(r.outcome),
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
        .map(sim_result)
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Roll a Marvel Multiverse check.
#[pyfunction]
#[pyo3(signature = (edges, troubles, target, modifier=0, policy="reroll_lowest"))]
fn roll_marvel(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: &str,
) -> PyResult<MarvelCheck> {
    core::roll_marvel(
        edges,
        troubles,
        target,
        modifier,
        parse_marvel_policy(policy)?,
    )
    .map(marvel_check)
    .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Simulate Marvel Multiverse checks.
#[pyfunction]
#[pyo3(signature = (edges, troubles, target, modifier=0, policy="reroll_lowest", n=10000))]
fn simulate_marvel(
    edges: u32,
    troubles: u32,
    target: i64,
    modifier: i64,
    policy: &str,
    n: usize,
) -> PyResult<MarvelSimResult> {
    core::simulate_marvel(
        edges,
        troubles,
        target,
        modifier,
        parse_marvel_policy(policy)?,
        n,
    )
    .map(marvel_sim_result)
    .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Python module for diceman.
#[pymodule]
fn diceman(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(roll, m)?)?;
    m.add_function(wrap_pyfunction!(simulate, m)?)?;
    m.add_function(wrap_pyfunction!(roll_marvel, m)?)?;
    m.add_function(wrap_pyfunction!(simulate_marvel, m)?)?;
    m.add_class::<RollOutcome>()?;
    m.add_class::<RollResult>()?;
    m.add_class::<SimResult>()?;
    m.add_class::<MarvelOutcome>()?;
    m.add_class::<MarvelCheck>()?;
    m.add_class::<MarvelSimResult>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_marvel_policy_accepts_valid_names() {
        assert_eq!(
            parse_marvel_policy("reroll_lowest").unwrap(),
            core::EdgePolicy::RerollLowest
        );
        assert_eq!(
            parse_marvel_policy("chase_fantastic").unwrap(),
            core::EdgePolicy::ChaseFantastic
        );
    }

    #[test]
    fn parse_marvel_policy_rejects_invalid_name_with_choices() {
        Python::initialize();
        let err = parse_marvel_policy("lowest").unwrap_err().to_string();

        assert!(err.contains("lowest"));
        assert!(err.contains("reroll_lowest"));
        assert!(err.contains("chase_fantastic"));
    }

    #[test]
    fn fantastic_label_matches_python_contract() {
        assert_eq!(
            fantastic_label(Some(core::Fantastic::Success)),
            Some("success".to_string())
        );
        assert_eq!(
            fantastic_label(Some(core::Fantastic::Failure)),
            Some("failure".to_string())
        );
        assert_eq!(fantastic_label(None), None);
    }

    #[test]
    fn marvel_check_maps_core_fields() {
        let check = marvel_check(core::MarvelCheck {
            outcome: core::MarvelOutcome {
                total: 13,
                auto_fail: false,
                m_shown: true,
            },
            target: 12,
            modifier: 1,
            success: true,
            fantastic: Some(core::Fantastic::Success),
            expression: "3dMarvel[4, M, 3] = 13".to_string(),
        });

        assert_eq!(check.outcome.total, 13);
        assert!(!check.outcome.auto_fail);
        assert!(check.outcome.m_shown);
        assert_eq!(check.target, 12);
        assert_eq!(check.modifier, 1);
        assert!(check.success);
        assert_eq!(check.fantastic, Some("success".to_string()));
        assert_eq!(check.expression, "3dMarvel[4, M, 3] = 13");
    }

    #[test]
    fn marvel_sim_result_reuses_total_sim_result() {
        let sim = marvel_sim_result(core::MarvelSimResult {
            n: 2,
            target: 12,
            modifier: 0,
            success_rate: 0.5,
            fantastic_success_rate: 0.5,
            fantastic_failure_rate: 0.0,
            auto_fail_rate: 0.0,
            m_shown_rate: 0.5,
            total: core::SimResult {
                distribution: HashMap::from([(10, 1), (13, 1)]),
                min: 10,
                max: 13,
                mean: 11.5,
                std_dev: 1.5,
                n: 2,
            },
        });

        assert_eq!(sim.n, 2);
        assert_eq!(sim.target, 12);
        assert_eq!(sim.modifier, 0);
        assert_eq!(sim.success_rate, 0.5);
        assert_eq!(sim.fantastic_success_rate, 0.5);
        assert_eq!(sim.fantastic_failure_rate, 0.0);
        assert_eq!(sim.auto_fail_rate, 0.0);
        assert_eq!(sim.m_shown_rate, 0.5);
        assert_eq!(sim.total.distribution, HashMap::from([(10, 1), (13, 1)]));
        assert_eq!(sim.total.n, 2);
    }

    #[test]
    fn marvel_functions_accept_default_argument_values() {
        let check = roll_marvel(0, 0, 10, 0, "reroll_lowest").unwrap();
        assert_eq!(check.target, 10);
        assert_eq!(check.modifier, 0);
        assert!(check.expression.starts_with("3dMarvel["));

        let sim = simulate_marvel(0, 0, 10, 0, "reroll_lowest", 1).unwrap();
        assert_eq!(sim.n, 1);
        assert_eq!(sim.target, 10);
        assert_eq!(sim.modifier, 0);
        assert_eq!(sim.total.n, 1);
    }

    #[test]
    fn roll_outcome_maps_symbols_fields() {
        let outcome = roll_outcome(core::RollOutcome::Symbols(core::SymbolsOutcome {
            successes: 2,
            advantages: -1,
            triumphs: 1,
            despairs: 2,
            light: 3,
            dark: 4,
        }));

        assert_eq!(outcome.kind, "symbols");
        assert_eq!(outcome.value, 2);
        assert_eq!(outcome.successes, Some(2));
        assert_eq!(outcome.advantages, Some(-1));
        assert_eq!(outcome.triumphs, Some(1));
        assert_eq!(outcome.despairs, Some(2));
        assert_eq!(outcome.light, Some(3));
        assert_eq!(outcome.dark, Some(4));
    }

    #[test]
    fn roll_outcome_leaves_symbols_fields_none_for_non_symbols_kinds() {
        let numeric = roll_outcome(core::RollOutcome::Numeric(15));
        assert_eq!(numeric.kind, "numeric");
        assert_eq!(numeric.value, 15);
        assert_eq!(numeric.successes, None);
        assert_eq!(numeric.advantages, None);
        assert_eq!(numeric.triumphs, None);
        assert_eq!(numeric.despairs, None);
        assert_eq!(numeric.light, None);
        assert_eq!(numeric.dark, None);

        let successes = roll_outcome(core::RollOutcome::Successes(4));
        assert_eq!(successes.kind, "successes");
        assert_eq!(successes.value, 4);
        assert_eq!(successes.successes, None);

        let marvel = roll_outcome(core::RollOutcome::Marvel(core::MarvelOutcome {
            total: 13,
            auto_fail: false,
            m_shown: true,
        }));
        assert_eq!(marvel.kind, "marvel");
        assert_eq!(marvel.value, 13);
        assert_eq!(marvel.successes, None);
    }

    #[test]
    fn roll_exposes_symbols_kind_and_attributes() {
        let result = roll("2dAbility&1dDifficulty").unwrap();
        assert_eq!(result.outcome.kind, "symbols");
        assert!(result.outcome.successes.is_some());
        assert!(result.outcome.advantages.is_some());
        assert!(result.outcome.triumphs.is_some());
        assert!(result.outcome.despairs.is_some());
        assert!(result.outcome.light.is_some());
        assert!(result.outcome.dark.is_some());
    }

    #[test]
    fn simulate_over_narrative_notation_raises_non_numeric_outcome_error() {
        Python::initialize();
        let err = match simulate("2dAbility&1dDifficulty", 10) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected simulate over narrative notation to fail"),
        };

        assert!(err.contains("dice result is not numeric and cannot be used in arithmetic"));
    }
}
