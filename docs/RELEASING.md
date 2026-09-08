# Releasing Rustwright

This guide is for the release owner. This fork distributes prebuilt binaries as
**GitHub Release assets** (for example `rustwright-cli-<target>` consumed by
`install.sh`). It does not publish to PyPI, npm, NuGet, RubyGems, or Maven
Central.

## Agent-assisted version bump

Use `/version-upgrade <version> prepare` to bump shared version fields and open
a release PR. The skill is defined in
`.claude/skills/version-upgrade/SKILL.md`. Publishing means tagging and
attaching binaries to the GitHub Release — not registry Trusted Publishing.

## Prepare a release

- [ ] Choose one version in SemVer form, for example `0.2.0`.
- [ ] Set that exact string in every source-of-truth field:
  - `pyproject.toml` → `[project].version`
  - `Cargo.toml` → `[package].version` for `rustwright-core`
  - `capi/Cargo.toml` → `[package].version` for `rustwright-capi`
  - `rust-native/Cargo.toml` → `[package].version` for `rustwright`
  - `csharp/Rustwright/Rustwright.csproj` → `<Version>` (if kept in tree)
  - `ruby/lib/rustwright.rb` → `VERSION` (if kept in tree)
  - `java/build.gradle.kts` → top-level `version =` and `coordinates(...)` (if kept)
- [ ] Set the same string in the shipped runtime metadata:
  - `python/rustwright/sync_api.py` → Rustwright creator `version` in `_write_har`
  - `python/rustwright/sync_api.py` → `playwrightVersion` in `_write_trace_zip`
  - `python/rustwright/cli.py` → source-checkout fallback in `_version`
  - `python/rustwright/_backend.py` → source-checkout fallback in `_version` (before `+local`)
  - `python/rustwright/_agent/cli.py` → source-checkout fallback
- [ ] Add a `## [<version>] - <date>` section to `CHANGELOG.md` by renaming the
      current `## [Unreleased]` section and opening a fresh empty one above it,
      add the matching link reference at the bottom, and add the version to
      `REQUIRED_VERSIONS` in `tests/test_changelog.py`.
- [ ] Regenerate the lockfiles; do not edit generated entries by hand. There are
      three Cargo lockfiles, because `cli/` and `mcp/` are separate workspaces
      that depend on `rustwright-core` by path — the root `cargo` command does
      not touch them:

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
  git grep -n "$PREVIOUS_VERSION" -- . ':!CHANGELOG.md' ':!docs/'
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
- [ ] Build release binaries for the supported targets (at least the
      `rustwright-cli-<target>` names expected by `install.sh`).
- [ ] Create a GitHub Release for the tag and upload those assets.
- [ ] Confirm `install.sh` (or an equivalent curl install path) downloads the
      new CLI asset successfully on a clean machine.

`rustwright-mcp` may be attached the same way when shipping the MCP server.
`cli/` and `mcp/` version independently from the shared library version when
needed.
