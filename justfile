lint:
    cargo clippy --all --all-features -- -W clippy::pedantic

fmt:
    cargo +nightly fmt --all

test:
    cargo test

test-all:
    cargo test --all-features

test-no-default:
    cargo test --no-default-features

# Network test against the live rules server
test-fetch:
    cargo test --features fetch,rustls --test fetch -- --ignored

# Prove the crate builds for a target without std
build-no-std:
    cargo build --no-default-features --features embedded-rules --target x86_64-unknown-none

check: test test-all test-no-default build-no-std lint
