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

Generate a plain chain specification file and customize it with your specific parachain values:

```rust,no_run
use pop_common::Profile;
use pop_parachains::{build_parachain, export_wasm_file, generate_plain_chain_spec, generate_raw_chain_spec, generate_genesis_state_file, ChainSpec};
use std::path::Path;

let path = Path::new("./"); // Location of the parachain project.
let package = None;  // The optional package to be built.
// The path to the node binary executable.
let binary_path = build_parachain(&path, package, &Profile::Release, None).unwrap();;
// Generate a plain chain specification file of a parachain
let plain_chain_spec_path = path.join("plain-parachain-chainspec.json");
generate_plain_chain_spec(&binary_path, &plain_chain_spec_path, true, "dev");
// Customize your chain specification
let mut chain_spec = ChainSpec::from(&plain_chain_spec_path).unwrap();
chain_spec.replace_para_id(2002);
chain_spec.replace_relay_chain("paseo-local");
chain_spec.replace_chain_type("Development");
chain_spec.replace_protocol_id("my-protocol");
// Writes the chain specification to a file
chain_spec.to_file(&plain_chain_spec_path).unwrap();
```

Generate a raw chain specification file and export the WASM and genesis state files:

```rust,no_run
use pop_common::Profile;
use pop_parachains::{build_parachain, export_wasm_file, generate_plain_chain_spec, generate_raw_chain_spec, generate_genesis_state_file};
use std::path::Path;

let path = Path::new("./"); // Location of the parachain project.
let package = None;  // The optional package to be built.
// The path to the node binary executable.
let binary_path = build_parachain(&path, package, &Profile::Release, None).unwrap();;
// Generate a plain chain specification file of a parachain
let plain_chain_spec_path = path.join("plain-parachain-chainspec.json");
generate_plain_chain_spec(&binary_path, &plain_chain_spec_path, true, "dev");
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
use std::path::PathBuf;

let path = "./";
let pallet_config = TemplatePalletConfig {
    authors: "R0GUE".to_string(),
    description: "Template pallet".to_string(),
    pallet_in_workspace: false,
    pallet_advanced_mode: true,
    pallet_default_config: true,
    pallet_common_types: Vec::new(),
    pallet_storage: Vec::new(),
    pallet_genesis: false,
    pallet_custom_origin: false,
};

create_pallet_template(PathBuf::from(path),pallet_config);
```

## Acknowledgements

`pop-parachains` would not be possible without the awesome
crate: [zombienet-sdk](https://github.com/paritytech/zombienet-sdk).
