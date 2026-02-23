# Pop CLI

<img src="https://learn.onpop.io/~gitbook/image?url=https%3A%2F%2F574321477-files.gitbook.io%2F%7E%2Ffiles%2Fv0%2Fb%2Fgitbook-x-prod.appspot.com%2Fo%2Fspaces%252FUqTUVzYjmRwzCWTsfd1O%252Fuploads%252FxALe5uzVAiXnQFZOjxmw%252Fplay-on-polkadot.png%3Falt%3Dmedia%26token%3Dd8ce69f9-39fc-4568-9404-381032d923d4&width=400&dpr=2&quality=100&sign=19e161&sv=1"></img>

<div align="center">

[![Twitter URL](https://img.shields.io/twitter/follow/Pop?style=social)](https://x.com/onpopio/)
[![Twitter URL](https://img.shields.io/twitter/follow/R0GUE?style=social)](https://twitter.com/gor0gue)
[![Telegram](https://img.shields.io/badge/Telegram-gray?logo=telegram)](https://t.me/onpopio)



> An all-in-one tool for Polkadot development.

</div>

## Installation

### Quick Install (Recommended)

**Using cargo-binstall** (cross-platform - downloads pre-built binaries when available):
```shell
# Install cargo-binstall if you don't have it
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash

# Install pop-cli
cargo binstall pop-cli --locked
```

### macOS

**Using Homebrew:**
```shell
brew install r0gue-io/pop-cli/pop
```

### Linux

**Using Ubuntu PPA:**
```shell
sudo add-apt-repository ppa:r0gue-io/pop
sudo apt-get update
sudo apt-get install pop-cli
```
> :information_source: If `add-apt-repository` is not found, install it with `sudo apt-get install software-properties-common`.

**Using Homebrew:**
```shell
brew install r0gue-io/pop-cli/pop
```

**Debian/Ubuntu** (using `.deb` package from [releases](https://github.com/r0gue-io/pop-cli/releases)):
```shell
sudo dpkg -i pop-cli_*.deb
```

**Nix/NixOS:**
```shell
# Run directly without installing
nix run github:r0gue-io/pop-cli

# Or install to your profile
nix profile install github:r0gue-io/pop-cli
```

For NixOS configuration:
```nix
{
  inputs.pop-cli.url = "github:r0gue-io/pop-cli";
  # ...
  environment.systemPackages = [ inputs.pop-cli.packages.${system}.default ];
}
```

### From Source

**From crates.io** (compiles from source):
```shell
cargo install --locked pop-cli
```

**From GitHub** (latest development version):
```shell
cargo install --locked --git https://github.com/r0gue-io/pop-cli
```

### Requirements

> :information_source: Pop CLI requires Rust 1.91.1 or later.

> :information_source: For detailed instructions on how to install Pop CLI, please refer to our
> documentation: <https://learn.onpop.io/v/cli/installing-pop-cli>

### Telemetry

Pop CLI collects anonymous usage metrics to help us understand how the tool is being used and how we can improve it.
We do not collect any personal information. If you wish to disable telemetry
or read more about our telemetry practices please see
our [telemetry](crates/pop-telemetry/README.md) documentation.

## Documentation

On the [Pop Docs website](https://learn.onpop.io) you will find:

* 👉 [Get Started with Pop CLI](https://learn.onpop.io/v/cli)

## Building Pop CLI locally

Build with default features (parachain support):

```sh
cargo build
```

Build with Smart Contracts support (opt-in):

```sh
cargo build --features contract
```

Build for parachain functionality only:

```sh
cargo build --no-default-features --features chain
```

Build for Smart Contracts functionality only:

```sh
cargo build --no-default-features --features contract
```

> :information_source: Smart contract (ink!) support is available as an opt-in feature. To install with contract support: `cargo install pop-cli --locked --features contract`

## Testing Pop CLI

To test the tool locally. Due to the time it can take to build a Parachain or a Smart Contract, some tests have been
separated from the normal testing flow into integration tests.

We use `cargo nextest` for faster test runs.
```sh
cargo install cargo-nextest
```

Run the unit tests only:

```sh
# Recommended
cargo nextest run --lib --bins
# If you don't have nextest installed
cargo test --lib --bins
```

To run the integration tests relating to Smart Contracts:

```sh
cargo nextest run --no-default-features --features contract --test contract
```

To run the integration tests relating to Parachains:

```sh
cargo nextest run --no-default-features --features chain --test chain
cargo nextest run --no-default-features --features chain --test metadata
```

Run all tests (unit + integration):

```sh
cargo nextest run
```

> Running tests may result in rate limits being exhausted due to the reliance on the GitHub REST API for determining
> releases. As
> per <https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api#getting-a-higher-rate-limit>, a
> personal access token can be used via the `GITHUB_TOKEN` environment variable.

## Security/advisory checks

We use `cargo-deny` locally to check advisories and licenses.

```bash
cargo install cargo-deny
cargo deny check

# Advisories only
cargo deny check advisories
# Licenses only
cargo deny check licenses
```

## Acknowledgements

Pop CLI would not be possible without these awesome crates!

- Local network deployment powered by [zombienet-sdk](https://github.com/paritytech/zombienet-sdk)
- [cargo contract](https://github.com/use-ink/cargo-contract) a setup and deployment tool for developing Wasm based
  Smart Contracts via ink!
- Build deterministic runtimes powered by [srtool-cli](https://github.com/chevdor/srtool-cli)

## License

The entire code within this repository is licensed under the [GPLv3](./LICENSE).

Please [contact us](https://r0gue.io/contact) if you have questions about the licensing of our products.
