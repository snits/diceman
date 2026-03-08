# Cumulative Success Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `--cumulative` and `--lte` flags to the `sim` CLI command that show cumulative probability of meeting/beating (or rolling under) each target value.

**Architecture:** Add `cumulative_gte()` and `cumulative_lte()` methods to `SimResult` in the core library (testable with existing infrastructure). Add CLI flags and rendering in main.rs. The rendering reuses the same bar-chart style as the existing distribution histogram.

**Tech Stack:** Rust, clap (existing), diceman core library

---

### Task 1: Add `cumulative_gte()` to SimResult

**Files:**
- Modify: `crates/diceman/src/sim.rs:26-71` (SimResult impl block)

**Step 1: Write the failing test**

Add to the existing test module in `crates/diceman/src/sim.rs`:

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman cumulative_gte`
Expected: FAIL — method `cumulative_gte` not found

**Step 3: Write minimal implementation**

Add to the `impl SimResult` block in `crates/diceman/src/sim.rs`:

```rust
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p diceman cumulative_gte`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/diceman/src/sim.rs
git commit -s -m "feat(sim): add cumulative_gte() to SimResult"
```

---

### Task 2: Add `cumulative_lte()` to SimResult

**Files:**
- Modify: `crates/diceman/src/sim.rs:26-71` (SimResult impl block)

**Step 1: Write the failing test**

Add to the existing test module in `crates/diceman/src/sim.rs`:

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p diceman cumulative_lte`
Expected: FAIL — method `cumulative_lte` not found

**Step 3: Write minimal implementation**

Add to the `impl SimResult` block in `crates/diceman/src/sim.rs`:

```rust
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p diceman cumulative_lte`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/diceman/src/sim.rs
git commit -s -m "feat(sim): add cumulative_lte() to SimResult"
```

---

### Task 3: Add CLI flags and cumulative rendering

**Files:**
- Modify: `crates/diceman-cli/src/main.rs:15-37` (Commands enum, Sim variant)
- Modify: `crates/diceman-cli/src/main.rs:54-67` (match arm for Sim)
- Add function after `print_sim_histogram`

**Step 1: Add `--cumulative` and `--lte` flags to the Sim variant**

In the `Commands` enum, update the `Sim` variant to add two new fields:

```rust
Sim {
    /// Dice expression (e.g., "2d6")
    expression: String,

    /// Number of trials to run
    #[arg(short, long, default_value = "10000")]
    n: usize,

    /// Output as JSON
    #[arg(long)]
    json: bool,

    /// Show cumulative success probability
    #[arg(long)]
    cumulative: bool,

    /// Use "roll under" direction (implies --cumulative)
    #[arg(long)]
    lte: bool,
},
```

**Step 2: Update the match arm to destructure and use the new flags**

Update the `Commands::Sim` match arm:

```rust
Commands::Sim { expression, n, json, cumulative, lte } => {
    match diceman::simulate(&expression, n) {
        Ok(result) => {
            if json {
                print_sim_json(&result);
            } else {
                print_sim_histogram(&expression, &result);
                if cumulative || lte {
                    println!();
                    print_cumulative_histogram(&result, lte);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
```

**Step 3: Add the `print_cumulative_histogram` function**

Add after `print_sim_histogram`:

```rust
fn print_cumulative_histogram(result: &diceman::SimResult, lte: bool) {
    let cumulative = if lte {
        result.cumulative_lte()
    } else {
        result.cumulative_gte()
    };

    let direction = if lte { "<=" } else { ">=" };
    println!("Cumulative ({} target):", direction);
    println!();

    let max_bar_width = 40;

    for (value, pct) in &cumulative {
        let bar_width = (pct * max_bar_width as f64) as usize;
        let bar: String = "█".repeat(bar_width);

        println!("{:>4}: {:40} {:5.1}%", value, bar, pct * 100.0);
    }
}
```

**Step 4: Verify it compiles and run manually**

Run: `cargo build -p diceman-cli`
Expected: Compiles successfully

Run: `cargo run --bin diceman -- sim "2d6" -n 10000 --cumulative`
Expected: Shows distribution followed by cumulative >=  chart

Run: `cargo run --bin diceman -- sim "2d6" -n 10000 --lte`
Expected: Shows distribution followed by cumulative <= chart

Run: `cargo run --bin diceman -- sim "2d6" -n 10000`
Expected: Shows distribution only (unchanged behavior)

**Step 5: Commit**

```bash
git add crates/diceman-cli/src/main.rs
git commit -s -m "feat(cli): add --cumulative and --lte flags to sim command"
```

---

### Task 4: Run full test suite

**Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass, no regressions

**Step 2: Final manual smoke test**

Run: `cargo run --bin diceman -- sim "1d20+5" --cumulative`
Run: `cargo run --bin diceman -- sim "1d20+5" --lte`
Run: `cargo run --bin diceman -- sim "4d6kh3" --cumulative`
Expected: Output looks correct and readable
