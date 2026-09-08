# Rustwright quickstart

Rustwright is an alpha, Chromium-only CDP engine exposed as **native CLI** and
**MCP** binaries. Review [`LIMITATIONS.md`](LIMITATIONS.md) before production use.

## Agent / MCP setup

Paste this into Claude Code or another agent:

> Set up Rustwright MCP from https://github.com/beyondoss/rustwright. If the repository already exists, use the current checkout; otherwise clone it. Read `mcp/README.md` and `LIMITATIONS.md` first. Build `rustwright-mcp` with Cargo, point `RUSTWRIGHT_CHROMIUM` at Chrome/Chromium if needed, register the server with the MCP client, and verify `browser_navigate` to `https://example.com` returns a snapshot. Do not modify or commit source files.

### Install from source

```bash
git clone https://github.com/beyondoss/rustwright
cd rustwright
cargo install --path mcp
# or: cargo run --manifest-path mcp/Cargo.toml
```

Register with Claude Code:

```bash
claude mcp add rustwright -- rustwright-mcp
```

Or any MCP client:

```json
{
  "mcpServers": {
    "rustwright": {
      "command": "rustwright-mcp",
      "env": {
        "RUSTWRIGHT_CHROMIUM": "/path/to/chrome-or-chromium"
      }
    }
  }
}
```

See [`mcp/README.md`](mcp/README.md) for tools and configuration.

## CLI

```bash
curl -fsSL https://raw.githubusercontent.com/beyondoss/rustwright/main/install.sh | sh
# or from a checkout:
cargo install --path cli
```

```bash
export RUSTWRIGHT_CHROMIUM=/path/to/chrome
rustwright-cli open https://example.com
rustwright-cli snapshot
rustwright-cli close
```

See [`cli/README.md`](cli/README.md).

## Browser

Rustwright launches Chromium itself. Set `RUSTWRIGHT_CHROMIUM`, `CHROME`, or
`CHROMIUM` to an executable when auto-discovery is not enough.
