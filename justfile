# Run all checks by default
default: all

# Format all code
fmt:
    cargo fmt --all

# Type-check the workspace
check:
    cargo check --workspace

# Lint with clippy (deny warnings)
clippy:
    cargo clippy --workspace -- -D warnings

# Run tests with coverage reporting
coverage:
    cargo llvm-cov --workspace --fail-under-lines 100

# Run all checks: fmt, clippy, tests with coverage
all: fmt clippy coverage
