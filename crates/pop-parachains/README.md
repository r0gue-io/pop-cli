# pop-parachains

A crate for generating, building and running parachains and pallets. Used by
[`pop-cli`](https://github.com/r0gue-io/pop-cli).

## Usage

Generate a new parachain:
```rust,no_run
use pop_parachains::{instantiate_template_dir, Config, Parachain};
use std::path::Path;

let destination_path = Path::new("./");
let tag_version = None; // Latest
let config = Config {
    symbol: "UNIT".to_string(),
    decimals: 12,
    initial_endowment: "1u64 << 60".to_string()
};
let tag = instantiate_template_dir(&Parachain::Standard, &destination_path, tag_version, config);
```

Build a Parachain:
```rust,no_run
use pop_common::Profile;
use pop_parachains::build_parachain;
use std::path::Path;

let path = Path::new("./");
let package = None;  // The optional package to be built.
let binary_path = build_parachain(&path, package, &Profile::Release, None).unwrap();
```

Generate a raw chain specification file and export the WASM and genesis state files:
```rust,no_run
use pop_common::Profile;
use pop_parachains::{build_parachain, export_wasm_file, generate_plain_chain_spec, generate_raw_chain_spec, generate_genesis_state_file};
use std::path::Path;

let path = Path::new("./"); // Location of the parachain project.
let package = None;  // The optional package to be built.
let para_id = 2000;
// The path to the node binary executable.
let binary_path = build_parachain(&path, package, &Profile::Release, None).unwrap();;
// Generate a plain chain specification file of a parachain
let plain_chain_spec_path = path.join("plain-parachain-chainspec.json");
generate_plain_chain_spec(&binary_path, &plain_chain_spec_path, para_id);
// Generate a raw chain specification file of a parachain
let chain_spec = generate_raw_chain_spec(&binary_path, &plain_chain_spec_path, "raw-parachain-chainspec.json").unwrap();
// Export the WebAssembly runtime for the parachain. 
let wasm_file = export_wasm_file(&binary_path, &chain_spec, "para-2000-wasm").unwrap(); 
// Generate the parachain genesis state.
let genesis_state_file = generate_genesis_state_file(&binary_path, &chain_spec, "para-2000-genesis-state").unwrap(); 
```

Run a Parachain:
```rust,no_run
use pop_parachains::Zombienet;
use std::path::Path;
use tokio_test;

tokio_test::block_on(async {
    let cache = Path::new("./cache"); // The cache location, used for caching binaries.
    let network_config = "network.toml"; // The configuration file to be used to launch a network.
    let relay_chain_version = None; // Latest
    let relay_chain_runtime_version = None; // Latest
    let system_parachain_version = None; // Latest
    let system_parachain_runtime_version = None; // Latest
    let parachains = None; // The parachain(s) specified.

    let mut zombienet = Zombienet::new(
        &cache,
        &network_config,
        relay_chain_version,
        relay_chain_runtime_version,
        system_parachain_version,
        system_parachain_runtime_version,
        parachains,
    ).await.unwrap();

    zombienet.spawn().await;

    //To download the missing binaries before starting the network:
    let release = true; // Whether the binary should be built using the release profile.
    let status = {}; // Mechanism to observe status updates
    let verbose = false; // Whether verbose output is required
    let missing = zombienet.binaries();
    for binary in missing {
        binary.source(release, &status, verbose).await; 
    }
})
```

Generate a new Pallet:
```rust,no_run
use pop_parachains::{create_pallet_template, TemplatePalletConfig};

let path = "./".to_string();
let pallet_config = TemplatePalletConfig {
    name: "MyPallet".to_string(),
    authors: "R0GUE".to_string(),
    description: "Template pallet".to_string()
};

create_pallet_template(Some(path),pallet_config);
```

## Acknowledgements
`pop-parachains` would not be possible without the awesome crate: [zombienet-sdk](https://github.com/paritytech/zombienet-sdk).
