# pop-contracts

A crate for generating, building, deploying and calling [`ink!`](https://github.com/paritytech/ink) Smart Contracts. 

## Usage

Generate a new Smart Contract:
```rust
use pop_contracts::create_smart_contract;

let name = '...';
let contract_path = ...;
create_smart_contract(name, &contract_path)?;
```

Build an existing Smart Contract:
```rust
use pop_contracts::build_smart_contract;

let contract_path = ...;
build_smart_contract(&contract_path)?;
```

Test an existing Smart Contract:
```rust
use pop_contracts::{test_e2e_smart_contract, test_smart_contract};

let contract_path = ...;

//unit testing
test_smart_contract(&contract_path)?;
//e2e testing
test_e2e_smart_contract(&contract_path)?;
```

Deploy and instantiate an existing Smart Contract:
```rust
use pop_contracts::{ instantiate_smart_contract, set_up_deployment, UpOpts};

// prepare extrinsic for deployment
let up_opts = UpOpts {
    path: ...,
	constructor: ...,
	args: ...,
	value: ...,
	gas_limit: ...,
	proof_size: ...,
	salt: ...,
	url: ...,
	suri: ...,
}
let instantiate_exec = set_up_deployment(up_opts);


let contract_address = instantiate_smart_contract(instantiate_exec,  Weight::from_parts(gas_limit, proof_size))
			.await
			.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;
```

If you don't know the `gas_limit` and `proof_size`, you can perform a dry run to estimate the gas amount before instatianting the Smart Contract:
```rust
use pop_contracts::{ instantiate_smart_contract, dry_run_gas_estimate_instantiate};

let weight_limit = match dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
let contract_address = instantiate_smart_contract(instantiate_exec,  weight_limit)
			.await
			.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;
```

Call a deployed (and instantiated) Smart Contract:
```rust
use pop_contracts::{set_up_call, CallOpts};

// prepare extrinsic for call
let call_opts = CallOpts {
    path: ...,
	contract: ...,
	message: ...,
	args: ...,
	value: ...,
	gas_limit: ...,
	proof_size: ...,
	url: ...,
	suri: ...,
	execute: ...,
}
let call_exec = set_up_call(call_opts).await?;
```
For operations that only require reading from the blockchain state, it does not require to submit an extrinsic:
```rust
use pop_contracts::dry_run_call;

let call_dry_run_result = dry_run_call(&call_exec).await?;
```
For operations that change a storage value, thus altering the blockchain state, requires to submit an extrinsic:
```rust
use pop_contracts::call_smart_contract;

let url = ....;
let call_result = call_smart_contract(call_exec, Weight::from_parts(gas_limit, proof_size), url)
				.await
				.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;
```
Same as above, if you don't know the `gas_limit` and `proof_size`, you can perform a dry run to estimate the gas amount before calling the Smart Contract:
```rust
use pop_contracts::{ call_smart_contract, dry_run_gas_estimate_call};

let url = ....;
let weight_limit = match dry_run_gas_estimate_call(&call_exec).await?;
let contract_address = call_smart_contract(call_exec,  weight_limit, url)
			.await
			.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;
```

## Acknowledgements
`pop-contracts` would not be possible without the awesome crate: [`cargo-contract`](https://github.com/paritytech/cargo-contract).