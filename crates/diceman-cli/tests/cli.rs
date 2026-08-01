// ABOUTME: Integration tests that exercise the built `diceman` binary end to end.
// ABOUTME: Covers the sim --json output contract and process exit-code behavior.

use std::process::Command;

fn diceman_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_diceman"))
}

#[test]
fn sim_json_carries_the_documented_key_set() {
    let output = diceman_cmd()
        .args(["sim", "2d6", "-n", "100", "--json"])
        .output()
        .expect("failed to run diceman binary");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("sim --json output must be valid JSON");
    let object = value
        .as_object()
        .expect("sim --json output must be a JSON object");

    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["distribution", "max", "mean", "min", "n", "std_dev"]);

    // Pin values to keys, not just key names — a mutant swapping which
    // field binds to which key (e.g. min <-> max) would pass a
    // key-set-only check while still shipping a broken contract.
    let n = object["n"].as_u64().expect("n must be a number");
    assert_eq!(n, 100);

    let min = object["min"].as_i64().expect("min must be a number");
    let max = object["max"].as_i64().expect("max must be a number");
    assert!((2..=12).contains(&min), "min={min} out of 2d6 range");
    assert!((2..=12).contains(&max), "max={max} out of 2d6 range");
    assert!(min <= max, "min={min} must be <= max={max}");

    let mean = object["mean"].as_f64().expect("mean must be a number");
    assert!(
        (min as f64) <= mean && mean <= (max as f64),
        "mean={mean} must fall within [min={min}, max={max}]"
    );

    let std_dev = object["std_dev"]
        .as_f64()
        .expect("std_dev must be a number");
    assert!(std_dev >= 0.0, "std_dev={std_dev} must be non-negative");

    let distribution = object["distribution"]
        .as_object()
        .expect("distribution must be a JSON object");
    let total: u64 = distribution
        .values()
        .map(|v| v.as_u64().expect("distribution counts must be numbers"))
        .sum();
    assert_eq!(total, n, "distribution counts must sum to n");
}

#[test]
fn invalid_notation_exits_1_with_stderr_message() {
    let output = diceman_cmd()
        .args(["roll", "not-dice-notation"])
        .output()
        .expect("failed to run diceman binary");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("Error:"));
}

#[test]
fn valid_roll_exits_0() {
    let output = diceman_cmd()
        .args(["roll", "4d6kh3"])
        .output()
        .expect("failed to run diceman binary");

    assert_eq!(output.status.code(), Some(0));
    assert!(!output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
