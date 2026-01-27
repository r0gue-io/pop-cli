# Repository Guidelines

## Project Structure & Module Organization
- `crates/` houses workspace members such as `pop-cli`, `pop-chains`, `pop-contracts`, `pop-common`, and `pop-telemetry`.
- `crates/pop-cli/src/` is the main CLI implementation; `crates/*/tests/` hold crate-level tests.
- `tests/` contains repo-level fixtures and integration assets (`networks/`, `runtimes/`, `snapshots/`).
- `pallets/` and `debian/` contain chain-specific components and packaging assets.
- Root files like `Cargo.toml`, `rust-toolchain.toml`, `flake.nix`, and `deny.toml` define workspace, toolchain, Nix, and security policy.

## Build, Test, and Development Commands
- `cargo build` builds the workspace with default features.
- `cargo build --no-default-features --features chain` builds chain-focused functionality only.
- `cargo build --no-default-features --features contract` builds contract-focused functionality only.
- `cargo nextest run --lib --bins` runs unit tests (recommended).
- `cargo nextest run --no-default-features --features contract --test contract` runs contract integration tests.
- `cargo nextest run --no-default-features --features chain --test chain` runs chain integration tests.
- `cargo nextest run` runs all tests.
- `cargo deny check` runs advisory and license checks.

## Coding Style & Naming Conventions
- Rust formatting is enforced by `.rustfmt.toml` (hard tabs, max width 100, edition 2024).
- Prefer `cargo fmt` before submitting changes; keep imports grouped by crate.
- Use clear, CLI-oriented names for commands and flags; match existing module naming in `crates/`.

## Testing Guidelines
- Primary framework is Rust’s built-in test harness plus `cargo nextest` for orchestration.
- Name integration tests by feature (`contract`, `chain`, `metadata`) to match existing patterns.
- Keep tests deterministic; avoid network dependencies unless explicitly mocked or documented.

## Commit & Pull Request Guidelines
- Commit messages follow Conventional Commits: `feat: ...`, `fix: ...`, `refactor: ...`, `ci: ...`.
- Keep subjects short and imperative; include PR numbers when available (e.g., `feat: add X (#123)`).
- PRs should include a brief summary, test results, and linked issues; add screenshots or CLI output snippets when UX changes.

## Security & Configuration Tips
- Use `GITHUB_TOKEN` when running tests that hit GitHub APIs to avoid rate limits.
- Respect `deny.toml` policy and keep dependencies aligned with the workspace versions.
