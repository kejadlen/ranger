FROM ramekin-agent

RUN apt-get update && apt-get install -y --no-install-recommends \
    bc \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*

# Rust nightly (edition 2024)
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH="/usr/local/cargo/bin:$PATH"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain nightly --profile minimal \
    && rustup component add llvm-tools-preview rustfmt clippy \
    && cargo install grcov


ENV RANGER_DEFAULT_BACKLOG=ranger

WORKDIR /workspace

ENTRYPOINT ["pi"]
