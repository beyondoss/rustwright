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
- [ ] Build release binaries for the supported targets (at least the
      `rustwright-cli-<target>` names expected by `install.sh`).
- [ ] Create a GitHub Release for the tag and upload those assets. Put release
      notes on the GitHub Release itself (there is no in-repo changelog).
- [ ] Confirm `install.sh` (or an equivalent curl install path) downloads the
      new CLI asset successfully on a clean machine.

`rustwright-mcp` may be attached the same way when shipping the MCP server.
`cli/` and `mcp/` version independently from the shared library version when
needed.

For native-host `rustwright-mcp` release assets, prefer
`tools/build_mcp_pgo.sh` (fat LTO + profile-guided optimization; host-safe
training, no browser). See `MEMORY_BENCH.md` for the measured size / process
RSS deltas. Cross-compiled targets without a matching native train stay on a
plain release build.
