# Releasing Rustwright

This guide is for the release owner. This fork distributes prebuilt binaries as
**GitHub Release assets** (for example `rustwright-cli-<target>` consumed by
`install.sh`, and `rustwright-mcp` when shipping the MCP server). It does not
publish to language package registries.

## Agent-assisted version bump

Use `/version-upgrade <version> prepare` to bump shared version fields and open
a release PR. The skill is defined in
`.claude/skills/version-upgrade/SKILL.md`. Publishing means tagging and
attaching binaries to the GitHub Release — not registry Trusted Publishing.

## Prepare a release

- [ ] Choose one version in SemVer form, for example `0.2.0`.
- [ ] Set that exact string in every source-of-truth field:
  - `Cargo.toml` → `[package].version` for `rustwright-core`
  - `rust-native/Cargo.toml` → `[package].version` for `rustwright`
- [ ] Regenerate the lockfiles; do not edit generated entries by hand. There are
      three Cargo lockfiles, because `cli/` and `mcp/` are separate workspaces
      that depend on the core by path — the root `cargo` command does not touch
      them:

  ```bash
  cargo metadata --format-version 1 > /dev/null
  cargo metadata --manifest-path cli/Cargo.toml --format-version 1 > /dev/null
  cargo metadata --manifest-path mcp/Cargo.toml --format-version 1 > /dev/null
  ```

- [ ] Confirm every `rustwright*` entry across `Cargo.lock`, `cli/Cargo.lock`,
      and `mcp/Cargo.lock` holds the release version, except `rustwright-cli`
      and `rustwright-mcp`, which version independently.
- [ ] Confirm nothing was missed:

  ```bash
  git grep -n "$PREVIOUS_VERSION" -- . ':!docs/'
  ```

- [ ] Run local release checks:

  ```bash
  cargo check --locked
  cargo test --lib --locked
  cargo metadata --manifest-path cli/Cargo.toml --locked --format-version 1 > /dev/null
  cargo metadata --manifest-path mcp/Cargo.toml --locked --format-version 1 > /dev/null
  ```

## Tag and attach GitHub Release assets

- [ ] Merge the release PR.
- [ ] Tag the merge commit `v${VERSION}` and push the tag.
- [ ] Create a GitHub Release for that tag (release notes live on the Release
      itself; there is no in-repo changelog). Publishing the Release triggers
      `.github/workflows/release.yml`, which builds and uploads:
      - `rustwright-cli-<target>` for the four triples `install.sh` understands
        (`x86_64` / `aarch64` × `unknown-linux-musl` / `apple-darwin`)
      - matching `rustwright-mcp-<target>` assets
- [ ] Confirm the Release assets appear, then that `install.sh` (or an
      equivalent curl install path) downloads the new CLI asset on a clean
      machine.

To rebuild or backfill assets on an existing Release (for example after fixing
the workflow), run **Actions → Release binaries → Run workflow** and pass the
tag (`v0.1.0`). Uploads use `--clobber`, so re-runs replace prior assets.

`cli/` and `mcp/` version independently from the shared library version when
needed.

Linux assets are **static musl** binaries. Plain musl builds (CLI, and any
non-PGO musl MCP) use `cargo zigbuild`. Native-runnable MCP builds go through
`tools/build_mcp_pgo.sh`: Darwin arm64 uses the Apple toolchain; Linux musl
PGO uses `musl-gcc` from `musl-tools` (Zig cannot link LLVM profile runtime).
Cross MCP without a runnable train stays on the plain release profile — today
that is only `x86_64-apple-darwin` when built on arm64 macOS. See
`MEMORY_BENCH.md` for measured PGO deltas.
