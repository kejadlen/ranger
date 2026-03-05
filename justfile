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

# Run tests with coverage reporting (100% required for library code)
coverage:
    #!/usr/bin/env bash
    set -euo pipefail
    export RUSTFLAGS="-Cinstrument-coverage"
    export LLVM_PROFILE_FILE="target/coverage/%p-%m.profraw"
    rm -rf target/coverage
    cargo test --workspace
    REPORT=$(grcov target/coverage \
        --binary-path ./target/debug/ \
        -s . \
        -t covdir \
        --ignore-not-existing \
        --keep-only 'src/**' \
        --ignore 'src/bin/**' \
        --excl-line 'cov-excl-line' \
        --excl-start 'cov-excl-start' \
        --excl-stop 'cov-excl-stop')
    echo "$REPORT" | jq -r '
        def files:
            to_entries[] | .value |
            if .children then .children | files
            else "\(.name): \(.coveragePercent)% (\(.linesCovered)/\(.linesTotal))"
            end;
        .children | files
    '
    COVERAGE=$(echo "$REPORT" | jq '.coveragePercent')
    echo ""
    echo "Total: ${COVERAGE}%"
    if [ "$(echo "$COVERAGE < 100" | bc -l)" -eq 1 ]; then
        echo "ERROR: Coverage is below 100%"
        exit 1
    fi

# Run all checks: fmt, clippy, tests with coverage
all: fmt clippy coverage

# Install ranger from source
install:
    cargo install --locked --path .
