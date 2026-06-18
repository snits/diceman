# FastRng Checkpoint Restore Design

## Goal

Allow consumers of `diceman::FastRng` to save RNG progress with their own application state and later restore the generator so subsequent rolls continue from the saved point.

## Current State

`FastRng` privately owns a `fastrand::Rng` and currently exposes only `new()` and `with_seed(seed)`. Consumers can create deterministic runs from an initial seed, but they cannot checkpoint a generator after rolls have advanced it.

`fastrand::Rng` already supports the required primitive operations:

- `get_seed()` returns the generator's current internal state.
- `seed(seed)` restores the generator to that state.

## Public API

Add a domain-level checkpoint value rather than exposing the inner `fastrand::Rng`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RngCheckpoint {
    state: u64,
}

impl RngCheckpoint {
    pub fn from_state(state: u64) -> Self;
    pub fn state(self) -> u64;
}

impl FastRng {
    pub fn checkpoint(&self) -> RngCheckpoint;
    pub fn restore(&mut self, checkpoint: RngCheckpoint);
}
```

`RngCheckpoint` keeps the fastrand implementation detail behind a diceman type while still giving non-serde consumers a simple `u64` value they can persist manually. With the existing `serde` feature enabled, the checkpoint should serialize and deserialize directly.

`diceman::RngCheckpoint` should be re-exported from the crate root beside `FastRng` and `Rng`.

## Data Flow

1. Consumer creates or receives a `FastRng`.
2. Consumer rolls through `roll_with_rng`.
3. Consumer calls `rng.checkpoint()` and persists the `RngCheckpoint`.
4. Consumer recreates `FastRng` with any constructor.
5. Consumer calls `rng.restore(checkpoint)`.
6. Subsequent rolls match the sequence that would have followed the checkpoint.

## Error Handling

No fallible API is needed. `fastrand::Rng::seed(u64)` accepts any `u64`, so every `RngCheckpoint` state is restorable.

## Testing

Use TDD in `crates/diceman/src/roller.rs`:

- A failing unit test should checkpoint a seeded `FastRng`, advance it, restore the checkpoint, and confirm the next roll repeats.
- A failing unit test should prove `RngCheckpoint::from_state(checkpoint.state())` preserves the checkpoint.
- With `serde` enabled, a failing unit test should serialize and deserialize a checkpoint, restore from the deserialized value, and confirm the sequence repeats.

Run the focused tests first, then the full core crate tests with and without the `serde` feature.
