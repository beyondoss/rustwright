# Agent interfaces

Rustwright drives Chromium for agents through two native surfaces:

| Surface | Binary | Docs |
|---|---|---|
| MCP stdio server | `rustwright-mcp` | [`mcp/README.md`](../mcp/README.md) |
| Persistent CLI | `rustwright-cli` | [`cli/README.md`](../cli/README.md) |

Both sit on the same Rust CDP engine and use compact accessibility snapshots
with element refs (`e1`, `e2`, …).

## MCP

```bash
cargo install --path mcp
claude mcp add rustwright -- rustwright-mcp
```

See [`mcp/README.md`](../mcp/README.md) for tools (`browser_navigate`,
`browser_snapshot`, `browser_click`, …) and configuration.

## CLI

```bash
cargo install --path cli
# or: curl -fsSL https://raw.githubusercontent.com/beyondoss/rustwright/main/install.sh | sh
export RUSTWRIGHT_CHROMIUM=/path/to/chrome
rustwright-cli open https://example.com
rustwright-cli snapshot
rustwright-cli click @e1
rustwright-cli close
```

See [`cli/README.md`](../cli/README.md) for the full command surface.

## Snapshots and refs

Both surfaces work from a compact accessibility snapshot rather than raw HTML
or pixels:

```
- heading "Rustwright" [level=1] [ref=e1]
- textbox "Email" [ref=e2]
- button "Sign in" [ref=e3]
```

Refs are session-scoped and never reused. Resolving a stale ref fails instead of
silently targeting a different element. Ref resolution is best-effort for
cooperative pages, not a security boundary. Snapshots include page values but
mask password fields.
