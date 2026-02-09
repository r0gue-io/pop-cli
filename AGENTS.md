# Repository Guidelines

## Project Structure & Module Organization

Pop CLI is organized as a Cargo workspace:

- **`crates/pop-cli/`** - Main CLI application with command implementations in `src/commands/`
- **`crates/pop-chains/`** - Parachain functionality (new chains, pallets, benchmarking, zombienet)
- **`crates/pop-contracts/`** - Smart contract functionality (ink! contracts, build, deploy, call)
- **`crates/pop-common/`** - Shared utilities (Git operations, manifest parsing, network helpers)
- **`crates/pop-telemetry/`** - Anonymous usage metrics collection
- **`crates/pop-fork/`** - Create local forks of live polkadot-sdk based chains
- `pallets/` holds runtime pallet templates used by chain scaffolding.
- `tests/` stores integration-test fixtures and snapshots (`tests/networks`, `tests/runtimes`, `tests/snapshots`).
- Packaging and release assets live in `debian/`, `Dockerfile*`, and `flake.nix`/`flake.lock` for Nix builds.

### Feature Flags
- **Default features**: `["chain", "telemetry", "contract"]`
- **`chain`** - Parachain development tools (includes `wallet-integration`)
- **`contract`** - ink! smart contract support (includes `wallet-integration`)
- **`telemetry`** - Anonymous usage analytics
- **`wallet-integration`** - Browser wallet connectivity for signing transactions
- **`integration-tests`** - Enable integration test helpers in pop-common

### Command Structure
Commands in `crates/pop-cli/src/commands/`:
- `new/` - Generate new parachain, pallet, or smart contract projects
- `build/` - Build projects and chain specifications (with `spec` subcommand)
- `up/` - Deploy contracts, launch networks (network, paseo, kusama, polkadot, westend subcommands)
- `call/` - Interact with chains (extrinsics) and contracts
- `test/` - Run tests and runtime operations (on-runtime-upgrade, execute-block, create-snapshot, fast-forward)
- `bench/` - Benchmark pallets and blocks
- `install/` - Install dependencies and tools
- `fork/` - Create local forks of live chains
- `clean/` - Remove cached artifacts and kill running nodes
- `hash/` - Hash data using various algorithms (blake2)
- `convert/` - Convert between address formats

### Important Notes
- Requires Rust 1.91.1 (set in `rust-toolchain.toml`)
- Uses `cargo nextest` for faster test execution
- Integration tests require the `integration-tests` feature flag
- Set `GITHUB_TOKEN` environment variable to avoid GitHub API rate limits during testing
- Uses Polkadot SDK crates extensively (subxt, sp-core, cumulus, frame-benchmarking-cli)
- CLI entry point is `crates/pop-cli/src/main.rs`, binary name is `pop`

## Build, Test, and Development Commands

### Build
- `cargo build` - Build with default features (chain, contract, telemetry)
- `cargo build --no-default-features --features chain` - Build for parachain functionality only
- `cargo build --no-default-features --features contract` - Build for smart contracts functionality only

### Testing
- `cargo nextest run --lib --bins` - Run unit tests only (preferred)
- `cargo test --lib --bins` - Run unit tests (fallback if nextest not available)
- `cargo test --doc` - Run documentation tests
- `cargo nextest run --no-default-features --features "contract,integration-tests" --test contract` - Run contract integration tests
- `cargo nextest run --no-default-features --features "chain,integration-tests" --test chain --test metadata` - Run parachain integration tests

### Linting and Formatting
- `cargo +nightly fmt --all -- --check` - Check code formatting
- `cargo +nightly fmt --all` - Format code
- `cargo clippy --all-targets -- -D warnings` - Run clippy with warnings as errors

### Security Checks
- `cargo deny check` - Check advisories and licenses
- `cargo deny check advisories` - Check security advisories only
- `cargo deny check licenses` - Check license compliance only

## Coding Style & Naming Conventions
- Rust 2024 edition; toolchain is pinned in `rust-toolchain.toml`.
- Formatting uses `rustfmt` with tabs (`hard_tabs = true`) and `max_width = 100` from `.rustfmt.toml`.
- Clippy is enabled with select allowances (e.g., `type_complexity`, `too_many_arguments`).
- Prefer idiomatic Rust module naming (`snake_case` files/modules, `CamelCase` types).
- Extract configuration values and repeated literals into named constants for readability.

## Testing Guidelines
- Unit tests live alongside code (`mod tests` in crates) and are run via `cargo nextest`.
- Integration tests are split by feature flags; expect external API rate limits and use `GITHUB_TOKEN` when needed.
- The full test suite is time-consuming. ALWAYS prefer running targeted tests relevant to the changes first, then crate-level tests, and only run the full workspace suite when explicitly asked.

## Terminology Guidelines

When writing code, documentation, or comments:

- **Use "Polkadot SDK"**, not "Substrate". The framework was rebranded; we use the current name.
- **Use "Paseo"** as the testnet, not "Westend". Paseo is the community testnet we support.
- Examples:
  - "Polkadot SDK chains" not "Substrate chains"
  - `wss://paseo.rpc.amforc.com` not `wss://westend-rpc.polkadot.io`

## Commit & Pull Request Guidelines
- Commit messages follow a Conventional Commits style (e.g., `feat: ...`, `fix: ...`, `refactor: ...`).
- Include a clear PR description, reference issues/PR numbers when applicable, and note test commands run.

## Security & Configuration Tips
- Use the pinned toolchain and `wasm32-unknown-unknown` target when building wasm-related artifacts.
- Run `cargo deny check advisories` before release-sensitive changes.
