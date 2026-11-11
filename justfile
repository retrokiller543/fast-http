set shell := ["bash", "-euo", "pipefail", "-c"]

# Comparison benchmarks live in their own workspace, so competitor crates never reach the
# fast-http crates' lockfile or feature resolution.
benchmarks := "--manifest-path benchmarks/Cargo.toml"

default:
    @just --list

# Compile the whole workspace.
build:
    cargo build --workspace --all-features

# Fast type check, including tests, examples and benches, in both workspaces.
check:
    cargo check --workspace --all-targets --all-features
    cargo check {{ benchmarks }} --workspace --all-targets --all-features

# Each library crate alone, so a feature hidden by workspace feature unification shows up.
check-isolated:
    for crate in core format layer macros multipart router server transport ws; do cargo check -p "fast-http-$crate" --all-targets; done

# Same invocation as CI: zero warnings allowed.
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo clippy {{ benchmarks }} --workspace --all-targets --all-features -- -D warnings

# Format the workspace.
fmt:
    cargo fmt --all
    cargo fmt {{ benchmarks }} --all

fmt-check:
    cargo fmt --all -- --check
    cargo fmt {{ benchmarks }} --all -- --check

# Unit and integration tests; nextest does not run doctests.
test *args:
    cargo nextest run --workspace --all-features {{ args }}

# Tests of the benchmark harnesses.
test-benchmarks *args:
    cargo nextest run {{ benchmarks }} --workspace {{ args }}

test-doc:
    cargo test --workspace --all-features --doc

# Everything that must pass before a commit.
verify: fmt-check clippy check-isolated deny test test-benchmarks test-doc

# The benchmarks workspace picks up `deny.toml` from the parent directory.
[doc("Banned crates (openssl, native-tls, rsa), advisories, licenses and sources; see `deny.toml`.")]
deny:
    cargo deny --all-features check bans advisories licenses sources
    cargo deny {{ benchmarks }} --all-features check bans advisories licenses sources

# Compile every bench without running it.
bench-check:
    cargo bench --workspace --all-features --no-run
    cargo bench {{ benchmarks }} --workspace --all-features --no-run

# Wall time plus allocation count and bytes per iteration; `just bench-counters` adds CPU counters.
# Criterion keeps reports and baselines in `benchmarks/criterion` (override with `CRITERION_HOME`),
# outside `target/`. Bench targets: core, format, router, engine (fast-http alone) and routing, stack (against
# axum, matchit and actix, in the benchmarks workspace). Arguments after `--` go to Criterion:
# `just bench router -- --save-baseline before`. With no target, every bench runs.
[doc("Criterion benches; targets core, format, router, engine, routing, stack; Criterion args after `--`.")]
bench *args:
    #!/usr/bin/env bash
    set -euo pipefail

    export CRITERION_HOME="${CRITERION_HOME:-{{ justfile_directory() }}/benchmarks/criterion}"
    own=()
    comparison=()
    criterion=()
    forwarding=false

    for arg in {{ args }}; do
        if [[ "$forwarding" == true ]]; then
            criterion+=("$arg")
        elif [[ "$arg" == "--" ]]; then
            forwarding=true
        elif [[ "$arg" == routing || "$arg" == stack ]]; then
            comparison+=(--bench "$arg")
        else
            own+=(--bench "$arg")
        fi
    done

    if [[ ${#own[@]} -eq 0 && ${#comparison[@]} -eq 0 ]]; then
        cargo bench --workspace --all-features -- "${criterion[@]}"
        cargo bench {{ benchmarks }} --workspace --all-features -- "${criterion[@]}"
    fi

    if [[ ${#own[@]} -gt 0 ]]; then
        cargo bench --workspace --all-features "${own[@]}" -- "${criterion[@]}"
    fi

    if [[ ${#comparison[@]} -gt 0 ]]; then
        cargo bench {{ benchmarks }} --workspace --all-features "${comparison[@]}" -- "${criterion[@]}"
    fi

# `just bench` with `FAST_HTTP_COUNTERS=1`: instructions, cycles and IPC join wall time and
# allocations. No root needed; the bench exits if the counters cannot be opened.
[doc("Criterion benches with CPU counters (instructions, cycles, IPC); same arguments as `bench`.")]
bench-counters *args:
    FAST_HTTP_COUNTERS=1 just bench {{ args }}

# Builds the servers in release, runs each in turn and writes `benchmarks/load/results/<unix time>/`.
# Takes several minutes with the defaults; see `benchmarks/load/README.md` for the flags.
[doc("Network comparison against axum and actix-web, driven by `oha` (`cargo install oha`).")]
load *args:
    cargo build {{ benchmarks }} --release -p fast-http-load --bins
    benchmarks/target/release/load --output benchmarks/load/results {{ args }}

# Runs the load comparison in a Linux container with server and oha pinned to separate cores.
[doc("Load comparison in Docker with pinned CPUs; `just load-linux 0-3 4-7 --repeat 3`.")]
load-linux server_cpus client_cpus *args:
    docker build -q -t fast-http-load benchmarks/load
    docker run --rm -v "{{ justfile_directory() }}:/src" -v fast-http-load-target:/src/benchmarks/target fast-http-load \
        bash -c "cargo build --release --manifest-path benchmarks/Cargo.toml -p fast-http-load --bins && \
        benchmarks/target/release/load --output benchmarks/load/results --server-cpus {{ server_cpus }} --client-cpus {{ client_cpus }} {{ args }}"

# Writes `benchmarks/load/results/profile-<framework>-<scenario>.txt` (macOS `sample`) or `.data`
# (Linux `perf record`); scenario is a path such as `/users/42`.
[doc("CPU profile of one server under load; `just profile-server fast-http /users/42`.")]
profile-server framework path="/health":
    #!/usr/bin/env bash
    set -euo pipefail

    cargo build --manifest-path benchmarks/Cargo.toml --profile profiling -p fast-http-load --bins
    mkdir -p benchmarks/load/results
    out="benchmarks/load/results/profile-{{ framework }}-$(echo '{{ path }}' | tr '/' '_')"
    ready="$out.ready"
    server_pid=""
    oha_pid=""

    cleanup() {
        if [[ -n "$oha_pid" ]]; then
            kill "$oha_pid" 2>/dev/null || true
        fi

        if [[ -n "$server_pid" ]]; then
            kill "$server_pid" 2>/dev/null || true
        fi
    }
    trap cleanup EXIT

    mkfifo "$ready"
    benchmarks/target/profiling/serve-{{ framework }} --workers 4 > "$ready" &
    server_pid=$!
    exec 3< "$ready"
    rm -f "$ready"
    read -r _ addr _ <&3

    oha --no-tui -c 64 -z 15s "http://$addr{{ path }}" > /dev/null &
    oha_pid=$!
    sleep 3

    if [[ "$(uname)" == Darwin ]]; then
        sample "$server_pid" 10 -file "$out.txt"
    else
        perf record -g -p "$server_pid" -o "$out.data" -- sleep 10
    fi

    wait "$oha_pid"

# Run the router example end to end.
example:
    cargo run -p fast-http-router --example users

# Run the example server on 127.0.0.1:3000 (HTTP/1.1 and h2c); Ctrl-C to stop.
serve:
    cargo run -p fast-http-server --example hello

clean:
    cargo clean
    cargo clean {{ benchmarks }}
