# POP
<img src=".icons/logo.jpeg"></img>

An all-in-one tool for Polkadot development.

## Install
You can install Pop CLI as follows:
```shell
cargo install --git https://github.com/r0gue-io/pop-cli
```

## Getting Started

### Parachains
Use `pop` to either clone of your existing templates or instantiate a new parachain template: 

```sh
# Create a minimal parachain template
pop new parachain my-app
# Get the extended-parachain-template
pop new parachain my-app ept
# Get a pallet-contracts enabled template
pop new parachain my-app cpt
# Get a evm compatible parachain template
pop new parachain my-app fpt
```

You can also customize a template by providing config options for token symbol (as it appears on polkadot-js apps UI), token decimals, and the initial endowment for substrate developer accounts. Here's how: 

```sh
# Create a minimal parachain template with "DOT" as token symbol, 6 token decimals and 1 billion tokens per dev account
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
cargo build --release
```
For running any parachain, we recommend using [zombienet](https://github.com/paritytech/zombienet).


### Contracts
Use `pop` to create a smart contract template: 

```sh
# Create a minimal smart contract template
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

### Build locally

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

For running any parachain, we recommend using [zombienet](https://github.com/paritytech/zombienet).

## Spawn Network using Zombienet
You can spawn a local network as follows:
```shell
pop up parachain -f ./tests/zombienet.toml -p https://github.com/r0gue-io/pop-node
```
