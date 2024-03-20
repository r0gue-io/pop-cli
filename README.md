# Pop CLI

<img src=".icons/logo.jpeg"></img>

An all-in-one tool for Polkadot development.

## Install

You can install Pop CLI as follows:

```shell
cargo install --locked --git https://github.com/r0gue-io/pop-cli
```

> :information_source: A [crates.io](https://crates.io/crates/pop-cli) version will be available soon!

## Getting Started

### Parachains

Use `pop` to generate a new parachain from a template:

```sh
# Create a minimal parachain
pop new parachain my-app
# Get a pallet-contracts enabled parachain
pop new parachain my-app cpt
# Get a evm compatible parachain
pop new parachain my-app fpt
```

Use `pop` to build your parachain.

```sh
# Build your parachain
pop build parachain -p ./my-app
```

or

```sh
cd my-app
pop build parachain
```

You can also customize your parachain by providing config options for token symbol (as it appears in chain metadata),
token decimals, and the initial endowment for developer accounts. Here's how:

```sh
# Create a minimal parachain with "DOT" as token symbol, 6 token decimals and 1 billion tokens per dev account
pop new parachain my-app --symbol DOT --decimals 6 --endowment 1_000_000_000
```

There's also the shorter version:

```sh
pop new parachain my-app -s DOT -d 6 -i 1_000_000_000
```

To create a new pallet, simply `pop new pallet`. And that's it. You will have a new `pallet-template` ready for hacking.
To customize the new pallet you can follow these options:

```sh
# create a pallet with name `pallet-awesome` in the current working directory
pop new pallet pallet-awesome
# or with options
pop new pallet pallet-awesome --authors Me --description "This pallet oozes awesomeness" --path my_app/pallets
```

Finally, you would need to build and run it.

```sh
cd my-app
pop build parachain --release
```

For running any parachain, we recommend using [zombienet](https://github.com/paritytech/zombienet).
See below for more information about using `pop up parachain` to help you with this.

### Contracts

Use `pop` to create a smart contract:

```sh
# Create a minimal smart contract
pop new contract my_contract
```

Test the smart contract:

```sh
# Test an existing smart contract
pop test contract -p ./my_contract
```

Build the smart contract:

```sh
# Build an existing smart contract
pop build contract -p ./my_contract
```

To deploy a contract you need your chain running. For testing purposes one option is to
run [substrate-contracts-node](https://github.com/paritytech/substrate-contracts-node):

```sh
cargo install contracts-node
substrate-contracts-node
```

> :information_source: We plan to automate this in the future.

Deploy and instantiate the smart contract:

```sh
pop up contract -p ./my_contract --constructor new --args "false" --suri //Alice
```

Some of the options available are:

- Specify the contract `constructor `to use, which in this example is `new()`.
- Specify the argument (`args`) to the constructor, which in this example is `false`.
- Specify the account uploading and instantiating the contract with `--suri`, which in this example is the default
  development account of `//Alice`.
  For other accounts, the actual secret key must be provided e.g. an 0x prefixed 64 bit hex string, or the seed phrase.

> :warning: **Use only for development**: Use a safer method of signing here before using this feature with production
> projects. We will be looking to provide alternative solutions in the future!

- You also can specify the url of your node with `--url ws://your-endpoint`, by default it is
  using `ws://localhost:9944`.

For more information about the options,
check [cargo-contract documentation](https://github.com/paritytech/cargo-contract/blob/master/crates/extrinsics/README.md#instantiate)


Interacting with the Smart Contract:

1. Read-only Operations: For operations that only require reading from the blockchain state. This approach does not require to submit an extrinsic (skip the flag `x/--execute`).
Example using the get() message:

```sh
pop call contract -p ./my_contract --contract $INSTANTIATED_CONTRACT_ADDRESS --message get --suri //Alice
```

2. State-modifying Operations: For operations that change a storage value, thus altering the blockchain state. Include the `x/--execute`  flag to submit an extrinsic on-chain.

Example executing the `flip()` message:

```sh
pop call contract -p ./my_contract --contract $INSTANTIATED_CONTRACT_ADDRESS --message flip --suri //Alice -x
```

### E2E testing

For end-to-end testing you will need to have a Substrate node with `pallet contracts`.
You do not need to run it in the background since the node is started for each test independently.
To install the latest version:

```
cargo install contracts-node --git https://github.com/paritytech/substrate-contracts-node.git
```

If you want to run any other node with `pallet-contracts` you need to change `CONTRACTS_NODE` environment variable:

```
export CONTRACTS_NODE="YOUR_CONTRACTS_NODE_PATH"
```

Run e2e testing on the smart contract:

```sh
# Run e2e tests for an existing smart contract
 pop test contract  -p ./my_contract --features e2e-tests
```

## Building Pop CLI locally

Build the tool locally with all the features:

```sh
cargo build --all-features
```

Build the tool only for parachain functionality:

```sh
cargo build --features parachain
```

Build the tool only for contracts functionality:

```sh
cargo build --features contract
```

## Spawn Network using Zombienet

You can spawn a local network using [zombienet](https://github.com/paritytech/zombienet-sdk) as follows:

```shell
pop up parachain -f ./tests/zombienet.toml -p https://github.com/r0gue-io/pop-node
```

> :information_source: Pop CLI will automatically source the necessary polkadot binaries. Currently, these will be built
> if on a non-linux system.
