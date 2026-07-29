---
name: validate
description: >
  Run the full CI validation pipeline locally. Use this workflow before pushing
  any code changes to ensure they pass CI. Follows the exact same order as the
  GitHub Actions CI workflow.
triggers:
  - before pushing code
  - after finishing a feature or fix
  - when asked to "validate", "check", or "run CI"
---

# Validate Pipeline

Run the full CI pipeline locally, in the exact order CI executes it.
Abort on the first failure — do not continue to later stages if an earlier one fails.

## Prerequisites

Ensure system dependencies are installed (only needed once):

```bash
# GPUI dependencies (Linux)
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev
```

## Pipeline Steps (execute in order)

### 1. TOML Formatting

```bash
taplo format --check
```

If this fails, fix with `taplo format` and re-run.

### 2. Rust Formatting

```bash
cargo fmt --all --check
```

If this fails, fix with `cargo fmt --all` and re-run.

### 3. Clippy Lints

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

All warnings are treated as errors. Fix every diagnostic before proceeding.

### 4. Tests

```bash
cargo test --workspace
```

All tests must pass. If a test fails, investigate and fix before continuing.

## Quick Validate (fast-path)

When you only need a quick confidence check without full compilation:

```bash
cargo check --workspace
```

This is faster than `cargo build` but does NOT run tests or lints.

## Single-Crate Validation

To validate a single crate:

```bash
cargo test -p <crate-name>
cargo clippy -p <crate-name> --all-targets -- -D warnings
```

Valid crate names: `fncc-macros`, `fncc-core`, `fncc-runtime`, `fncc`, `fncc-example`, `xtask`.

## Reporting

After running the pipeline, report:
- ✅ Which steps passed
- ❌ Which step failed (if any), with the error output
- 🔧 What was fixed (if auto-fixable issues were resolved)
