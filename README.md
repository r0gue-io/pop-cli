# DoTemplate
<img src=".icons/logo.jpeg" height="400px" width="400px" align="left"></img>

Your one-stop entry into the exciting world of Blockchain development with *Polkadot*

## Getting Started

Use `DoTemplate` to either clone of your existing templates or instantiate a new parachain template: 

```sh
# Create a minimal parachain template
dotemplate create my-app
# Get the extended-parachain-template
dotemplate create my-app ept
# Get a pallet-contracts enabled template
dotemplate create my-app cpt
# Get a evm compatible parachain template
dotemplate create my-app fpt
```

You can also customize a template by providing config options for token symbol (as it appears on polkadot-js apps UI), token decimals, and the initial endowment for substrate developer accounts. Here's how: 

```sh
# Create a minimal parachain template with "DOT" as token symbol, 6 token decimals and 1 billion tokens per dev account
dotemplate create my-app --symbol DOT --decimals 6 --endowment 1_000_000_000
```
There's also the shorter version: 
```sh
dotemplate create my-app -s DOT -d 6 -i 1_000_000_000
```

Finally, you would need to build and run it.
```sh
cd my-app
cargo build --release
```
For running any parachain, we recommend using [zombienet](https://github.com/paritytech/zombienet).

_Zombinet integration coming soon..._