# pop-parachains

A crate for generating, building and running Parachains and Pallets. 

> :information_source: A [crates.io](https://crates.io/crates/pop-parachains) version will be available soon!
## Usage

Generate a new Parachain:
```rust
use pop_parachains::{instantiate_template_dir, Config, Git, Template};

let template = Template::Base;
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
use pop_parachains::build_parachain;

let path = ...;
build_parachain(path)?;
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