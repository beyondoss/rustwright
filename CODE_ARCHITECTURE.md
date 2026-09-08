# Rustwright Code Architecture

Last updated: 2026-09-08

## Design goals

- Keep a single Rust CDP engine responsible for Chromium process management and
  protocol work.
- Expose that engine through agent surfaces only: **MCP** and **CLI**.
- Keep agent frontends thin — snapshot/ref tooling and tool/command wiring, not
  a second copy of engine semantics.

## Layout

| Path | Role |
|---|---|
| `src/lib.rs` | `rustwright-core`: Chromium launch/connect, CDP client/session, browser/page primitives, input/network/screenshot helpers. |
| `rust-native/` | Ergonomic Rust facade (`rustwright`) over the core; consumed by agent crates. |
| `agent/` | Shared agent helpers (snapshots, refs, actions) used by CLI and MCP. |
| `cli/` | Persistent browser automation CLI (`rustwright-cli`). |
| `mcp/` | Native MCP stdio server (`rustwright-mcp`) with `browser_*` tools. |

## Serving path

```text
MCP client / shell
        │
        ▼
rustwright-mcp  or  rustwright-cli
        │
        ▼
rustwright-agent  →  rustwright (rust-native)  →  rustwright-core  ──CDP──► Chromium
```

No language FFI, C ABI, or package-registry binding is on this path.
