// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]
mod build;
mod call;
mod errors;
mod new;
mod node;
mod templates;
mod test;
mod testing;
mod up;
mod utils;

pub use build::{build_smart_contract, is_supported, Verbosity};
pub use call::{
	call_smart_contract, dry_run_call, dry_run_gas_estimate_call, set_up_call, CallOpts,
};
pub use new::{create_smart_contract, is_valid_contract_name};
pub use node::{contracts_node_generator, is_chain_alive, run_contracts_node};
pub use templates::{Contract, ContractType};
pub use test::{test_e2e_smart_contract, test_smart_contract};
pub use testing::{mock_build_process, new_environment};
pub use up::{
	dry_run_gas_estimate_instantiate, dry_run_upload, instantiate_smart_contract,
	set_up_deployment, set_up_upload, upload_smart_contract, UpOpts,
};
pub use utils::{
	metadata::{get_messages, ContractFunction},
	parse_account, parse_hex_bytes,
};
