---
name: version-upgrade
description: Prepare and validate a Rustwright version bump for GitHub Release asset distribution. Use when asked to bump or upgrade the Rustwright version, prepare a release PR, or tag a release.
---

# Version Upgrade

Bump the shared Rustwright version fields and prepare a release PR. This fork
distributes prebuilt binaries as GitHub Release assets. It does not publish to
language package registries. Do not publish `rustwright-core` to crates.io from
this skill.

## Arguments and mode

Read `$ARGUMENTS` for an exact SemVer version and an optional mode.

- Accept stable versions such as `0.1.1` and prereleases in the form
  `0.2.0-alpha.1`, `0.2.0-beta.1`, or `0.2.0-rc.1`.
- If the caller asks only to check, verify, or report a release status, enter
  verify-only mode. Perform read-only GitHub and Git queries and return the
  result without creating a branch, changing a file, merging, or tagging.
- If no version is supplied, use the helper's default: increment a stable
  patch version or the final numeric prerelease component.
- Treat `prepare`, `PR only`, or `dry run` as prepare-only mode. Stop after the
  release PR is open and local checks pass.
- Enter full-release mode only when the caller explicitly says `publish`,
  `release now`, or `full release` in the current request. That means tagging
  and attaching GitHub Release assets — not registry publication.
- Do not pause for choices that can be derived from the repository. Stop only
  for a dirty worktree, failed validation, a required human review, or another
  condition that makes releasing unsafe.

## 1. Inspect current state

1. Read `docs/RELEASING.md`; repository instructions and release workflows
   override examples in this skill.
2. Require a clean worktree. Never discard local changes.
3. Fetch `origin/main` and all tags. Start from the current `origin/main`, not a
   stale local branch.
4. Search open and merged release PRs and Git tags before deciding where to
   start. A new target must be strictly newer than every existing `v*` release
   tag.
5. Classify the target as new, on an open release PR, prepared on main, tagged,
   or already released. Reuse an existing PR only after inspecting its complete
   commit history and diff. Reject unexpected files or content.

## 2. Prepare the version bump

For a new target only, create `release/v<version>` from `origin/main`. Use the
repository helper to update the source manifests:

```bash
python3 .claude/skills/version-upgrade/scripts/bump_version.py <version>
```

Omit `<version>` only when the caller did not specify one. Then regenerate,
rather than manually edit, the Cargo lockfiles:

```bash
cargo metadata --format-version 1 > /dev/null
cargo metadata --manifest-path cli/Cargo.toml --format-version 1 > /dev/null
cargo metadata --manifest-path mcp/Cargo.toml --format-version 1 > /dev/null
python3 .claude/skills/version-upgrade/scripts/bump_version.py --check <version>
```

The check must confirm one exact version in all of these locations:

- `Cargo.toml` (`rustwright-core`)
- `rust-native/Cargo.toml` (`rustwright`)
- the `rustwright-core` entry in `Cargo.lock`

Run the release checks from `docs/RELEASING.md`:

```bash
cargo check --locked
cargo test --lib --locked
```

Do not weaken, skip, or silently narrow a failing check.

## 3. Push the release PR

1. Review the complete diff and staged diff for accidental disclosures.
2. Commit only the expected version files. Use the authenticated GitHub
   account's public `users.noreply.github.com` identity.
3. Install the tracked pre-push hook with `python3 tools/install_hooks.py` when
   that tool exists.
4. Push the release branch without bypassing hooks and open a PR titled
   `chore(release): bump version to <version>`.

## 4. Merge, tag, and attach assets

Skip this section in prepare-only mode.

1. Wait for required PR checks. Merge using the repository's normal merge
   policy; do not bypass required reviews or protections.
2. Refresh `origin/main`, verify that commit contains the exact target version,
   then tag `v<version>` and push the tag.
3. Build and attach GitHub Release assets (`rustwright-cli-*`, and
   `rustwright-mcp` when shipping the MCP server). Prefer
   `tools/build_mcp_pgo.sh` for native-host MCP binaries.
4. Confirm `install.sh` can download the new CLI asset on a clean machine.
