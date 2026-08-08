set shell := ["bash", "-cu"]

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace

ci: fmt-check check test
