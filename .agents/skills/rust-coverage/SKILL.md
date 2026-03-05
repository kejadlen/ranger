---
name: rust-coverage
description: Use when measuring code coverage in Rust projects, debugging uncovered lines, generating coverage reports, or setting up LLVM source-based coverage with cargo-llvm-cov
---

# Rust Source-Based Code Coverage

LLVM source-based coverage via `cargo-llvm-cov`, which wraps rustc's `-C instrument-coverage` flag.

## How It Works

1. **Compile** with `-C instrument-coverage` — rustc inserts counters at coverage-relevant spans.
2. **Run** the instrumented binary — writes `.profraw` files.
3. **Merge** raw profiles into `.profdata` with `llvm-profdata merge`.
4. **Report** with `llvm-cov show` / `llvm-cov export`.

`cargo-llvm-cov` handles all four steps automatically.

## Prerequisites

```bash
rustup component add llvm-tools       # ships llvm-profdata + llvm-cov
cargo install cargo-llvm-cov          # convenience wrapper
```

## Quick Reference

| Goal | Command |
|------|---------|
| Run tests + summary | `cargo llvm-cov` |
| Workspace coverage | `cargo llvm-cov --workspace` |
| Fail if below threshold | `cargo llvm-cov --fail-under-lines 100` |
| Text report (per-line) | `cargo llvm-cov --text` |
| HTML report | `cargo llvm-cov --html --open` |
| JSON export | `cargo llvm-cov --json` |
| LCOV export | `cargo llvm-cov --lcov --output-path lcov.info` |
| Include only specific test | `cargo llvm-cov --test cli` |
| Branch coverage (unstable) | `cargo llvm-cov --branch` |
| MC/DC coverage (unstable) | `cargo llvm-cov --mcdc` |
| Skip report, just instrument | `cargo llvm-cov --no-report` |
| Report from prior run | `cargo llvm-cov report --html` |
| Clean coverage artifacts | `cargo llvm-cov clean` |
| Show env vars it sets | `cargo llvm-cov show-env` |

## Filtering

```bash
# Only library tests
cargo llvm-cov --lib

# Only integration tests
cargo llvm-cov --test cli

# Only specific binary
cargo llvm-cov --bin ranger

# Exclude files from report
cargo llvm-cov --ignore-filename-regex 'bin/.*'

# Exclude package from test AND report
cargo llvm-cov --workspace --exclude some-crate

# Exclude from report but still test
cargo llvm-cov --workspace --exclude-from-report some-crate
```

## Reading the Text Report

```bash
cargo llvm-cov --text
```

Output shows each source line with execution count:

```
   46|      1|    fn display_formats_as_iso8601() {    # executed once
   32|      0|    fn encode_by_ref(                    # never executed
   10|       |use std::fmt;                            # non-executable (no counter)
```

Lines with `0` count are uncovered. Lines with no count are non-executable (declarations, imports, comments).

## Show Missing Lines

```bash
cargo llvm-cov --show-missing-lines
```

Appends a `Missing Lines` column to the summary table showing exact uncovered line ranges — useful for quickly spotting gaps without reading the full text report.

## cfg(coverage)

When built with `cargo-llvm-cov`, the cfg flags `coverage` and `coverage_nightly` are set. Use them to skip code that shouldn't run under coverage:

```rust
#[cfg(not(coverage))]
fn debug_only_thing() { /* ... */ }
```

Opt out with `--no-cfg-coverage` if these flags interfere.

## Manual Workflow (Without cargo-llvm-cov)

For environments where `cargo-llvm-cov` isn't available:

```bash
# 1. Build with coverage
RUSTFLAGS="-C instrument-coverage" cargo test --no-run

# 2. Run tests (produces .profraw files)
LLVM_PROFILE_FILE="ranger-%p-%m.profraw" cargo test

# 3. Merge profiles
llvm-profdata merge -sparse *.profraw -o ranger.profdata

# 4. Find the test binary
BINARY=$(cargo test --no-run --message-format=json 2>/dev/null \
  | jq -r 'select(.executable) | .executable')

# 5. Generate report
llvm-cov show "$BINARY" \
  --instr-profile=ranger.profdata \
  --format=html \
  --output-dir=coverage/ \
  --Xdemangler=rustfilt

# Or export as LCOV
llvm-cov export "$BINARY" \
  --instr-profile=ranger.profdata \
  --format=lcov > lcov.info
```

Use `rustfilt` as demangler for readable Rust symbol names (`cargo install rustfilt`).

## Common Pitfalls

**Multiple binaries**: `llvm-cov` needs all instrumented binaries passed via `-object`. `cargo-llvm-cov` handles this automatically. Manually, you must pass each binary:

```bash
llvm-cov show -object bin1 -object bin2 --instr-profile=merged.profdata
```

**Dead code appears uncovered**: Unreachable code still gets instrumented. If coverage matters, delete dead code rather than excluding it.

**Proc macros and build scripts**: Not instrumented by default. Use `--include-build-script` if needed. When using `--no-rustc-wrapper` with `--target`, proc macros won't show coverage.

**Doctests**: Experimental. Enable with `--doctests` (nightly only).

**Branch coverage**: Requires nightly. `--branch` enables it but the feature is unstable — expect rough edges with match arms and closures.

## This Project

This project uses `just coverage` which runs:

```bash
cargo llvm-cov --workspace --fail-under-lines 100
```

100% line coverage is enforced. When adding code, make sure every line is exercised by tests. Use `cargo llvm-cov --text` or `cargo llvm-cov --html --open` to find uncovered lines.
