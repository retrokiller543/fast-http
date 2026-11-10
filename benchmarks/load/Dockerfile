FROM rust:1-bookworm
RUN cargo install oha --locked && apt-get update && apt-get install -y --no-install-recommends util-linux && rm -rf /var/lib/apt/lists/*
WORKDIR /src
