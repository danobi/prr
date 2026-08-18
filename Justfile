# prr-suggest task runner. CI runs these same recipes.

default:
    @just --list

build:
    cargo build

release:
    cargo build --release

test:
    cargo test --verbose

# Compile tests without running them (catches build breaks fast in CI)
build-tests:
    cargo test --verbose --no-run

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

# Statically linked musl build, plus a check that it really is static
static:
    cargo build --verbose --release --target=x86_64-unknown-linux-musl --features vendored-openssl
    ldd ./target/x86_64-unknown-linux-musl/release/prr 2>&1 | grep -q "statically linked"

# Everything CI runs
ci: fmt-check build-tests test lint

install:
    cargo install --path . --force

# Verify the Homebrew formula passes brew's audit (requires the tap)
brew-audit:
    brew audit --formula --strict tineoc/prr/prr-suggest
