# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo check                    # Fast type checking
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo test -p diceman          # Test core library only
cargo test test_name           # Run specific test
cargo run --bin diceman -- roll "4d6kh3"   # Run CLI
cargo run --bin diceman -- sim "2d6" -n 10000  # Simulate distribution
```

### Python Bindings

```bash
cd crates/diceman-py
maturin develop   # Install in current venv
maturin build     # Build wheel
```

## Architecture

This is a Rust workspace with three crates:

### Core Library (`crates/diceman`)

Classic compiler pipeline for dice notation:

```
Input ("4d6kh3") → Lexer → Parser → AST → Evaluator → RollResult
```

- **lexer.rs**: Tokenizes dice notation into `Token` enum
- **parser.rs**: Recursive descent parser producing `Expr` AST
- **ast.rs**: Expression types (`Expr`, `Roll`, `Modifier`, `Condition`)
- **roller.rs**: Evaluates AST with `Rng` trait for testability
- **sim.rs**: Monte Carlo simulation over many rolls

Modifier application order in roller: **reroll → explode → keep/drop**

The `Rng` trait allows injecting deterministic values for testing via `TestRng`.

### Python Bindings (`crates/diceman-py`)

PyO3 wrapper exposing `roll()` and `simulate()` to Python. Uses `::diceman as core` to avoid naming collision with the pymodule.

### CLI (`crates/diceman-cli`)

Thin wrapper with `roll` and `sim` subcommands. Supports `--json` output.

## Dice Notation (Roll20 Style)

- Basic: `2d6`, `1d20`, `d%` (d100), `4dF` (fudge)
- Arithmetic: `2d6 + 5`, `(1d6 + 2) * 3`
- Keep: `4d6kh3` (keep highest 3), `2d20kl1` (keep lowest 1)
- Explode: `1d6!` (explode on max), `1d6!>5` (explode on 5+), `1d6!p` (penetrating)
- Reroll: `1d6r` (reroll 1s), `1d6r<3` (reroll below 3)

## Issue Tracking

Uses beads (`bd`) for issue tracking. See AGENTS.md for workflow.
