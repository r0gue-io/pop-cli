// SPDX-License-Identifier: GPL-3.0

use super::display_message;
use crate::cli::{
	self,
	traits::{Confirm, Input, MultiSelect, Select},
};
use clap::Args;
use cliclack::{spinner, ProgressBar};
use frame_benchmarking_cli::PalletCmd;
use log::LevelFilter;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_project, constants::*, get_preset_names, get_relative_runtime_path, get_runtime_path,
	get_searilized_genesis_builder, load_pallet_extrinsics, parse_genesis_builder_policy,
	print_pallet_command, run_pallet_benchmarking, runtime_binary_path, search_for_extrinsics,
	search_for_pallets, PalletExtrinsicsRegistry,
};
use std::{collections::HashMap, env::current_dir, ffi::OsStr, fs, path::PathBuf, str::FromStr};
use strum::{EnumMessage, IntoEnumIterator};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};

const ALL_SELECTED: &str = "*";
const MAX_EXTRINSIC_LIMIT: usize = 10;
const MAX_PALLET_LIMIT: usize = 20;

#[derive(Args)]
pub(crate) struct BenchmarkPalletArgs {
	#[command(flatten)]
	pub command: PalletCmd,

	/// If this is set to true, no parameter menu pops up
	#[arg(long = "skip")]
	pub skip_menu: bool,
}

impl BenchmarkPalletArgs {
	pub fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let cmd = &mut self.command;
		if cmd.list.is_some() || cmd.json_output {
			if let Err(e) = run_pallet_benchmarking(cmd) {
				return display_message(&e.to_string(), false, cli);
			}
		}
		// If `all` is provided, we override the value of `pallet` and `extrinsic` to select all.
		if cmd.all {
			cmd.pallet = Some(ALL_SELECTED.to_string());
			cmd.extrinsic = Some(ALL_SELECTED.to_string());
			cmd.all = false;
		}

		let mut registry: PalletExtrinsicsRegistry = HashMap::default();
		cli.intro("Benchmarking your pallets")?;
		cli.warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)?;
		if let Some(ref spec) = cmd.shared_params.chain {
			return display_message(
				&format!(
					"Chain specs are not supported. Please remove `--chain={spec}` \
					       and use `--runtime=<PATH>` instead"
				),
				false,
				cli,
			);
		}
		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build
		// the runtime.
		if cmd.runtime.is_none() {
			match ensure_runtime_binary_exists(cli, &Profile::Release) {
				Ok(runtime_binary_path) => cmd.runtime = Some(runtime_binary_path),
				Err(e) => {
					return display_message(&e.to_string(), false, cli);
				},
			}
		}
		// No genesis builder, prompts user to select the genesis builder policy.
		if cmd.genesis_builder.is_none() {
			if let Err(e) = guide_user_to_configure_genesis(cmd, cli) {
				return display_message(&e.to_string(), false, cli);
			};
		}
		// No pallet provided, prompts user to select the pallets fetched from runtime.
		if cmd.pallet.is_none() {
			update_pallets(cmd, cli, &mut registry)?;
		}
		// No extrinsic provided, prompts user to select the extrinsics fetched from runtime.
		if cmd.extrinsic.is_none() {
			update_extrinsics(cmd, cli, &mut registry)?;
		}

		// Only prompt user to update parameters when `skip_menu` is not provided.
		if !self.skip_menu {
			loop {
				let option = guide_user_to_select_menu_option(cmd, cli)?;
				match option.update_arguments(cmd, &mut registry, cli) {
					Ok(true) => break,
					Ok(false) => continue,
					Err(e) => cli.info(e)?,
				}
			}
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking extrinsic weights of selected pallets...")?;
		let result = run_pallet_benchmarking(cmd);

		// Display the benchmarking command.
		let mut message = print_pallet_command(cmd);
		if self.skip_menu {
			message.push_str(" --skip");
		}
		cli.info(message)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}
}

#[derive(Clone, Copy, EnumIter, EnumMessageDerive, Eq, PartialEq)]
enum BenchmarkPalletMenuOption {
	/// FRAME Pallets to benchmark
	#[strum(message = "Pallets")]
	Pallets,
	/// Extrinsics inside the pallet to benchmark
	#[strum(message = "Extrinsics")]
	Extrinsics,
	/// Comma separated list of pallets that should be excluded from the benchmark
	#[strum(message = "Excluded pallets")]
	ExcludedPallets,
	/// Path to the runtime WASM binary
	#[strum(message = "Runtime path")]
	Runtime,
	/// How to construct the genesis state
	#[strum(message = "Genesis builder")]
	GenesisBuilderPolicy,
	/// The preset that we expect to find in the GenesisBuilder runtime API
	#[strum(message = "Genesis builder preset")]
	GenesisBuilderPreset,
	/// How many samples we should take across the variable components
	#[strum(message = "Steps")]
	Steps,
	/// How many repetitions of this benchmark should run from within the wasm
	#[strum(message = "Repeats")]
	Repeat,
	/// Indicates highest values for each of the component ranges
	#[strum(message = "High")]
	High,
	/// Indicates lowest values for each of the component ranges
	#[strum(message = "Low")]
	Low,
	/// The assumed default maximum size of any `StorageMap`
	#[strum(message = "Map size")]
	MapSize,
	/// Limit the memory (in MiB) the database cache can use
	#[strum(message = "Database cache size")]
	DatabaseCacheSize,
	/// Adjust the PoV estimation by adding additional trie layers to it
	#[strum(message = "Additional trie layer")]
	AdditionalTrieLayer,
	/// Don't print the median-slopes linear regression analysis
	#[strum(message = "No median slope")]
	NoMedianSlope,
	/// Don't print the min-squares linear regression analysis
	#[strum(message = "No min square")]
	NoMinSquare,
	///  If enabled, the storage info is not displayed in the output next to the analysis
	#[strum(message = "No storage info")]
	NoStorageInfo,
	#[strum(message = "> Save all parameter changes and continue")]
	SaveAndContinue,
}

impl BenchmarkPalletMenuOption {
	// Check if the menu option is disabled. If disabled, the menu option is not displayed in the
	// menu.
	fn is_disabled(self, cmd: &PalletCmd) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		Ok(match self {
			GenesisBuilderPreset =>
				cmd.genesis_builder == parse_genesis_builder_policy("none")?.genesis_builder,
			// If there are multiple pallets provided, disable the extrinsics.
			Extrinsics => cmd.pallet.as_ref().expect("No pallet provided").matches(",").count() > 0,
			_ => false,
		})
	}

	// Reads the command argument based on the selected menu option.
	//
	// This method retrieves the appropriate value from `PalletCmd` depending on
	// the `BenchmarkPalletMenuOption` variant. It formats the value as a string
	// for display or further processing.
	fn read_command(self, cmd: &PalletCmd) -> anyhow::Result<String> {
		use BenchmarkPalletMenuOption::*;
		Ok(match self {
			Pallets => self.get_joined_string(cmd.pallet.as_ref().expect("No pallet provided")),
			Extrinsics =>
				self.get_joined_string(cmd.extrinsic.as_ref().expect("No extrinsic provided")),
			ExcludedPallets =>
				if cmd.exclude_pallets.is_empty() {
					"None".to_string()
				} else {
					cmd.exclude_pallets.join(",")
				},
			Runtime => get_relative_runtime_path(cmd),
			GenesisBuilderPolicy => get_searilized_genesis_builder(cmd),
			GenesisBuilderPreset => cmd.genesis_builder_preset.clone(),
			Steps => cmd.steps.to_string(),
			Repeat => cmd.repeat.to_string(),
			High => self.get_range_values(&cmd.highest_range_values),
			Low => self.get_range_values(&cmd.lowest_range_values),
			MapSize => cmd.worst_case_map_values.to_string(),
			DatabaseCacheSize => cmd.database_cache_size.to_string(),
			AdditionalTrieLayer => cmd.additional_trie_layers.to_string(),
			NoMedianSlope => cmd.no_median_slopes.to_string(),
			NoMinSquare => cmd.no_min_squares.to_string(),
			NoStorageInfo => cmd.no_storage_info.to_string(),
			SaveAndContinue => String::default(),
		})
	}

	// Implementation to update the command argument when the menu option is selected.
	fn update_arguments(
		self,
		cmd: &mut PalletCmd,
		registry: &mut PalletExtrinsicsRegistry,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		match self {
			Pallets => update_pallets(cmd, cli, registry)?,
			Extrinsics => update_extrinsics(cmd, cli, registry)?,
			ExcludedPallets => update_excluded_pallets(cmd, cli, registry)?,
			Runtime => cmd.runtime = Some(ensure_runtime_binary_exists(cli, &Profile::Release)?),
			GenesisBuilderPolicy => update_genesis_builder_policy(cmd, cli).map(|_| ())?,
			GenesisBuilderPreset => update_genesis_preset(cmd, cli)?,
			Steps => cmd.steps = self.input_parameter(cmd, cli, true)?.parse(),
			Repeat => cmd.repeat = self.input_parameter(cmd, cli, true)?.parse(),
			High => cmd.highest_range_values = self.input_range_values(cmd, cli, true)?.parse(),
			Low => cmd.lowest_range_values = self.input_range_values(cmd, cli, true)?.parse(),
			MapSize => cmd.worst_case_map_values = self.input_parameter(cmd, cli, true)?.parse(),
			DatabaseCacheSize =>
				cmd.database_cache_size = self.input_parameter(cmd, cli, true)?.parse(),
			AdditionalTrieLayer =>
				cmd.additional_trie_layers = self.input_parameter(cmd, cli, true)?.parse(),
			NoMedianSlope => cmd.no_median_slopes = self.confirm(cmd, cli)?,
			NoMinSquare => cmd.no_min_squares = self.confirm(cmd, cli)?,
			NoStorageInfo => cmd.no_storage_info = self.confirm(cmd, cli)?,
			SaveAndContinue => return Ok(true),
		};
		Ok(false)
	}

	fn input_parameter(
		self,
		cmd: &PalletCmd,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<String> {
		let default_value = self.read_command(cmd)?;
		let prompt_message = format!(
			r#"Provide value to the parameter "{}""#,
			self.get_message().unwrap_or_default()
		);
		cli.input(prompt_message)
			.required(is_required)
			.placeholder(&default_value)
			.default_input(&default_value)
			.interact()
			.map(|v| v.trim().to_string())
			.map_err(anyhow::Error::from)
	}

	fn input_range_values(
		self,
		cmd: &PalletCmd,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<Vec<u32>> {
		let values = self.input_array(
			cmd,
			&format!(
				r#"Provide range values to the parameter "{}" (numbers separated by commas)"#,
				self.get_message().unwrap_or_default()
			),
			cli,
			is_required,
		)?;

		let mut parsed_inputs = vec![];
		for value in values {
			parsed_inputs.push(value.parse()?);
		}
		Ok(parsed_inputs)
	}

	fn input_array(
		self,
		cmd: &PalletCmd,
		label: &str,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<Vec<String>> {
		let default_value = self.read_command(cmd)?;
		let input = cli
			.input(label)
			.required(is_required)
			.placeholder(&default_value)
			.default_input(&default_value)
			.interact()
			.map(|v| v.trim().to_string())
			.map_err(anyhow::Error::from)?;

		let mut parsed_inputs = vec![];
		for value in input.split(",") {
			parsed_inputs.push(value.to_string());
		}
		Ok(parsed_inputs)
	}

	fn confirm(self, cmd: &PalletCmd, cli: &mut impl cli::traits::Cli) -> anyhow::Result<bool> {
		let default_value = self.read_command(cmd)?;
		let parsed_default_value = default_value.trim().parse().unwrap();
		cli.confirm(format!(
			r#"Do you want to enable "{}"?"#,
			self.get_message().unwrap_or_default()
		))
		.initial_value(parsed_default_value)
		.interact()
		.map_err(anyhow::Error::from)
	}

	fn get_range_values<T: ToString>(self, range_values: &[T]) -> String {
		if range_values.is_empty() {
			return "None".to_string();
		}
		range_values.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
	}

	fn get_joined_string(self, s: &String) -> String {
		if s == &"*".to_string() || s.is_empty() {
			return "All selected".to_string()
		}
		s.clone()
	}
}

fn update_pallets(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	registry: &mut PalletExtrinsicsRegistry,
) -> anyhow::Result<()> {
	fetch_pallet_registry(cmd, registry)?;
	cmd.pallet = Some(guide_user_to_select_pallets(&registry, &cmd.exclude_pallets, cli, true)?);
	Ok(())
}

fn update_extrinsics(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	registry: &mut PalletExtrinsicsRegistry,
) -> anyhow::Result<()> {
	fetch_pallet_registry(cmd, registry)?;
	// Not allow selecting extrinsics when multiple pallets are selected.
	let pallet_count = cmd.pallet.as_deref().expect("No pallet provided").matches(",").count();
	cmd.extrinsic = Some(match pallet_count {
		0 => guide_user_to_select_extrinsics(cmd, &registry, cli)?,
		_ => ALL_SELECTED.to_string(),
	});
	Ok(())
}

fn update_excluded_pallets(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	registry: &mut PalletExtrinsicsRegistry,
) -> anyhow::Result<()> {
	fetch_pallet_registry(cmd, registry)?;
	let pallets = guide_user_to_select_pallets(&registry, &vec![], cli, false)?;
	cmd.exclude_pallets =
		pallets.split(',').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
	Ok(())
}

fn update_genesis_builder_policy(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	let policy = guide_user_to_select_genesis_policy(cli)?;
	cmd.genesis_builder = parse_genesis_builder_policy(policy)?.genesis_builder;
	Ok(policy.to_string())
}

fn fetch_pallet_registry(
	cmd: &PalletCmd,
	registry: &mut PalletExtrinsicsRegistry,
) -> anyhow::Result<()> {
	if registry.is_empty() {
		let spinner = spinner();
		spinner.start("Fetching pallets and extrinsics from your runtime...");
		let runtime_path = cmd.runtime.as_ref().expect("No runtime found");
		log::set_max_level(LevelFilter::Off);
		let loaded_registry = load_pallet_extrinsics(&runtime_path)?;
		log::set_max_level(LevelFilter::Info);
		*registry = loaded_registry;
		spinner.clear();
	}
	Ok(())
}

fn update_genesis_preset(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	let runtime_path = cmd.runtime.as_ref().expect("No runtime found");
	cmd.genesis_builder_preset =
		guide_user_to_select_genesis_preset(cli, &runtime_path, &cmd.genesis_builder_preset)?;
	Ok(())
}

// Locate runtime WASM binary. If it doesn't exist, trigger build.
fn ensure_runtime_binary_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let runtime_path = guide_user_to_select_runtime_path(cli)?;

	// Return immediately if the user has specified a path to the runtime binary.
	if runtime_path.extension() == Some(OsStr::new("wasm")) {
		return Ok(runtime_path);
	}

	match runtime_binary_path(&target_path, &runtime_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Runtime binary was not found. The runtime will be built locally.")?;
			cli.warning("NOTE: this may take some time...")?;
			build_project(&runtime_path, None, mode, vec!["runtime-benchmarks"], None)?;
			runtime_binary_path(&target_path, &runtime_path).map_err(|e| e.into())
		},
	}
}

fn guide_user_to_select_pallets(
	registry: &PalletExtrinsicsRegistry,
	excluded_pallets: &Vec<String>,
	cli: &mut impl cli::traits::Cli,
	required: bool,
) -> anyhow::Result<String> {
	// Prompt for pallet search input.
	let input = cli
		.input(r#"Search for pallets by name separated by commas. ("*" to select all)"#)
		.placeholder("nfts, assets, system")
		.required(false)
		.interact()?;

	if input.trim() == ALL_SELECTED {
		return Ok(ALL_SELECTED.to_string());
	}

	// Prompt user to select pallets.
	let pallets = search_for_pallets(registry, excluded_pallets, &input, MAX_PALLET_LIMIT);
	let mut prompt = cli.multiselect("Select the pallets:").required(required);
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?.join(", "))
}

fn guide_user_to_select_extrinsics(
	cmd: &mut PalletCmd,
	registry: &PalletExtrinsicsRegistry,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	let pallets = cmd.pallet.as_ref().expect("No pallet provided").split(",");

	// Prompt for extrinsic search input.
	let input = cli
		.input(r#"Search for extrinsics by name separated by commas. ("*" to select all)"#)
		.placeholder("transfer, mint, burn")
		.required(false)
		.interact()?;

	if input.trim() == ALL_SELECTED {
		return Ok(ALL_SELECTED.to_string());
	}

	// Prompt user to select extrinsics.
	let extrinsics = search_for_extrinsics(
		registry,
		pallets.map(String::from).collect(),
		&input,
		MAX_EXTRINSIC_LIMIT,
	);
	let mut prompt = cli.multiselect("Select the extrinsics to benchmark:").required(true);
	for extrinsic in extrinsics {
		prompt = prompt.item(extrinsic.clone(), &extrinsic, "");
	}
	Ok(prompt.interact()?.join(","))
}

fn guide_user_to_select_runtime_path(cli: &mut impl cli::traits::Cli) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let mut project_path = get_runtime_path(&cwd).or_else(|_| {
		cli.warning(format!(
			r#"No runtime folder found at {:?}. Please input the runtime path manually."#,
			cwd
		))?;
		guide_user_to_input_runtime_path(cli)
	})?;

	// If there is no TOML file exist, list all directories in the "runtime" folder and prompt the
	// user to select a runtime.
	if project_path.is_dir() && !project_path.join("Cargo.toml").exists() {
		let runtime = guide_user_to_select_runtime(&project_path, cli)?;
		project_path = project_path.join(runtime);
	}
	Ok(project_path)
}

fn guide_user_to_input_runtime_path(cli: &mut impl cli::traits::Cli) -> anyhow::Result<PathBuf> {
	let input = cli
		.input("Please provide the path to the runtime or parachain project.")
		.required(true)
		.default_input("./runtime")
		.placeholder("./runtime")
		.interact()
		.map(PathBuf::from)
		.map_err(anyhow::Error::from)?;
	input.canonicalize().map_err(anyhow::Error::from)
}

fn guide_user_to_configure_genesis(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	let runtime_path = cmd.runtime.as_ref().expect("No runtime found.");
	let preset_names = get_preset_names(runtime_path)?;
	let policy = match preset_names.is_empty() {
		true => GENESIS_BUILDER_NO_POLICY,
		false => guide_user_to_select_genesis_policy(cli)?,
	};

	let parsed_policy = parse_genesis_builder_policy(policy)?;
	cmd.genesis_builder = parsed_policy.genesis_builder;
	if policy == GENESIS_BUILDER_RUNTIME_POLICY {
		cmd.genesis_builder_preset =
			guide_user_to_select_genesis_preset(cli, runtime_path, &cmd.genesis_builder_preset)?;
	}
	Ok(())
}

fn guide_user_to_select_runtime(
	project_path: &PathBuf,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<PathBuf> {
	let runtimes = fs::read_dir(project_path).expect("No project found");
	let mut prompt = cli.select("Select the runtime:");
	for runtime in runtimes {
		let path = runtime.unwrap().path();
		let manifest = from_path(Some(path.as_path()))?;
		let package = manifest.package();
		let name = package.clone().name;
		let description = package.description().unwrap_or_default().to_string();
		prompt = prompt.item(path, &name, &description);
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_genesis_policy(cli: &mut impl cli::traits::Cli) -> anyhow::Result<&str> {
	let mut prompt = cli
		.select("Select the genesis builder policy:")
		.initial_value(GENESIS_BUILDER_NO_POLICY);
	for (policy, description) in [
		(GENESIS_BUILDER_NO_POLICY, "Do not provide any genesis state"),
		(
			GENESIS_BUILDER_RUNTIME_POLICY,
			"Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API",
		),
	] {
		prompt = prompt.item(policy, policy, description);
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_genesis_preset(
	cli: &mut impl cli::traits::Cli,
	runtime_path: &PathBuf,
	default_value: &str,
) -> anyhow::Result<String> {
	let spinner = cliclack::spinner();
	spinner.start("Fetching available genesis builder presets of your runtime...");
	let mut prompt = cli
		.select("Select the genesis builder preset:")
		.initial_value(default_value.to_string());
	let preset_names = get_preset_names(runtime_path)?;
	if preset_names.is_empty() {
		return Err(anyhow::anyhow!("No preset found for the runtime"));
	}
	spinner.stop(format!("Found {} genesis builder presets", preset_names.len()));
	for preset in preset_names {
		prompt = prompt.item(preset.to_string(), preset, "");
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_menu_option(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<BenchmarkPalletMenuOption> {
	let mut prompt = cli.select("Select the parameter to update:");

	let mut index = 0;
	for param in BenchmarkPalletMenuOption::iter() {
		if param.is_disabled(cmd)? {
			continue;
		}
		let label = param.get_message().unwrap_or_default();
		let hint = param.get_documentation().unwrap_or_default();
		let formatted_label = match param {
			BenchmarkPalletMenuOption::SaveAndContinue => label,
			_ => &format!("({index}) - {label}: {}", param.read_command(cmd)?),
		};
		prompt = prompt.item(param, formatted_label, hint);
		index += 1;
	}
	Ok(prompt.interact()?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use clap::Parser;
	use duct::cmd;
	use tempfile::tempdir;

	#[test]
	fn benchmark_pallet_works() -> anyhow::Result<()> {
		let mut cli =
			expect_select_genesis_policy(expect_pallet_benchmarking_intro(MockCli::new()), 0)
				.expect_warning("NOTE: this may take some time...")
				.expect_outro("Benchmark completed successfully!");

		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(true).to_str().unwrap(),
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;
		BenchmarkPalletArgs { command: cmd, skip_menu: true }.execute(&mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn benchmark_pallet_with_chainspec_fails() -> anyhow::Result<()> {
		let spec = "path-to-chainspec";
		let mut cli =
			expect_pallet_benchmarking_intro(MockCli::new()).expect_outro_cancel(format!(
				"Chain specs are not supported. Please remove `--chain={spec}` \
			          and use `--runtime=<PATH>` instead"
			));

		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--chain",
			spec,
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;

		BenchmarkPalletArgs { command: cmd, skip_menu: true }.execute(&mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn benchmark_pallet_without_runtime_benchmarks_feature_fails() -> anyhow::Result<()> {
		let mut cli = 	expect_select_genesis_policy(expect_pallet_benchmarking_intro(MockCli::new()), 0)
			.expect_outro_cancel(
		        "Failed to run benchmarking: Invalid input: Could not call runtime API to Did not find the benchmarking metadata. \
		        This could mean that you either did not build the node correctly with the `--features runtime-benchmarks` flag, \
				or the chain spec that you are using was not created by a node that was compiled with the flag: \
				Other: Exported method Benchmark_benchmark_metadata is not found"
			);
		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(false).to_str().unwrap(),
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;
		BenchmarkPalletArgs { command: cmd, skip_menu: true }.execute(&mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn benchmark_pallet_fails_with_error() -> anyhow::Result<()> {
		let mut cli =  expect_select_genesis_policy(expect_pallet_benchmarking_intro(MockCli::new()), 0)
			.expect_outro_cancel("Failed to run benchmarking: Invalid input: No benchmarks found which match your input.");
		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(true).to_str().unwrap(),
			"--pallet",
			"unknown-pallet-name",
			"--extrinsic",
			"",
		])?;
		BenchmarkPalletArgs { command: cmd, skip_menu: true }.execute(&mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_select_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = temp_dir.path().join("runtime");
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let mut cli = MockCli::new().expect_select(
			"Select the runtime:",
			Some(true),
			true,
			Some(runtimes.map(|runtime| (runtime.to_string(), "".to_string())).to_vec()),
			0,
		);
		fs::create_dir(&runtime_path)?;
		for runtime in runtimes {
			cmd("cargo", ["new", runtime, "--bin"]).dir(&runtime_path).run()?;
		}
		guide_user_to_select_runtime(&runtime_path, &mut cli)?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_configure_genesis_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime_path(false);
		let mut cli = expect_select_genesis_preset(
			expect_select_genesis_policy(MockCli::new(), 1),
			&runtime_path,
			0,
		);
		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			runtime_path.to_str().unwrap(),
			"--pallet",
			"",
			"--extrinsic",
			"",
		])?;
		guide_user_to_configure_genesis(&mut cmd, &mut cli)?;
		assert_eq!(cmd.genesis_builder, parse_genesis_builder_policy("runtime")?.genesis_builder);
		assert_eq!(
			cmd.genesis_builder_preset,
			get_preset_names(&runtime_path)?.first().cloned().unwrap_or_default()
		);
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_policy_works() -> anyhow::Result<()> {
		// Select genesis builder policy `none`.
		let mut cli = expect_select_genesis_policy(MockCli::new(), 0);
		guide_user_to_select_genesis_policy(&mut cli)?;
		cli.verify()?;

		// Select genesis builder policy `runtime`.
		let runtime_path = get_mock_runtime_path(false);
		cli = expect_select_genesis_preset(
			expect_select_genesis_policy(MockCli::new(), 1),
			&runtime_path,
			0,
		);
		guide_user_to_select_genesis_policy(&mut cli)?;
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	#[test]
	fn guide_user_to_select_genesis_preset_works() -> anyhow::Result<()> {
		let runtime_path = get_mock_runtime_path(false);
		let mut cli = expect_select_genesis_preset(MockCli::new(), &runtime_path, 0);
		guide_user_to_select_genesis_preset(&mut cli, &runtime_path, "development")?;
		cli.verify()
	}

	fn expect_pallet_benchmarking_intro(cli: MockCli) -> MockCli {
		cli.expect_intro("Benchmarking your pallets").expect_warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)
	}

	fn expect_select_genesis_policy(cli: MockCli, item: usize) -> MockCli {
		let policies = vec![
			(GENESIS_BUILDER_NO_POLICY.to_string(), "Do not provide any genesis state".to_string()),
			(
				GENESIS_BUILDER_RUNTIME_POLICY.to_string(),
				"Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API"
					.to_string(),
			),
		];
		cli.expect_select(
			"Select the genesis builder policy:",
			Some(true),
			true,
			Some(policies),
			item,
		)
	}

	fn expect_select_genesis_preset(cli: MockCli, runtime_path: &PathBuf, item: usize) -> MockCli {
		let preset_names = get_preset_names(runtime_path)
			.unwrap()
			.into_iter()
			.map(|preset| (preset, String::default()))
			.collect();
		cli.expect_select(
			"Select the genesis builder preset:",
			Some(true),
			true,
			Some(preset_names),
			item,
		)
	}

	// Construct the path to the mock runtime WASM file.
	fn get_mock_runtime_path(with_benchmark_features: bool) -> std::path::PathBuf {
		let path = format!(
			"../../tests/runtimes/{}.wasm",
			if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
		);
		current_dir().unwrap().join(path).canonicalize().unwrap()
	}
}
