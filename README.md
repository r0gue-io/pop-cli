# POP
<img src=".icons/logo.jpeg"></img>

Your one-stop entry into the exciting world of Blockchain development with *Polkadot*

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
pop new contract my-contract
```

Test the smart contract: 
```sh
# Test an existing smart contract
pop test contract -p ./my-contract
```

Build the smart contract: 
```sh
# Build an existing smart contract
pop build contract -p ./my-contract
```

For e2e testing you will need to have a Substrate node with `pallet contracts`.
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