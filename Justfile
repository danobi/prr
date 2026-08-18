# prr-suggest task runner

default:
    @just --list

build:
    cargo build

release:
    cargo build --release

test:
    cargo test

fmt:
    cargo fmt

lint:
    cargo clippy --all-targets -- -D warnings

# Everything CI would run
check: fmt test lint

install:
    cargo install --path . --force

# Verify the Homebrew formula parses and passes brew's audit
brew-audit:
    brew audit --formula --strict tineoc/prr/prr-suggest
