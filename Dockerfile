# syntax=docker/dockerfile:1.6
ARG RUSTWRIGHT_DOCKER_BASE_IMAGE=rust:1.88-bookworm
FROM ${RUSTWRIGHT_DOCKER_BASE_IMAGE}

USER root

ENV CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    PATH=/usr/local/cargo/bin:$PATH \
    RUSTWRIGHT_CHROMIUM=/usr/bin/chromium

WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        chromium \
        curl \
        git \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY rust-native ./rust-native
COPY agent ./agent
COPY cli ./cli
COPY mcp ./mcp

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target \
    cargo build --release --locked \
    && cargo build --manifest-path cli/Cargo.toml --release --locked \
    && cargo build --manifest-path mcp/Cargo.toml --release --locked \
    && cp cli/target/release/rustwright-cli /usr/local/bin/rustwright-cli \
    && cp mcp/target/release/rustwright-mcp /usr/local/bin/rustwright-mcp

CMD ["rustwright-mcp"]
