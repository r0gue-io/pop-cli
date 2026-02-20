---
name: pop-cli-release
description: Use when releasing Pop CLI versions, including version bump, changelog prep, publishing crates, and release announcements.
---

# NewRelease Workflow

Complete checklist for releasing a new version of Pop CLI.

## Prerequisites

- Must have push access to `r0gue-io/pop-cli`
- Must be in the crates.io `pop-cli` team for publishing
- Clean working directory recommended

## Release Checklist

### Phase 1: Pre-Release Preparation

#### 1.1 Code Complete
- [ ] All planned features merged
- [ ] All PRs for this release approved and merged
- [ ] No blocking issues remaining

#### 1.2 Documentation
- [ ] Update docs to reflect new functionalities
- [ ] If PR still under review near deadline, have docs PR ready to merge
- [ ] Ensure new features have external messaging ready (Twitter draft)

#### 1.3 Cleanup Deprecated Code
- [ ] Remove deprecated code NOT introduced in this release
- [ ] Search for deprecation patterns:
  ```bash
  rg -i "deprecated|todo.*remove|fixme.*remove" --type rust
  ```
- [ ] Reference: https://github.com/r0gue-io/pop-cli/pull/331/commits/3cd8194b165693f06d9001c8b543a968c920d3ce

### Phase 2: Version Bump & Changelog

#### 2.1 Bump Versions
- [ ] Update `workspace.package.version` in root `Cargo.toml`
  ```toml
  [workspace.package]
  version = "X.Y.Z"
  ```
- [ ] Verify all crate versions align (they inherit from workspace)

#### 2.2 Update Changelog
- [ ] Run git-cliff to generate changelog:
  ```bash
  git cliff --bump -o CHANGELOG.md
  ```
- [ ] Review generated changelog for accuracy
- [ ] Add any missing notable changes manually

#### 2.3 Create Release PR
- [ ] Create PR with version bump + changelog
- [ ] Reference format: https://github.com/r0gue-io/pop-cli/pull/244
- [ ] Recent examples:
  - https://github.com/r0gue-io/pop-cli/pull/760
  - https://github.com/r0gue-io/pop-cli/pull/756

### Phase 3: GitHub Release

#### 3.1 Create Release
- [ ] Go to https://github.com/r0gue-io/pop-cli/releases
- [ ] Click "Draft a new release"
- [ ] Create new tag: `vX.Y.Z`
- [ ] Generate release notes automatically
- [ ] Add **Community Contributions** section manually
- [ ] Match format of previous releases

### Phase 4: Publish to crates.io

#### 4.1 Dry Run
```bash
cargo publish -p pop-common --dry-run
cargo publish -p pop-telemetry --dry-run
cargo publish -p pop-contracts --dry-run
cargo publish -p pop-chains --dry-run
cargo publish -p pop-cli --dry-run
```

#### 4.2 Publish (in order)
```bash
cargo publish -p pop-common
cargo publish -p pop-telemetry
cargo publish -p pop-contracts
cargo publish -p pop-chains
cargo publish -p pop-cli
```

**Note**: If permission denied, request access at https://github.com/orgs/r0gue-io/teams

### Phase 5: Announcements

#### 5.1 Website Update
- [ ] Create PR to `r0gue-io/pop-website`
- [ ] Add release highlights
- [ ] Reference: https://github.com/r0gue-io/pop-website/pull/65

#### 5.2 Social Announcements
- [ ] Post on Twitter/X
- [ ] Post on Telegram

## Version Format

Current version location: `Cargo.toml` → `[workspace.package]` → `version`

```toml
[workspace.package]
version = "0.12.1"  # Current
version = "0.13.0"  # Next minor
version = "1.0.0"   # Next major
```

## Quick Commands

```bash
# Check current version
grep -A5 "\[workspace.package\]" Cargo.toml | grep version

# Generate changelog
git cliff --bump -o CHANGELOG.md

# Dry run all publishes
for crate in pop-common pop-telemetry pop-contracts pop-chains pop-cli; do
  cargo publish -p $crate --dry-run
done

# Find deprecated code
rg -i "deprecated" --type rust
```

## Notes

- Always dry-run before actual publish
- Publish crates in dependency order (common → cli)
- Wait for each crate to be available on crates.io before publishing dependents
- Community contributions section in release notes is manual
