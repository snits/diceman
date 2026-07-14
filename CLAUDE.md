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

## Dice Notation

See README.md for full notation reference, or run `diceman notation`.

## Issue Tracking

Uses kata for issue tracking. See AGENTS.md for workflow.

<!-- BEGIN KATA (managed by `kata init --with-agents`) -->
## kata issue tracker

This project uses [kata](https://github.com/kenn-io/kata) as its shared issue
ledger. Run `kata quickstart` at the start of each session for the full agent
contract. The short version:

- Search before creating: `kata search "<keywords>" --agent`.
- Prefer updating existing issues over duplicates (`kata comment`, `kata label add`, `kata edit`).
- Default to `--agent` for ordinary reads and mutations; use `--json` only when a script needs structured data.
- Close only verified work: `kata close <ref> --done --message "<scope + verification>" --commit <sha>`.
- If work is incomplete, label `needs-review` and comment what remains rather than closing.
- Never `kata delete` or `kata purge` without explicit user authorization.

## kata work.* conventions (agent orchestration)

When working a kata-tracked issue, keep its `work.*` metadata truthful
(see docs/operations/agent-orchestration.md for the full recipe):

- On claim/start: `kata meta set <ref> work.attention ok`; if the work has a
  dedicated branch, stamp it once with `kata meta set <ref> work.branch <branch>`.
- Signal live state: `kata meta set <ref> work.attention stuck|needs-human|ok`
  plus a one-line `work.attention_msg` saying why. Raise `stuck` when you cannot
  proceed, `needs-human` when you want review; clear back to `ok` when unblocked.
- Never stop with the signal stale: close the issue, or leave the attention
  pair reflecting the hand-off.
- Coordinators read `work.*` on issues they delegated; only the working agent
  writes them. `work.*` on closed issues is meaningless.
<!-- END KATA -->
