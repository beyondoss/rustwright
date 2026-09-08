# Memory audit — max & idle RSS

Status: living inventory (2026-09-08). Measurement protocol: `MEMORY_BENCH.md`,
`benchmarks/process_tree_memory.py`. Prior bound work: `#255`
(`perf: bound page memory and attribute it by PSS role`).

**Goal:** drive down **idle** library RSS (what stays after work quiesces) and
**max** library RSS (peak during navigation / actions / screenshots / snapshots).
Chromium still dominates `rss_tree_kb`; this audit targets `rss_self_kb` and
client PSS (python + rustwright, excluding the browser role).

**Rule:** every retained store must have an owner, a growth model, a cap, and a
release path. Temp peaks must not leave a high watermark without an idle drain.

---

## Measurement vocabulary

| Metric | Meaning |
| --- | --- |
| `rss_self_kb` | Peak RSS of the host process (Python + in-process Rust client) |
| `rss_tree_kb` | Peak sum of host + descendants (includes Chromium) |
| PSS by role | Linux proportional set size split (`python` / `driver` / `chromium`) |
| Idle RSS | Sample after GC/settle with a live page/browser but no in-flight op |
| Max RSS | Peak during the hottest phase of a workload |

Publish only Testbox / Docker suite numbers. Do not treat host ad-hoc samples as
canonical evidence (`AGENTS.md`, `BENCHMARK.md`).

---

## What #255 already fixed

| Bound | Cap | Release |
| --- | --- | --- |
| `CdpEventLog` entry count | 8192 | browser close → `release_memory_buffers` |
| `CdpEventLog` retained bytes | 8 MiB | same |
| Per-entry retained event | 64 KiB (else tombstone) | same |
| Retained strings / arrays in compacted events | 8 KiB / 128 items | compaction |
| Python navigation response bodies | 20 entries / 8 MiB page-owned | page close prune |
| Native console / network record rings | 1024 each | page close |
| Native network body fetch return | 20 MiB after CDP delivery | not retained in store |

Gaps left after #255 are the focus below: **uncapped page-owned payloads**,
**close-only release**, **temp peaks that stack copies**, and **per-poll script rebuilds**.

---

## Inventory — retained (idle)

### R1 — `NetworkRequestStore` full `post_data` / `post_data_entries` — HIGH

- **Symbols:** `NetworkRequestStore`, `request_from_event`, `route_from_event`
  (`src/lib.rs`)
- **Retains:** Per-request `serde_json::Value` including raw POST bodies and
  redirect chains
- **Growth:** Per `Network.requestWillBeSent*`; `applied_order` capped at 8192
  **count**, no **byte** budget; bodies not run through `compact_retained_cdp_value`
- **Idle:** Watermark stays until `PageInner::release_memory_buffers` (close only)
- **Also note:** Compacted CDP event-log allowlist strips `postData` only on the
  oversize path; small events can still retain full `request` objects in the log
- **Fix:** Truncate/strip `post_data*` at ingest to `CDP_RETAINED_STRING_MAX_BYTES`;
  always strip heavy fields before event-log retain; optional idle drain API

### R2 — `CdpEventLog` watermark — HIGH

- **Symbols:** `CdpEventLog`, `CDP_EVENT_LOG_*`, `compact_retained_cdp_event`
- **Retains:** Compacted events up to 8 MiB / 8192 entries for the **browser**
  lifetime
- **Growth:** Every CDP event; `push` clones the live event before compacting
- **Idle:** Stays at high-water until browser close
- **Fix:** Always strip network/console heavy payloads before retain; build
  retained copy without keeping a second full live clone longer than needed;
  add idle trim below caps for agent/MCP

### R3 — `ConsoleRecordStore` args — MED–HIGH

- **Symbols:** `ConsoleRecordStore`, `console_arg_value`, capacity 1024
- **Retains:** `text` + `args: Vec<Value>` (full CDP arg values)
- **Growth:** Ring 1024 while console capture armed
- **Idle:** Full ring persists
- **Fix:** Cap string args at ingest; drop oversized object args to preview

### R4 — `NativeNetworkRecordStore.request_body` — MED

- **Symbols:** `NativeNetworkEntry` / `RustwrightNetworkRecord.request_body`
- **Retains:** Full `postData` string per recorded request
- **Growth:** Ring 1024
- **Fix:** Truncate at ingest (same string budget as event log)

### R5 — `broadcast::channel(4096)` live CDP events — MED

- **Symbols:** `CdpClient::from_websocket_stream`
- **Retains:** Up to 4096 **full** (non-compacted) `Value`s for slow subscribers
- **Idle:** Empty when consumers keep up; cost is lagging subscribers (agent
  page-event pumps)
- **Fix:** Smaller capacity for single-page agent; don’t subscribe until needed

### R6 — Unbounded CDP write / serializer-release `mpsc` — LOW idle / HIGH if stalled

- **Symbols:** `write_tx`, `serializer_release_tx`
- **Retains:** Outgoing JSON strings / release commands while writer stalls
- **Fix:** Bound with backpressure (behavior change — elevate carefully)

### R7 — `PageFrameState` + serializer realms — MED (iframe-heavy)

- **Symbols:** `PageFrameState`, `CdpRuntimeState`
- **Retains:** Frame tree, execution contexts, serializers for live frames
- **Idle:** Correct for a live page; no global size cap
- **Fix:** Evict detached frames/realms aggressively; idle TTL on unused serializers

### R8 — Python page history / body caches — HIGH (Python API)

- **Symbols:** `_navigation_responses`, `_response_log`, `_fulfilled_route_bodies`,
  `Response._body_cache` (`python/rustwright/sync_api.py`)
- **Already bounded** for navigation bodies (20 / 8 MiB); other rings still idle-retain
- **Fix:** Idle-evict body caches while keeping Response objects; don’t keep HAR
  bodies until write

### R9 — Agent `BrowserState` — LOW–MED

- **Symbols:** `current_refs`, `PageRuntime` event/detail receivers, snapshot helper
- **Retains:** Last snapshot refs; per-tab receivers; helper install flag
- **Fix:** Drop unused subscriptions; clear refs on TTL; helper already reinstalled
  after navigation (`#6`)

### R10 — MCP shaping — LOW

- Ephemeral 9 KiB / 200-line budgets; no long-lived page caches beyond agent/core

---

## Inventory — peaks (max RSS)

### P1 — Screenshot / PDF base64 + decode stacking — HIGH

- **Symbols:** `page_screenshot_async`, PDF path, MCP `output_content`
- **Peak:** CDP `Value` (base64) + decoded `Vec<u8>` + path write **clone**
- **Fix:** Write without cloning (`spawn_blocking` returns the same `Vec`);
  drop base64 `Value` immediately after decode; avoid re-encode in MCP when possible

### P2 — Full `locator_script` assembly — HIGH

- **Symbols:** `locator_script_for_root` (~43 KiB template + shadow helper),
  `LOCATOR_FILL_TEMPLATE` (~20 KiB)
- **Peak:** Assembled `String` + serializer wrapper + CDP `Value` + write-queue `String`
- **Mitigated:** Actionability wait caches expression (`#6`); CSS fast path exists
- **Still hot:** `page_click_actionable_wait_async` / fill / assert rebuild per poll
- **Fix:** Pass `&str`, cache expression across polls; long-term install helper once

### P3 — `Network.getResponseBody` up to 20 MiB — HIGH when hit

- Cap applied after Chromium delivers; base64 + decoded can coexist briefly
- **Fix:** Decode/truncate eagerly; align navigation retain with Python 8 MiB budget

### P4 — Agent snapshot helper install (~17 KiB) — MED

- Mitigated by install-once (`#6`); re-sent after navigation
- **Fix:** Prefer init-script persistence across same-document navigations if safe

### P5 — File upload base64 (agent) — MED

- Up to 20 MiB/file × encode expansion
- **Fix:** Cap concurrent encoded budget; encode once into wire buffer

### P6 — Telemetry — NEGLIGIBLE

- One-shot PostHog JSON (`src/telemetry.rs`)

---

## Ranked action plan

| Priority | Tag | Item | Expected effect |
| --- | --- | --- | --- |
| 1 | EXPLOIT idle | Truncate/strip `post_data*` at `NetworkRequestStore` / native ingest | Cuts uncapped retained POST bodies |
| 2 | EXPLOIT idle | Always strip heavy payloads before `CdpEventLog` retain | Lowers idle watermark without waiting for 64 KiB tombstone |
| 3 | EXPLOIT idle | Cap console arg strings at ingest | Bounds 1024-slot console ring |
| 4 | EXPLOIT peak | Screenshot path write without `bytes.clone()` | Removes one full-frame copy from peak |
| 5 | EXPLOIT peak | Stop per-poll `body.clone()` / rebuild on click wait (and fill next) | Stops RSS ratchet during long actionability waits |
| 6 | ELEVATE idle | Idle trim API (`trim_retained_memory`) below close-only release | Drops watermark while page stays open |
| 7 | ELEVATE peak | Bound CDP write queue | Prevents stall-amplified peaks |
| 8 | ELEVATE | Installed locator helper (no per-call script assembly) | Structural peak cut |

---

## Implementation tracking

| Item | Status |
| --- | --- |
| This inventory | done |
| R1 post_data ingest bound | done (8 KiB, shared with event-log string budget) |
| R3/R4 console + native body bound | done |
| R2 heavy-payload strip before retain | done (`strip_retained_heavy_payloads`) |
| P1 screenshot no-clone write | done |
| P2 click-wait `&str` / no body clone | done (`evaluate_locator_for_page` takes `&str`) |
| Idle trim API | planned |
| Bounded write queue | planned |
| Testbox before/after RSS numbers | planned (Testbox only) |

When claiming a win, cite suite + lifecycle + `rss_self_kb` / client PSS distributions
from ignored `.benchmark-data/` artifacts — never a single unreproducible run.
