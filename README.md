**A Rust rewrite of Playwright's browser engine**, speaking raw
[Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/)
in-process — **[~27× less MCP server memory than Playwright MCP](#benchmarks)**
(about **5 MB vs 135 MB** process RSS), with no Playwright automation fingerprint.
Alpha; Chromium-only.

This repository ships **native binaries** (CLI + MCP) as GitHub Release assets.
It does **not** publish to PyPI, npm, or other language registries.

[![status: alpha](https://img.shields.io/badge/status-alpha-orange)](#project-status)
[![tests](https://img.shields.io/github/actions/workflow/status/beyondoss/rustwright/test.yml?label=tests)](https://github.com/beyondoss/rustwright/actions/workflows/test.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Chromium only](https://img.shields.io/badge/browser-Chromium-4285F4?logo=googlechrome&logoColor=white)](#limitations)

---

## What is Rustwright?

Rustwright drives Chromium from a **native Rust CDP engine** — no Playwright Node driver subprocess in the path.

```text
playwright MCP:  agent ──stdio──► Node MCP ──► Playwright driver ──CDP──► Chromium
rustwright:      agent ──stdio──► rustwright-mcp ── raw CDP ─────────────► Chromium
```

Primary entry points in this repository:

| Surface | What it is |
|---|---|
| [`rustwright-mcp`](mcp/) | Native MCP stdio server (`browser_*` tools) |
| [`rustwright-cli`](cli/) | Agent-focused shell CLI with compact snapshots and `@eN` refs |

## Quickstart

### CLI

```bash
curl -fsSL https://raw.githubusercontent.com/beyondoss/rustwright/main/install.sh | sh
```

Point at an existing Chrome/Chromium if needed, then drive a session:

```bash
export RUSTWRIGHT_CHROMIUM=/path/to/chrome   # or CHROME / CHROMIUM
rustwright-cli open https://example.com
rustwright-cli snapshot          # compact page tree with @eN refs
rustwright-cli click @e1
rustwright-cli close
```

See [`cli/README.md`](cli/README.md) for the full command surface. From a checkout you can also `cargo install --path cli`.

### MCP server

```bash
cargo install --git https://github.com/beyondoss/rustwright rustwright-mcp
# or attach a prebuilt rustwright-mcp binary from a GitHub Release
```

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

| If... | Do this |
|---|---|
| You already have Chrome/Chromium | Set `RUSTWRIGHT_CHROMIUM` (or `CHROME` / `CHROMIUM`) to the executable. |
| Screenshots are too large to inline | Tune `RUSTWRIGHT_MCP_SCREENSHOT_MAX_BYTES`; oversized captures fall back to a temp-file path. |

See [`mcp/README.md`](mcp/README.md) for tools and configuration.

### Browser

Rustwright launches Chromium itself. Use a system Chrome/Chromium, or set
`RUSTWRIGHT_CHROMIUM`, `CHROME`, or `CHROMIUM` to an executable path.

## Why Rustwright?

- **No Node driver subprocess.** Playwright MCP sits on Playwright's Node driver. Rustwright's engine is native Rust.
- **Raw CDP, in Rust.** A from-scratch async CDP client — not a wrapper around another automation library.
- **No Playwright automation fingerprint.** The driver never loads, so its signatures never appear. See [Automation detection](#automation-detection).
- **Trusted input.** Clicks and typing use real CDP input events (`Input.dispatchMouseEvent`), not synthetic `element.click()` DOM calls.
- **Cross-origin iframes (OOPIF).** Auto-attaches out-of-process iframe targets with flattened CDP sessions.
- **One engine, agent surfaces.** The same Rust core backs the MCP server and CLI.

## How it works

One Rust core — an async CDP client built on Tokio (WebSocket, with opt-in Unix-pipe transport) — talks to Chromium directly. The MCP server and CLI sit on that core; nothing in the serving path requires Node or a package-registry install.

## Browser automation for AI agents

Give an agent or shell script a browser through compact accessibility snapshots with element refs (`e1`, `e2`, …), instead of raw HTML or screenshots. Refs are session-scoped, never reused, and best-effort rather than a security boundary; snapshots include page values but mask password fields.

The MCP and CLI sections above are the supported agent paths. Setting up via an AI agent? Tell it to fetch
`https://raw.githubusercontent.com/beyondoss/rustwright/HEAD/mcp/README.md`
and follow it.

## Remote Chromium

To drive an already-running Chromium over CDP, use
`chromium().connect_over_cdp(...)` from the Rust library API (or point
compatible tooling at the endpoint). See the
[remote-browser guide](docs/REMOTE_BROWSERS.md) for endpoint shape, diagnostics,
and security notes.

## Automation detection

Because Rustwright never loads Playwright's Node driver, it never emits the automation signatures that ship with it:

- **No Playwright driver signatures** — no `__playwright__binding__` / utility-world globals, no driver bootstrap. The backend reports `playwright_driver: "none"`.
- **No `Runtime.enable` on the default path** — a normal launch + navigate never enables the CDP Runtime domain, closing the `Runtime.enable` console-serialization leak behind `isAutomatedWithCDP`. (Console/page-error/binding opt-ins still enable it lazily — detectable by design.)
- **Headless identity normalized by default** — launches with `--disable-blink-features=AutomationControlled`, rewrites `HeadlessChrome/` → `Chrome/` in the UA and client hints, and installs a `navigator.webdriver` cleanup init script.

Local fingerprint runs — default Playwright failed webdriver/headless checks that Rustwright passed; these are local diagnostics, not a guarantee:

| Probe | Result |
|---|---|
| SannySoft | ✅ Clean |
| BrowserScan | ✅ Clean |
| DeviceAndBrowserInfo | ✅ Clean (after the Runtime-domain cleanup) |
| CreepJS | ⚠️ Detects headless |

> [!IMPORTANT]
> **Rustwright is not "undetectable."** It is not a CAPTCHA or Cloudflare bypass, and it is not fully CDP-invisible — it still uses CDP primitives (`Target.setAutoAttach`, init scripts, and lazy `Runtime.enable` for console event/pageerror event/binding opt-ins). The claim is narrow: **no Playwright-specific automation fingerprint**, plus baseline signal hygiene.

## Benchmarks

On the same host-safe MCP protocol workload (`initialize` + `tools/list`, no
Chromium launched), **rustwright-mcp** used about **27× less** process memory
than **@playwright/mcp**:

| MCP server | Median process VmRSS |
|---|---:|
| `rustwright-mcp` | **5.0 MiB** |
| `@playwright/mcp` 0.0.80 | **135.2 MiB** |

That is the MCP server process only (not the browser). Chromium still dominates
end-to-end RSS when a page is open; the win is the automation side-car you keep
alive next to the agent.

Reproduce: `tools/compare_mcp_rss.py`. Notes: [`MEMORY_BENCH.md`](MEMORY_BENCH.md).

## Alternatives

| | Rustwright | @playwright/mcp | Puppeteer |
|---|---|---|---|
| **Surfaces** | Native MCP + CLI | Node MCP on Playwright | JS/TS library |
| **Engine / transport** | Rust core, raw CDP | Playwright Node driver | Node over CDP |
| **In-process engine (no driver subprocess)** | ✅ | ❌ bundled Node driver | ✅ Node is the runtime |
| **Browsers** | Chromium only | Chromium, Firefox, WebKit | Chrome, Firefox |
| **Default input** | Trusted CDP events | Playwright defaults | Browser / CDP |
| **Playwright fingerprint** | No | Yes | n/a |
| **Maturity** | 🟠 Alpha | 🟢 Mature | 🟢 Mature |

Rustwright's lane: **a Rust CDP engine for Chromium**, exposed through MCP and CLI for agents.

## Limitations

See [`LIMITATIONS.md`](LIMITATIONS.md) for detail.

- **Alpha** — MCP/CLI surfaces work; expect rough edges.
- **Chromium only** — Firefox and WebKit are out of scope.
- **OOPIF** — residual gaps in non-main-frame follow-ups and drag/screenshot/bounding-box.
- **Automation detection is partial** — 3 of 4 public fingerprint targets clean in local runs (CreepJS still detects headless). **No undetectability promise.**
- **No registry packages** — distribution is GitHub Release binaries / source builds.

## Contributing

Rustwright is a Rust workspace: `cargo` builds the engine, CLI, and MCP server.
See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`docs/RELEASING.md`](docs/RELEASING.md)
for local checks and GitHub Release packaging.

## Project status

Rustwright is an early alpha, developed in the open. If the architecture
resonates, [give it a ⭐](https://github.com/beyondoss/rustwright).

## License

[MIT](LICENSE)

<div align="center">
<sub>Built with 🦀 and a lot of CDP frames · <a href="https://github.com/beyondoss/rustwright">beyondoss/rustwright</a></sub>
</div>
