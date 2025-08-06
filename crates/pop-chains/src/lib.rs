// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]

mod accounts;
/// Provides functionality for benchmarking.
pub mod bench;
mod build;
/// Provides functionality to construct, encode, sign, and submit chain extrinsics.
mod call;
/// Deployment providers' metadata and utility functions.
mod deployer_providers;
mod errors;
mod generator;
mod new_chain;
mod new_pallet;
/// A registry of parachains.
pub mod registry;
mod relay;
mod templates;
mod traits;
/// Provides functionality for testing runtime upgrades.
pub mod try_runtime;
/// Provides functionality for launching a local network.
pub mod up;
mod utils;

pub use bench::{
	binary::*, generate_binary_benchmarks, generate_omni_bencher_benchmarks,
	generate_pallet_benchmarks, get_preset_names, get_runtime_path, load_pallet_extrinsics,
	BenchmarkingCliCommand, GenesisBuilderPolicy, PalletExtrinsicsRegistry,
	GENESIS_BUILDER_DEV_PRESET,
};
pub use build::{
	binary_path, build_chain, build_project, export_wasm_file, generate_genesis_state_file,
	generate_plain_chain_spec, generate_raw_chain_spec, is_supported, runtime,
	runtime::{ContainerEngine, DeterministicBuilder},
	runtime_binary_path, ChainSpec,
};
pub use call::{
	construct_extrinsic, construct_proxy_extrinsic, construct_sudo_extrinsic, decode_call_data,
	encode_call_data,
	metadata::{
		action::{supported_actions, Action},
		find_dispatchable_by_name, find_pallet_by_name,
		params::{field_to_param, Param},
		parse_chain_metadata, Function, Pallet,
	},
	parse_and_format_events, set_up_client, sign_and_submit_extrinsic, submit_signed_extrinsic,
	CallData,
};
pub use deployer_providers::{DeploymentProvider, SupportedChains};
pub use errors::Error;
pub use indexmap::IndexSet;
pub use new_chain::instantiate_template_dir;
pub use new_pallet::{create_pallet_template, new_pallet_options::*, TemplatePalletConfig};
pub use relay::{clear_dmpq, RelayChain, Reserved};
pub use try_runtime::{
	binary::*, parse, parse_try_state_string, run_try_runtime, shared_parameters::*, state,
	try_state_details, try_state_label, upgrade_checks_details, TryRuntimeCliCommand,
};
// External export from subxt.
pub use subxt::{
	blocks::ExtrinsicEvents,
	tx::{DynamicPayload, Payload},
	OnlineClient, SubstrateConfig,
};
pub use templates::{ChainTemplate, Config, Provider};
pub use utils::helpers::is_initial_endowment_valid;
/// Information about the Node. External export from Zombienet-SDK.
pub use zombienet_sdk::NetworkNode;

const PASSET_HUB_SPEC_JSON: &str = include_str!("../artifacts/passet-hub-spec.json");
fn get_passet_hub_spec_content() -> &'static str {
	PASSET_HUB_SPEC_JSON
}
