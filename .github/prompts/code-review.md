Rustwright is a Rust CDP engine for Chromium, exposed as a native **MCP** server
(`mcp/`) and **CLI** (`cli/`). It is alpha, Chromium-only, MIT-licensed.

The Rust core lives in `src/lib.rs`. Agent helpers live in `agent/`. The
ergonomic Rust facade is `rust-native/`. There is no language FFI / C ABI /
package-registry binding surface in this repository.

Review for correctness of CDP and process handling, MCP/CLI tool contracts,
resource cleanup, and stealth/fingerprint hygiene — not for Playwright API
parity across languages.
