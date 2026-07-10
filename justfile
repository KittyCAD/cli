lint:
    cargo clippy --all-features --all-targets -- -D warnings

lint-fix:
    cargo clippy --all-features --all-targets --fix

install:
    cargo install --path . --locked

test:
    cargo nextest run
