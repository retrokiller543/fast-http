set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Compile the whole workspace.
build:
    cargo build --workspace --all-features

# Fast type check, including tests, examples and benches.
check:
    cargo check --workspace --all-targets --all-features

# Same invocation as CI: zero warnings allowed.
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Format the workspace.
fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

# Unit and integration tests; nextest does not run doctests.
test *args:
    cargo nextest run --workspace --all-features {{ args }}

test-doc:
    cargo test --workspace --all-features --doc

# Everything that must pass before a commit.
verify: fmt-check clippy deny test test-doc

# Banned crates (openssl, native-tls, rsa), advisories and sources; see `deny.toml`.
deny:
    cargo deny --all-features check bans advisories sources

# Compile every bench without running it.
bench-check:
    cargo bench --workspace --all-features --no-run

# Criterion keeps reports and baselines in `benchmarks/criterion` (override with `CRITERION_HOME`),
# outside `target/`. Bench targets: core, format, router, routing, stack. Arguments after `--` go to
# Criterion: `just bench router -- --save-baseline before`. With no target, every bench runs.
bench *args:
    #!/usr/bin/env bash
    set -euo pipefail

    export CRITERION_HOME="${CRITERION_HOME:-{{ justfile_directory() }}/benchmarks/criterion}"
    targets=()
    criterion=()
    forwarding=false

    for arg in {{ args }}; do
        if [[ "$forwarding" == true ]]; then
            criterion+=("$arg")
        elif [[ "$arg" == "--" ]]; then
            forwarding=true
        else
            targets+=(--bench "$arg")
        fi
    done

    cargo bench --workspace --all-features "${targets[@]}" -- "${criterion[@]}"

# Run the router example end to end.
example:
    cargo run -p fast-http-router --example users

clean:
    cargo clean
