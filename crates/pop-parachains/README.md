# pop-parachains

A crate for generating, building and running parachains and pallets. Used by
[`pop-cli`](https://github.com/r0gue-io/pop-cli).

## Usage

Generate a new parachain:
```rust
use pop_parachains::{instantiate_template_dir, Config, Git, Template};

let template = Template::Standard;
let destination_path = ...;
let config = Config {
    symbol: ...,
    decimals: ...,
    initial_endowment: ..,
}
instantiate_template_dir(template,destination_path,config)?;
```

Build a Parachain:
```rust
use pop_parachains::{build_parachain, Profile};

let path = ...;
build_parachain(path, Profile::Release, None)?;
```

Generate a raw chain specification file and export the WASM and genesis state files:
```rust
use pop_parachains::{binary_path, generate_chain_spec, generate_raw_chain_spec, export_wasm_file, generate_genesis_state_file};

let path = ...; // Location of the parachain project.
let para_id = 2000;
// The path to the node binary executable that contains the `build-spec` command.
let binary_path = binary_path(path.join("target/release"),path.join("node")) 
// Generate a plain chain specification file of a parachain
let plain_chain_spec = generate_chain_spec(&path, &binary_path, "plain-parachain-chainspec.json", para_id)?;
// Generate a raw chain specification file of a parachain
let chain_spec = generate_raw_chain_spec(&path, &plain_chain_spec, &binary_path, "raw-parachain-chainspec.json")?;
// Export the WebAssembly runtime for the parachain. 
let wasm_file = export_wasm_file(&path, &chain_spec, &binary_path, "para-2000-wasm")?; 
// Generate the parachain genesis state.
let genesis_state_file = generate_genesis_state_file(&path, &chain_spec, &binary_path, "para-2000-genesis-state")?; 
```

Run a Parachain:
```rust
use pop_parachains::Zombienet;


let cache = ... // The cache location, used for caching binaries.
let config_file = ...  // The Zombienet config to be used to launch a network.
let relay_chain_version = ... // relay_chain version if applies
let system_chain_version = ... // system_chain version if applies
let parachains_binaries = ... // The binaries required to launch parachains

let mut zombienet = Zombienet::new(
    cache,
    config_file,
    relay_chain_version,
    system_chain_version,
    parachains_binaries,
)
.await?;

zombienet.spawn().await?
```

To download the missing binaries before starting the network:
```rust
// Check if any binaries need to be sourced
let missing = zombienet.missing_binaries();
if missing.len() > 0 {
    for binary in missing {
        binary.source(&cache).await?;
    }
}
```

Generate a new Pallet:
```rust
use pop_parachains::{create_pallet_template, TemplatePalletConfig};

let path = ...;
let pallet_config = TemplatePalletConfig {
    name: ...,
    authors: ...,
    description: ...,
}

create_pallet_template(path,pallet_config)?;
```

## Acknowledgements
`pop-parachains` would not be possible without the awesome crate: [zombienet-sdk](https://github.com/paritytech/zombienet-sdk).
