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
	call_smart_contract, call_smart_contract_from_signed_payload, dry_run_call,
	dry_run_gas_estimate_call, get_call_payload, set_up_call, CallOpts,
};
pub use new::{create_smart_contract, is_valid_contract_name};
pub use node::{contracts_node_generator, is_chain_alive, run_contracts_node};
pub use templates::{Contract, ContractType};
pub use test::{test_e2e_smart_contract, test_smart_contract};
pub use testing::{mock_build_process, new_environment};
pub use up::{
	dry_run_gas_estimate_instantiate, dry_run_upload, get_code_hash_from_event, get_contract_code,
	get_instantiate_payload, get_upload_payload, instantiate_contract_signed,
	instantiate_smart_contract, set_up_deployment, set_up_upload, submit_signed_payload,
	upload_contract_signed, upload_smart_contract, ContractInfo, UpOpts,
};
pub use utils::{
	metadata::{get_message, get_messages, ContractFunction},
	parse_account, parse_hex_bytes,
};
// External exports
pub use contract_extrinsics::CallExec;
pub use ink_env::DefaultEnvironment;
