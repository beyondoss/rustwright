**A Rust rewrite of Playwright's browser engine**, speaking raw
[Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/)
in-process — **[~4× smaller client memory than playwright-python](#benchmarks)**
(no Node driver), with no Playwright automation fingerprint. Alpha; Chromium-only.

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
playwright-python:  your code ──pipe──► Node driver ──CDP──► Chromium
rustwright:         your code / agent ── raw CDP ──────────► Chromium
```

Primary entry points in this repository:

| Surface | What it is |
|---|---|
| [`rustwright-cli`](cli/) | Agent-focused shell CLI with compact snapshots and `@eN` refs |
| [`rustwright-mcp`](mcp/) | Native MCP stdio server (`browser_*` tools) |
| [Language bindings (alpha)](bindings/CONTRACT.md) | Go, Java, C#/.NET, Ruby, PHP, and native Rust over a shared C ABI |

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

- **No Node driver subprocess.** Playwright's Python binding launches and pipes to a bundled Node driver. Rustwright's engine is native.
- **Raw CDP, in Rust.** A from-scratch async CDP client — not a wrapper around another automation library.
- **No Playwright automation fingerprint.** The driver never loads, so its signatures never appear. See [Automation detection](#automation-detection).
- **Trusted input.** Clicks and typing use real CDP input events (`Input.dispatchMouseEvent`), not synthetic `element.click()` DOM calls.
- **Cross-origin iframes (OOPIF).** Auto-attaches out-of-process iframe targets with flattened CDP sessions.
- **One engine, many surfaces.** The same Rust core backs the CLI, MCP server, and alpha language bindings.

## How it works

One Rust core — an async CDP client built on Tokio (WebSocket, with opt-in Unix-pipe transport) — talks to Chromium directly. Thin bindings and agent frontends sit on that core; nothing in the serving path requires Node or a package-registry install.

## Browser automation for AI agents

Give an agent or shell script a browser through compact accessibility snapshots with element refs (`e1`, `e2`, …), instead of raw HTML or screenshots. Refs are session-scoped, never reused, and best-effort rather than a security boundary; snapshots include page values but mask password fields.

The MCP and CLI sections above are the supported agent paths. Setting up via an AI agent? Tell it to fetch
`https://raw.githubusercontent.com/beyondoss/rustwright/HEAD/mcp/README.md`
and follow it.

## Remote Chromium

To drive an already-running Chromium over CDP, use
`chromium.connect_over_cdp()` from a language binding (or point compatible
tooling at the endpoint). See the [remote-browser guide](docs/REMOTE_BROWSERS.md)
for endpoint shape, diagnostics, and security notes.
`BrowserType.connect()` is not a CDP alias.

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

Rustwright's **client** footprint is about **4× smaller** than
playwright-python's: median peak PSS **28 MB vs 110 MB (−75%)** on a remote-CDP
form-fill suite (Chromium off-box; smaller in every case). playwright-python
pays for a bundled Node driver; Rustwright speaks CDP in-process, so that cost
is gone. With a local Chromium, whole-process memory is still browser-dominated
and roughly comparable.

Details: [`benchmarks/form_fill/RESULTS.md`](benchmarks/form_fill/RESULTS.md).
Methodology: [`BENCHMARK.md`](BENCHMARK.md).

## Alternatives

| | Rustwright | playwright-python | Puppeteer | Patchright |
|---|---|---|---|---|
| **Surfaces** | Native CLI, MCP, C ABI bindings | Official Python Playwright | JS/TS Puppeteer | Playwright drop-in fork |
| **Engine / transport** | Rust core, raw CDP | Python → Node driver | Node over CDP | Patched PW driver |
| **In-process engine (no driver subprocess)** | ✅ | ❌ bundled Node driver | ✅ Node is the runtime | ❌ Playwright-style driver |
| **Browsers** | Chromium only | Chromium, Firefox, WebKit | Chrome, Firefox | Chromium-based |
| **Default input** | Trusted CDP events | Browser-level | Browser / CDP | Playwright + stealth |
| **Cross-origin iframes** | OOPIF (alpha) | Mature | Frame APIs | Inherits Playwright |
| **Playwright fingerprint** | No | Yes | n/a | Patched |
| **Maturity** | 🟠 Alpha | 🟢 Mature | 🟢 Mature | 🟡 Focused fork |

Rustwright's lane: **a Rust CDP engine for Chromium**, exposed through agent CLI/MCP and alpha language bindings.

## Limitations

See [`LIMITATIONS.md`](LIMITATIONS.md) for detail.

- **Alpha** — API shape covered; full **behavioral** parity not yet proven.
- **Chromium only** — Firefox and WebKit error explicitly.
- **OOPIF** — residual gaps in non-main-frame `JSHandle` follow-ups and drag/screenshot/bounding-box.
- **Automation detection is partial** — 3 of 4 public fingerprint targets clean in local runs (CreepJS still detects headless). **No undetectability promise.**
- **No registry packages here** — distribution is GitHub Release binaries / source builds, not PyPI or npm.

## Roadmap

- [ ] **Kotlin binding** — idiomatic Kotlin wrapper (Kotlin/JVM can already consume the Java FFM binding)
- [ ] Grow the language bindings beyond the alpha subset (contexts, routing, locators)
- [x] **Rustwright MCP server** — expose browser automation as tools for MCP-compatible AI agents ([mcp/](mcp/))
- [ ] CI / Testbox-backed benchmark evidence
- [ ] Close remaining OOPIF gaps

Recently shipped:

- [x] **Language bindings (alpha)** — Go, Java, C#/.NET, Ruby, and PHP over a shared C ABI, plus a native Rust API ([`bindings/CONTRACT.md`](bindings/CONTRACT.md)); cross-language equivalence gated in CI
- [x] Native async engine over the Tokio CDP core
- [x] OOPIF auto-attach with flattened CDP sessions
- [x] `Runtime.enable` console-serialization leak closed on the default path

Firefox and WebKit are **not planned** — Rustwright is deliberately Chromium-only.

## Contributing

Rustwright is a Rust workspace: `cargo` builds the engine, CLI, MCP server, and C ABI. Language-binding smoke and engine tests run in CI (`test.yml`, `bindings.yml`). See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`docs/RELEASING.md`](docs/RELEASING.md) for local checks and GitHub Release packaging.

## Project status

Rustwright is an early alpha, developed in the open. If the architecture
resonates, [give it a ⭐](https://github.com/beyondoss/rustwright).

## Telemetry

Rustwright sends one `engine_launched` event per process to PostHog with the Rustwright version, operating system, and CPU architecture. It sends no URLs or page content.

Events carry a random installation ID, generated without using the hostname, username, or MAC address, stored at `~/.cache/rustwright/telemetry_id` (or the `$XDG_CACHE_HOME`/`%LOCALAPPDATA%` equivalent) so events from the same installation can be counted together. Delete the file to reset it. Events are marked personless, and GeoIP enrichment is disabled at the event level. The request, like any HTTPS request, exposes the sender's IP address to PostHog, but Rustwright does not store the IP address in the event payload.

To opt out:

```bash
export DISABLE_TELEMETRY=1   # or DO_NOT_TRACK=1
```

## License

[MIT](LICENSE)

<div align="center">
<sub>Built with 🦀 and a lot of CDP frames · <a href="https://github.com/beyondoss/rustwright">beyondoss/rustwright</a></sub>
</div>
