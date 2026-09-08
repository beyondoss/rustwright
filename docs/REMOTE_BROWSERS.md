# Remote browsers

Rustwright supports remote Chromium through raw Chrome DevTools Protocol (CDP).
Use `chromium().connect_over_cdp(...)` from the Rust library API with an HTTP
discovery URL or a direct CDP WebSocket URL. Rustwright does not implement
Playwright's internal wire protocol.

## API contract

```rust
use rustwright::{chromium, ConnectOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = chromium().connect_over_cdp(ConnectOptions::new("http://browser:9222"))?;
    let page = browser.new_page()?;
    page.goto("https://example.com", None)?;
    browser.close()?;
    Ok(())
}
```

Playwright-style `BrowserType.connect()` is intentionally unsupported. Remote
services must expose raw Chromium CDP. Endpoints from `playwright run-server`
and `BrowserType.launchServer()` use the Playwright wire protocol and are
unsupported. Direct WebSocket protocol detection is heuristic — prefer an HTTP
discovery URL when you need a definitive protocol diagnosis.

For HTTP endpoints, Rustwright requests `/json/version` first. It uses a valid
`webSocketDebuggerUrl` from that response. Otherwise, it requests `/json` only
to detect Playwright's `wsEndpointPath`. Both requests share one connection
deadline and receive the caller's headers.

## MCP / CLI

The MCP server and CLI launch a local Chromium by default. Point
`RUSTWRIGHT_CHROMIUM` at a local executable when needed. Driving a remote CDP
endpoint from MCP/CLI is not the primary path documented here; use the Rust
library API above when you must attach to an already-running Chromium.

## Security

Treat a CDP endpoint as full control of the browser profile behind it. Do not
expose CDP ports to untrusted networks. Prefer authenticated tunnels or private
networks.
