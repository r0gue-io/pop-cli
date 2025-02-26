use ac_node_api::Metadata;
use anyhow::Result;
use clap::Parser;
use frame_benchmarking_cli::PalletCmd;
use frame_metadata::RuntimeMetadataPrefixed;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use pop_common::get_relative_or_absolute_path;
use sc_chain_spec::GenesisConfigBuilderRuntimeCaller;
use sp_runtime::traits::BlakeTwo256;
use std::{
	collections::HashMap,
	env::current_dir,
	fs::{self},
	path::{Path, PathBuf},
};
use subxt::ext::codec::{Decode, Encode};
use wasm_loader::Source;
use wasm_testbed::WasmTestBed;

/// Constant variables used for benchmarking.
pub mod constants {
	/// Do not provide any genesis state.
	pub const GENESIS_BUILDER_NO_POLICY: &str = "none";
	/// Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API.
	pub const GENESIS_BUILDER_RUNTIME_POLICY: &str = "runtime";
}

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Type alias for records where the key is the pallet name and the value is a array of its
/// extrinsics.
pub type PalletExtrinsicsRegistry = HashMap<String, Vec<String>>;

/// Get genesis builder preset names of the runtime.
///
/// # Arguments
/// * `binary_path` - Path to the runtime WASM binary.
pub fn get_preset_names(binary_path: &PathBuf) -> anyhow::Result<Vec<String>> {
	let binary = fs::read(binary_path).expect("No runtime binary found");
	let genesis_config_builder = GenesisConfigBuilderRuntimeCaller::<HostFunctions>::new(&binary);
	genesis_config_builder.preset_names().map_err(|e| anyhow::anyhow!(e))
}

/// Get the runtime folder path and throws error if not exist.
///
/// # Arguments
/// * `parent` - Parent path that contains the runtime folder.
pub fn get_runtime_path(parent: &Path) -> anyhow::Result<PathBuf> {
	["runtime", "runtimes"]
		.iter()
		.map(|f| parent.join(f))
		.find(|path| path.exists())
		.ok_or_else(|| anyhow::anyhow!("No runtime found"))
}

/// Loads a mapping of pallets and their associated extrinsics from the runtime WASM binary.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime WASM binary.
pub fn load_pallet_extrinsics(runtime_path: &Path) -> anyhow::Result<PalletExtrinsicsRegistry> {
	let mut registry = PalletExtrinsicsRegistry::default();
	let source = Source::File(runtime_path.to_path_buf());
	let wasm = WasmTestBed::new(&source)?;
	let encoded = wasm.runtime_metadata_prefixed().encode();
	let runtime_metadata_prefixed = RuntimeMetadataPrefixed::decode(&mut &encoded[..])
		.map_err(|_| anyhow::anyhow!("Failed to decode the "))?;
	let metadata = Metadata::try_from(runtime_metadata_prefixed).unwrap();

	for pallet in metadata.pallets() {
		if let Some(call_variants) = pallet.call_variants() {
			let mut calls: Vec<String> = call_variants.iter().map(|c| c.name.clone()).collect();
			calls.sort_by(|c1, c2| c2.cmp(c1));
			registry.insert(pallet.name().to_string(), calls);
		}
	}
	Ok(registry)
}

/// Print the pallet benchmarking command with arguments.
///
/// # Arguments
/// * `cmd` - Command to benchmarking extrinsic weights of FRAME pallets.
pub fn print_pallet_command(cmd: &PalletCmd) -> String {
	let mut args = vec!["pop bench pallet".to_string()];

	if let Some(ref pallet) = cmd.pallet {
		args.push(format!("--pallet={}", pallet));
	}
	if let Some(ref extrinsic) = cmd.extrinsic {
		args.push(format!("--extrinsic={}", extrinsic));
	}
	if !cmd.exclude_pallets.is_empty() {
		args.push(format!("--exclude-pallets={}", cmd.exclude_pallets.join(",")));
	}

	args.push(format!("--steps={}", cmd.steps));

	if !cmd.lowest_range_values.is_empty() {
		args.push(format!(
			"--low={}",
			cmd.lowest_range_values
				.iter()
				.map(ToString::to_string)
				.collect::<Vec<_>>()
				.join(",")
		));
	}
	if !cmd.highest_range_values.is_empty() {
		args.push(format!(
			"--high={}",
			cmd.highest_range_values
				.iter()
				.map(ToString::to_string)
				.collect::<Vec<_>>()
				.join(",")
		));
	}

	args.extend([
		format!("--repeat={}", cmd.repeat),
		format!("--external-repeat={}", cmd.external_repeat),
		format!("--db-cache={}", cmd.database_cache_size),
		format!("--map-size={}", cmd.worst_case_map_values),
		format!("--additional-trie-layers={}", cmd.additional_trie_layers),
	]);

	if cmd.json_output {
		args.push("--json".to_string());
	}
	if let Some(ref json_file) = cmd.json_file {
		args.push(format!("--json-file={}", json_file.display()));
	}
	if cmd.no_median_slopes {
		args.push("--no-median-slopes".to_string());
	}
	if cmd.no_min_squares {
		args.push("--no-min-squares".to_string());
	}
	if cmd.no_storage_info {
		args.push("--no-storage-info".to_string());
	}
	if let Some(ref output) = cmd.output {
		args.push(format!("--output={}", output.display()));
	}
	if let Some(ref header) = cmd.header {
		args.push(format!("--header={}", header.display()));
	}
	if let Some(ref template) = cmd.template {
		args.push(format!("--template={}", template.display()));
	}
	if let Some(ref output_analysis) = cmd.output_analysis {
		args.push(format!("--output-analysis={}", output_analysis));
	}
	if let Some(ref output_pov_analysis) = cmd.output_pov_analysis {
		args.push(format!("--output-pov-analysis={}", output_pov_analysis));
	}
	if let Some(ref heap_pages) = cmd.heap_pages {
		args.push(format!("--heap-pages={}", heap_pages));
	}
	if cmd.no_verify {
		args.push("--no-verify".to_string());
	}
	if cmd.extra {
		args.push("--extra".to_string());
	}
	if let Some(ref runtime) = cmd.runtime {
		args.push(format!("--runtime={}", runtime.display()));
	}
	if cmd.allow_missing_host_functions {
		args.push("--allow-missing-host-functions".to_string());
	}
	if let Some(ref genesis_builder) = cmd.genesis_builder {
		let builder_str = serde_json::to_string(genesis_builder).unwrap().to_lowercase();
		args.push(format!("--genesis-builder={}", builder_str));

		if builder_str == constants::GENESIS_BUILDER_RUNTIME_POLICY {
			args.push(format!("--genesis-builder-preset={}", cmd.genesis_builder_preset));
		}
	}
	if let Some(ref execution) = cmd.execution {
		args.push(format!("--execution={}", execution));
	}
	if let Some(ref json_input) = cmd.json_input {
		args.push(format!("--json-input={}", json_input.display()));
	}
	if cmd.unsafe_overwrite_results {
		args.push(format!("--unsafe-overwrite-results={}", cmd.unsafe_overwrite_results));
	}
	args.join(" ")
}

/// Parse the pallet command from string value of genesis policy builder.
///
/// # Arguments
/// * `policy` - Genesis builder policy ( none | spec | runtime ).
pub fn parse_genesis_builder_policy(policy: &str) -> anyhow::Result<PalletCmd> {
	PalletCmd::try_parse_from([
		"",
		"--list",
		"--runtime",
		"dummy-runtime", // For parsing purpose.
		"--genesis-builder",
		policy,
	])
	.map_err(|e| {
		anyhow::anyhow!(format!(r#"Invalid genesis builder option {policy}: {}"#, e.to_string()))
	})
}

/// Run command for pallet benchmarking.
///
/// # Arguments
/// * `cmd` - Command to benchmark the FRAME Pallets.
pub fn run_pallet_benchmarking(cmd: &PalletCmd) -> Result<()> {
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!(format!("Failed to run benchmarking: {}", e.to_string())))
}

/// Performs a fuzzy search for pallets that match the provided input.
///
/// # Arguments
/// * `registry` - A mapping of pallets and their extrinsics.
/// * `excluded_pallets` - Pallets that are excluded from the search results.
/// * `input` - The search input used to match pallets.
/// * `limit` - Maximum number of pallets returned from search.
pub fn search_for_pallets(
	registry: &PalletExtrinsicsRegistry,
	excluded_pallets: &[String],
	input: &str,
	limit: usize,
) -> Vec<String> {
	let matcher = SkimMatcherV2::default();
	let pallets = registry.keys();

	if input.is_empty() {
		return pallets.map(String::from).take(limit).collect();
	}
	let pallets: Vec<&str> = pallets
		.filter(|s| !excluded_pallets.contains(&s.to_string()))
		.map(String::as_str)
		.collect();
	let mut output: Vec<(String, i64)> = pallets
		.into_iter()
		.map(|v| (v.to_string(), matcher.fuzzy_match(v, input).unwrap_or_default()))
		.collect();
	output.sort_by(|a, b| b.1.cmp(&a.1));
	output.into_iter().map(|(name, _)| name).take(limit).collect::<Vec<String>>()
}

/// Performs a fuzzy search for extrinsics that match the provided input.
///
/// # Arguments
/// * `pallet_extrinsics` - A mapping of pallets and their extrinsics.
/// * `pallets` - List of pallets used to find the extrinsics.
/// * `input` - The search input used to match extrinsics.
pub fn search_for_extrinsics(
	registry: &PalletExtrinsicsRegistry,
	pallets: &Vec<String>,
	input: &str,
	limit: usize,
) -> Vec<String> {
	let matcher = SkimMatcherV2::default();
	let extrinsics: Vec<&str> = registry
		.iter()
		.filter(|(pallet, _)| pallets.contains(pallet))
		.flat_map(|(_, extrinsics)| extrinsics.iter().map(String::as_str))
		.collect();

	if input.is_empty() {
		return extrinsics.into_iter().map(String::from).take(limit).collect();
	}
	let mut output: Vec<(String, i64)> = extrinsics
		.into_iter()
		.map(|v| (v.to_string(), matcher.fuzzy_match(v, input).unwrap_or_default()))
		.collect();
	output.sort_by(|a, b| b.1.cmp(&a.1));
	output.into_iter().map(|(name, _)| name).take(limit).collect::<Vec<String>>()
}

/// Get serialized value of the  the pallet benchmarking command's genesis builder.
///
/// # Arguments
/// * `cmd` - Command to benchmark the FRAME Pallets.
pub fn get_serialized_genesis_builder(cmd: &PalletCmd) -> String {
	let genesis_builder = cmd.genesis_builder.as_ref().expect("No policy provided");
	serde_json::to_string(genesis_builder)
		.expect("Failed to convert genesis builder policy to string")
		.replace('"', "")
		.to_lowercase()
}
/// Get relative path of the runtime.
///
/// # Arguments
/// * `cmd` - Command to benchmark the FRAME Pallets.
pub fn get_relative_runtime_path(cmd: &PalletCmd) -> String {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let runtime_path = cmd.runtime.as_ref().expect("No runtime provided");
	let path = get_relative_or_absolute_path(cwd.as_path(), runtime_path.as_path());
	path.as_path().to_str().expect("No path provided").to_string()
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[test]
	fn get_preset_names_works() -> anyhow::Result<()> {
		assert_eq!(
			get_preset_names(&get_mock_runtime_path(true))?,
			vec!["development", "local_testnet"]
		);
		Ok(())
	}

	#[test]
	fn get_runtime_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		for name in ["runtime", "runtimes"] {
			let path = temp_dir.path();
			fs::create_dir(&path.join(name))?;
			get_runtime_path(&path)?;
		}
		Ok(())
	}

	#[test]
	fn load_pallet_extrinsics_works() -> Result<()> {
		let registry = load_pallet_extrinsics(&get_mock_runtime_path(true))?;
		assert_eq!(registry.get("Timestamp").cloned().unwrap_or_default(), ["on_finalize", "set"]);
		assert_eq!(
			registry.get("Sudo").cloned().unwrap_or_default(),
			["check_only_sudo_account", "remove_key", "set_key", "sudo", "sudo_as"]
		);
		Ok(())
	}

	#[test]
	fn search_pallets_works() -> Result<()> {
		let runtime_path = get_mock_runtime_path(true);
		let registry = load_pallet_extrinsics(&runtime_path)?;
		[("message", "MessageQueue"), ("timestamp", "Timestamp"), ("balances", "Balances")]
			.iter()
			.for_each(|(input, pallet)| {
				let pallets = search_for_pallets(&registry, &[], input, 5);
				assert_eq!(pallets.first(), Some(&pallet.to_string()));
				assert_eq!(pallets.len(), 5);
			});

		assert_ne!(
			search_for_pallets(&registry, &["MessageQueue".to_string()], "message", 5).first(),
			Some(&"MessageQueue".to_string())
		);
		Ok(())
	}

	#[test]
	fn search_extrinsics_works() -> Result<()> {
		let runtime_path = get_mock_runtime_path(true);
		let registry = load_pallet_extrinsics(&runtime_path)?;
		let extrinsics =
			search_for_extrinsics(&registry, &vec!["pallet_timestamp".to_string()], "", 5);
		assert_eq!(extrinsics, vec!["on_finalize".to_string(), "set".to_string()]);
		assert_eq!(
			search_for_extrinsics(&registry, &vec!["pallet_timestamp".to_string()], "set", 5),
			vec!["set".to_string(), "on_finalize".to_string()]
		);
		Ok(())
	}

	#[test]
	fn parse_genesis_builder_policy_works() -> anyhow::Result<()> {
		for policy in ["none", "runtime"] {
			parse_genesis_builder_policy(policy)?;
		}
		Ok(())
	}

	fn get_mock_runtime_path(with_runtime_benchmarks: bool) -> PathBuf {
		let binary_path = format!(
			"../../tests/runtimes/{}.wasm",
			if with_runtime_benchmarks { "base_parachain_benchmark" } else { "base_parachain" }
		);
		std::env::current_dir().unwrap().join(binary_path).canonicalize().unwrap()
	}
}
