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

pub use build::{BuildMode, ImageVariant, Verbosity, build_smart_contract, is_supported};
pub use call::{
	CallOpts, call_smart_contract, call_smart_contract_from_signed_payload, dry_run_call,
	dry_run_gas_estimate_call, get_call_payload, set_up_call,
};
pub use errors::Error;
pub use new::{create_smart_contract, is_valid_contract_name};
pub use node::{
	eth_rpc_generator, ink_node_generator, is_chain_alive, run_eth_rpc_node, run_ink_node,
};
pub use templates::Contract;
pub use test::test_e2e_smart_contract;
pub use testing::{mock_build_process, new_environment};
pub use up::{
	ContractInfo, UpOpts, dry_run_gas_estimate_instantiate, dry_run_upload, get_contract_code,
	instantiate_contract_signed, instantiate_smart_contract, set_up_deployment, set_up_upload,
	submit_signed_payload, upload_contract_signed, upload_smart_contract,
};
pub use utils::{
	metadata::{
		ContractCallable, ContractFunction, ContractStorage, FunctionType, Param, extract_function,
		fetch_contract_storage, fetch_contract_storage_with_param, get_contract_storage_info,
		get_message, get_messages,
	},
	parse_hex_bytes,
};
// External exports
pub use contract_build::MetadataSpec;
pub use contract_extrinsics::{CallExec, ExtrinsicOpts, UploadCode};
pub use ink_env::{DefaultEnvironment, Environment};
pub use sp_core::Bytes;
pub use sp_weights::Weight;
pub use up::{get_instantiate_payload, get_upload_payload};
pub use utils::map_account::AccountMapper;
