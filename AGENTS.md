# Repository Guidelines

## Project Structure & Module Organization
This workspace is orchestrated by `Cargo.toml` at the root. User-facing CLI code lives in `crates/pop-cli`, with shared utilities in `crates/pop-common`, parachain helpers in `crates/pop-chains`, contract tooling in `crates/pop-contracts`, and telemetry plumbing in `crates/pop-telemetry`. Substrate runtime pallets sit under `pallets/`. Scenario data and golden files for integration testing live in `tests/networks`, `tests/runtimes`, and `tests/snapshots`. Container build recipes are in `Dockerfile` and `Dockerfile.ci`.

## Build, Test, and Development Commands
- `cargo build --release` produces an optimized binary with all default features.
- `cargo build --no-default-features --features chain` isolates parachain functionality; swap `chain` for `contract` as needed.
- `cargo run -- --help` is the quickest way to verify CLI wiring during development.

## Coding Style & Naming Conventions
Formatting is enforced by `.rustfmt.toml`; use tabs (no spaces) with a 100 column limit via `cargo +nightly fmt --all`. Keep modules and files snake_case, Rust types PascalCase, and constants SCREAMING_SNAKE. Run `cargo clippy --workspace --all-targets --all-features -D warnings` before opening a PR to catch lint issues and enforce consistent idioms.

## Testing Guidelines
Install `cargo-nextest` and prefer `cargo nextest run --lib --bins` for unit coverage. Feature-specific suites run with `cargo nextest run --no-default-features --features contract --test contract` and the analogous `chain` invocation. Populate `GITHUB_TOKEN` when invoking integration tests to avoid API rate limits. Update artifacts under `tests/snapshots` only when behavior changes are intentional and document the rationale in the PR.

## Commit & Pull Request Guidelines
Adopt Conventional Commits (`feat:`, `fix:`, `refactor:`, `ci:`) as seen in recent history, keeping the subject ≤72 characters and body explaining motivation plus testing notes. Squash noisy fixups locally. Pull requests should include: problem statement, summary of changes, commands run, and linked issues or tracking tickets. Attach terminal excerpts for impactful CLI updates and request reviews from maintainers responsible for the affected crate.

## Security & Tooling Checks
Use `cargo deny check` (or `cargo deny check advisories`) before merging to validate dependencies and licensing. Coordinate disclosure-sensitive issues privately; do not file public PRs for exploitable bugs until maintainers acknowledge receipt.
