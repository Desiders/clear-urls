lint:
    cargo clippy --all --all-features -- -W clippy::pedantic

fmt:
    cargo +nightly fmt --all
