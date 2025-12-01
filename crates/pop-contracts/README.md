# pop-contracts

A crate for generating, building, deploying, and calling [`ink!`](https://github.com/paritytech/ink) Smart Contracts.
Used by [`pop-cli`](https://github.com/r0gue-io/pop-cli).

## Usage

Generate a new Smart Contract:

```rust,no_run
use pop_contracts::{create_smart_contract, Contract};
use std::path::Path;

let contract_path = Path::new("./");
create_smart_contract("my-contract", &contract_path, &Contract::Standard);
```

Build an existing Smart Contract:

```rust,no_run
use pop_contracts::build_smart_contract;
use std::path::Path;
pub use contract_build::{Verbosity, MetadataSpec, BuildMode};

let contract_path = Path::new("./");
let build_mode = BuildMode::Release; // `Release` for release mode, `Debug` for debug mode, `Verifiable` for verifiable contract.
let result = build_smart_contract(&contract_path, build_mode, Verbosity::Default, Some(MetadataSpec::Ink), None); // You can build your contract with Solidity metadata using `Some(MetadataSpec::Solidity)`
```


Test an existing Smart Contract:

```rust,no_run
use pop_common::test_project;
use pop_contracts::test_e2e_smart_contract;
use std::path::Path;

let contract_path = Path::new("./");
let contracts_node_path = Path::new("./path-to-contracts-node-binary");

//unit testing
test_project(&contract_path, None);
//e2e testing
test_e2e_smart_contract(&contract_path, Some(contracts_node_path), None);
```

Deploy and instantiate an existing Smart Contract:

```rust,no_run
use pop_contracts::{ dry_run_gas_estimate_instantiate, instantiate_smart_contract, set_up_deployment, UpOpts};
use std::path::PathBuf;
use tokio_test;
use url::Url;

tokio_test::block_on(async {
    let contract_path = PathBuf::from("./");
    // prepare extrinsic for deployment
    let up_opts = UpOpts {
            path: contract_path,
            constructor: "new".to_string(),
            args: ["false".to_string()].to_vec(),
            value: "1000".to_string(),
            gas_limit: None,
            proof_size: None,
            url: Url::parse("ws://localhost:9944").unwrap(),
            suri: "//Alice".to_string(),
    };
    let instantiate_exec = set_up_deployment(up_opts).await.unwrap();

    // If you don't know the `gas_limit` and `proof_size`, you can perform a dry run to estimate the gas amount before instatianting the Smart Contract.
    let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await.unwrap();

    let contract_address = instantiate_smart_contract(instantiate_exec, weight).await.unwrap();
});
```

Upload a Smart Contract only:

```rust,no_run
use pop_contracts::{ dry_run_upload, set_up_upload, upload_smart_contract, UpOpts};
use std::path::PathBuf;
use tokio_test;
use url::Url;

tokio_test::block_on(async {
    // prepare extrinsic for deployment
    let contract_path = PathBuf::from("./");
    let up_opts = UpOpts {
            path: contract_path,
            constructor: "new".to_string(),
            args: ["false".to_string()].to_vec(),
            value: "1000".to_string(),
            gas_limit: None,
            proof_size: None,
            url: Url::parse("ws://localhost:9944").unwrap(),
            suri: "//Alice".to_string(),
    };
    let upload_exec = set_up_upload(up_opts).await.unwrap();
    // to perform only a dry-run
    let hash_code = dry_run_upload(&upload_exec).await.unwrap();
    // to upload the smart contract
    let code_hash = upload_smart_contract(&upload_exec).await.unwrap();
});
```

Call a deployed (and instantiated) Smart Contract:

```rust,no_run
use pop_contracts::{call_smart_contract, dry_run_call, dry_run_gas_estimate_call, set_up_call,CallOpts};
use std::path::PathBuf;
use tokio_test;
use url::Url;

tokio_test::block_on(async {
    // prepare extrinsic for call
    let contract_path = PathBuf::from("./");
    let get_call_opts = CallOpts {
        path: contract_path.clone(),
        contract: "5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A".to_string(),
        message: "get".to_string(),
        args: [].to_vec(),
        value: "1000".to_string(),
        gas_limit: None,
        proof_size: None,
        url: Url::parse("ws://localhost:9944").unwrap(),
        suri: "//Alice".to_string(),
        execute: false
    };
    let get_call_exec = set_up_call(get_call_opts).await.unwrap();
    // For operations that only require reading from the blockchain state, it does not require to submit an extrinsic.
    let call_dry_run_result = dry_run_call(&get_call_exec).await.unwrap();

    // For operations that change a storage value, thus altering the blockchain state, requires to submit an extrinsic.
    let flip_call_opts = CallOpts {
        path: contract_path,
        contract: "5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A".to_string(),
        message: "flip".to_string(),
        args: [].to_vec(),
        value: "1000".to_string(),
        gas_limit: None,
        proof_size: None,
        url: Url::parse("ws://localhost:9944").unwrap(),
        suri: "//Alice".to_string(),
        execute: true
    };
    let flip_call_exec = set_up_call(flip_call_opts).await.unwrap();
    let url = Url::parse("ws://localhost:9944").unwrap();
    // If you don't know the `gas_limit` and `proof_size`, you can perform a dry run to estimate the gas amount before calling the Smart Contract.
    let (_, weight_limit) = dry_run_gas_estimate_call(&flip_call_exec).await.unwrap();
    // Use this weight to execute the call.
    let call_result = call_smart_contract(flip_call_exec, weight_limit, &url).await.unwrap();
});
```

## Acknowledgements

`pop-contracts` would not be possible without the awesome crate: [
`cargo-contract`](https://github.com/use-ink/cargo-contract).
