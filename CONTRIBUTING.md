# Contributing

Rustwright is an alpha Rust project: a CDP engine plus native **CLI** and **MCP**
frontends. Expect some rough edges and large core files.

## Local setup

Install a [Rust toolchain](https://rustup.rs/) (1.85+) and a Chrome/Chromium
binary. Point the engine at it if discovery fails:

```bash
export RUSTWRIGHT_CHROMIUM=/path/to/chrome   # or CHROME / CHROMIUM
```

## Checks

Core workspace (`rustwright-core`, `rust-native`, `agent`):

```bash
cargo check --locked
cargo test --locked
```

CLI and MCP are separate Cargo workspaces:

```bash
cargo test --manifest-path cli/Cargo.toml --locked
cargo test --manifest-path mcp/Cargo.toml --locked
```

Build and run the MCP server from a checkout:

```bash
cargo run --manifest-path mcp/Cargo.toml
```

Or the CLI:

```bash
cargo run --manifest-path cli/Cargo.toml -- open https://example.com
cargo run --manifest-path cli/Cargo.toml -- snapshot
cargo run --manifest-path cli/Cargo.toml -- close
```

Once Chromium is available, run ignored real-browser CLI cases with
`cargo test --manifest-path cli/Cargo.toml -- --ignored`.

## Docs

- [`mcp/README.md`](mcp/README.md) — MCP tools and client config
- [`cli/README.md`](cli/README.md) — CLI command surface
- [`docs/RELEASING.md`](docs/RELEASING.md) — GitHub Release binary packaging
