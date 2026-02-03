# Repository Guidelines

## Project Structure & Module Organization
- `crates/` contains workspace crates: `pop-cli` (binary), plus supporting libs like `pop-chains`, `pop-contracts`, `pop-common`, and `pop-telemetry`.
- `pallets/` holds runtime pallet templates used by chain scaffolding.
- `tests/` stores integration-test fixtures and snapshots (`tests/networks`, `tests/runtimes`, `tests/snapshots`).
- Packaging and release assets live in `debian/`, `Dockerfile*`, and `flake.nix`/`flake.lock` for Nix builds.

## Build, Test, and Development Commands
- `cargo build --no-default-features --features chain` builds only parachain features.
- `cargo build --no-default-features --features contract` builds only smart-contract features.
- `cargo nextest run --lib --bins` runs unit tests (preferred); `cargo test --lib --bins` is the fallback.
- `cargo nextest run --no-default-features --features contract --test contract` runs contract integration tests.
- `cargo nextest run --no-default-features --features chain --test chain` and `--test metadata` run chain integration tests.
- `cargo deny check` runs security and license checks (see `deny.toml`).

## Coding Style & Naming Conventions
- Rust 2024 edition; toolchain is pinned in `rust-toolchain.toml` (Rust 1.91.1).
- Formatting uses `rustfmt` with tabs (`hard_tabs = true`) and `max_width = 100` from `.rustfmt.toml`.
- Clippy is enabled in `crates/pop-cli/Cargo.toml` with select allowances (e.g., `type_complexity`, `too_many_arguments`).
- Prefer idiomatic Rust module naming (`snake_case` files/modules, `CamelCase` types).

## Testing Guidelines
- Unit tests live alongside code (e.g., `*_test.rs` or `mod tests` in crates) and are run via `cargo nextest`.
- Integration tests are split by feature flags as noted above; expect external API rate limits and use `GITHUB_TOKEN` when needed.

## Commit & Pull Request Guidelines
- Commit messages follow a Conventional Commits style (e.g., `feat: ...`, `fix: ...`, `refactor: ...`).
- Include a clear PR description, reference issues/PR numbers when applicable, and note test commands run.

## Security & Configuration Tips
- Use the pinned toolchain and `wasm32-unknown-unknown` target when building wasm-related artifacts.
- Run `cargo deny check advisories` before release-sensitive changes.
