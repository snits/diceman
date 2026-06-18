# FastRng Checkpoint Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a domain-style checkpoint/restore API to `diceman::FastRng` so consumers can persist and restore RNG progress.

**Architecture:** Introduce `RngCheckpoint` as the public persistence boundary and keep `fastrand::Rng` private inside `FastRng`. `FastRng::checkpoint()` wraps `fastrand::Rng::get_seed()`, and `FastRng::restore()` wraps `fastrand::Rng::seed()`.

**Tech Stack:** Rust 2021, `fastrand 2.3.0`, existing optional `serde` feature, unit tests in `crates/diceman/src/roller.rs`.

---

## File Structure

- Modify `crates/diceman/src/roller.rs`
  - Add `RngCheckpoint`.
  - Add `RngCheckpoint::from_state()` and `RngCheckpoint::state()`.
  - Add `FastRng::checkpoint()` and `FastRng::restore()`.
  - Add unit tests for checkpoint restore and raw state round trip.
  - Add serde-feature unit test for JSON round trip.
- Modify `crates/diceman/src/lib.rs`
  - Re-export `RngCheckpoint` from the crate root.

## Task 1: Add FastRng Checkpoint Restore Behavior

**Files:**
- Modify: `crates/diceman/src/roller.rs:18-40`
- Modify: `crates/diceman/src/lib.rs:41`

- [ ] **Step 1: Write the failing checkpoint restore test**

Add this test to the main `#[cfg(test)] mod tests` in `crates/diceman/src/roller.rs`:

```rust
#[test]
fn fast_rng_restore_repeats_rolls_after_checkpoint() {
    let mut rng = FastRng::with_seed(42);

    let _before_checkpoint = rng.roll(20);
    let checkpoint = rng.checkpoint();

    let first_after_checkpoint = rng.roll(20);
    let _advanced = rng.roll(20);

    rng.restore(checkpoint);

    assert_eq!(rng.roll(20), first_after_checkpoint);
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
cargo test -p diceman fast_rng_restore_repeats_rolls_after_checkpoint
```

Expected result: compile failure because `FastRng` has no `checkpoint` or `restore` methods.

- [ ] **Step 3: Add the minimal checkpoint API**

In `crates/diceman/src/roller.rs`, replace the `FastRng` section with:

```rust
/// Persistable checkpoint for a random number generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RngCheckpoint {
    state: u64,
}

/// Default RNG using fastrand.
pub struct FastRng(fastrand::Rng);

impl FastRng {
    pub fn new() -> Self {
        Self(fastrand::Rng::new())
    }

    pub fn with_seed(seed: u64) -> Self {
        Self(fastrand::Rng::with_seed(seed))
    }

    pub fn checkpoint(&self) -> RngCheckpoint {
        RngCheckpoint {
            state: self.0.get_seed(),
        }
    }

    pub fn restore(&mut self, checkpoint: RngCheckpoint) {
        self.0.seed(checkpoint.state);
    }
}
```

- [ ] **Step 4: Run the focused test and verify it passes**

Run:

```bash
cargo test -p diceman fast_rng_restore_repeats_rolls_after_checkpoint
```

Expected result: the test passes.

- [ ] **Step 5: Write the failing state round-trip test**

Add this test to the same `tests` module in `crates/diceman/src/roller.rs`:

```rust
#[test]
fn rng_checkpoint_rebuilds_from_persisted_state() {
    let mut rng = FastRng::with_seed(99);

    let _before_checkpoint = rng.roll(12);
    let checkpoint = rng.checkpoint();
    let persisted_state = checkpoint.state();

    let first_after_checkpoint = rng.roll(12);

    let rebuilt = RngCheckpoint::from_state(persisted_state);
    rng.restore(rebuilt);

    assert_eq!(rng.roll(12), first_after_checkpoint);
}
```

- [ ] **Step 6: Run the state round-trip test and verify it fails**

Run:

```bash
cargo test -p diceman rng_checkpoint_rebuilds_from_persisted_state
```

Expected result: compile failure because `RngCheckpoint` has no `state` or `from_state` methods.

- [ ] **Step 7: Add the state persistence methods**

Add this impl below `RngCheckpoint` in `crates/diceman/src/roller.rs`:

```rust
impl RngCheckpoint {
    /// Create a checkpoint from a previously persisted state value.
    pub fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// Return the persistable state value for this checkpoint.
    pub fn state(self) -> u64 {
        self.state
    }
}
```

- [ ] **Step 8: Run the state round-trip test and verify it passes**

Run:

```bash
cargo test -p diceman rng_checkpoint_rebuilds_from_persisted_state
```

Expected result: the test passes.

- [ ] **Step 9: Write the failing crate-root re-export test**

Add this test to `crates/diceman/src/lib.rs` in its existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn rng_checkpoint_is_available_from_crate_root() {
    let mut rng = FastRng::with_seed(123);
    let checkpoint: RngCheckpoint = rng.checkpoint();
    let expected = rng.roll(6);

    rng.restore(checkpoint);

    assert_eq!(rng.roll(6), expected);
}
```

- [ ] **Step 10: Run the crate-root test and verify it fails**

Run:

```bash
cargo test -p diceman rng_checkpoint_is_available_from_crate_root
```

Expected result: compile failure because `RngCheckpoint` is not re-exported from the crate root.

- [ ] **Step 11: Re-export `RngCheckpoint` from the crate root**

In `crates/diceman/src/lib.rs`, update the re-export:

```rust
pub use roller::{DieResult, FastRng, Rng, RngCheckpoint, RollResult};
```

- [ ] **Step 12: Run the crate-root test and verify it passes**

Run:

```bash
cargo test -p diceman rng_checkpoint_is_available_from_crate_root
```

Expected result: the test passes.

## Task 2: Add Serde Checkpoint Round Trip

**Files:**
- Modify: `crates/diceman/src/roller.rs:1162-1200`

- [ ] **Step 1: Write the failing serde test**

Add this test to the existing `#[cfg(all(test, feature = "serde"))] mod serde_tests` in `crates/diceman/src/roller.rs`:

```rust
#[test]
fn rng_checkpoint_deserializes_for_restore() {
    let mut rng = FastRng::with_seed(7);

    let _before_checkpoint = rng.roll(10);
    let checkpoint = rng.checkpoint();
    let expected = rng.roll(10);

    let json = serde_json::to_string(&checkpoint).unwrap();
    let restored_checkpoint: RngCheckpoint = serde_json::from_str(&json).unwrap();

    rng.restore(restored_checkpoint);

    assert_eq!(rng.roll(10), expected);
}
```

- [ ] **Step 2: Run the serde test and verify it fails before serde derives exist**

Run:

```bash
cargo test -p diceman --features serde rng_checkpoint_deserializes_for_restore
```

Expected result before deriving serde traits: compile failure because `RngCheckpoint` does not implement `serde::Serialize` or `serde::Deserialize`.

- [ ] **Step 3: Add serde derives**

Ensure `RngCheckpoint` has this attribute:

```rust
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
```

- [ ] **Step 4: Run the serde test and verify it passes**

Run:

```bash
cargo test -p diceman --features serde rng_checkpoint_deserializes_for_restore
```

Expected result: the test passes.

## Task 3: Verify Feature Integration

**Files:**
- No additional source changes expected.

- [ ] **Step 1: Run core crate tests without optional features**

Run:

```bash
cargo test -p diceman
```

Expected result: all `diceman` tests pass.

- [ ] **Step 2: Run core crate tests with serde enabled**

Run:

```bash
cargo test -p diceman --features serde
```

Expected result: all `diceman` tests pass, including serde checkpoint round trip.

- [ ] **Step 3: Run formatting check**

Run:

```bash
cargo fmt --check
```

Expected result: formatting is clean.

- [ ] **Step 4: Review public API surface**

Confirm these public items exist:

```rust
diceman::FastRng::checkpoint
diceman::FastRng::restore
diceman::RngCheckpoint
diceman::RngCheckpoint::from_state
diceman::RngCheckpoint::state
```

Expected result: consumers can checkpoint, persist a state value, rebuild a checkpoint, and restore without accessing `fastrand::Rng`.
