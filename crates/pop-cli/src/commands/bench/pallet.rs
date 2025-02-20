// SPDX-License-Identifier: GPL-3.0

use super::display_message;
use crate::cli::{
	self,
	traits::{Input, MultiSelect, Select},
};
use clap::Args;
use cliclack::{spinner, ProgressBar};
use frame_benchmarking_cli::PalletCmd;
use log::LevelFilter;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_project, check_preset, get_runtime_path, list_pallets_and_extrinsics,
	parse_genesis_builder_policy, run_pallet_benchmarking, runtime_binary_path,
	search_for_extrinsics, search_for_pallets, PalletExtrinsicsCollection,
};
use std::{collections::HashMap, env::current_dir, fs, path::PathBuf};
use strum::{EnumIs, EnumMessage, IntoEnumIterator};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};

const ALL_SELECTED: &str = "*";
const GENESIS_CONFIG_NO_POLICY: &str = "none";
const GENESIS_CONFIG_RUNTIME_POLICY: &str = "runtime";
const MAX_EXTRINSIC_LIMIT: usize = 10;
const MAX_PALLET_LIMIT: usize = 20;

#[derive(Args)]
pub(crate) struct BenchmarkPalletArgs {
	#[command(flatten)]
	pub command: PalletCmd,

	/// If this is set to true, no parameter menu pops up.
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
		let mut pallet_extrinsics: PalletExtrinsicsCollection = HashMap::default();
		let spinner = spinner();
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
			let policy = update_genesis_builder_policy(cmd, cli)?;
			if policy == GENESIS_CONFIG_RUNTIME_POLICY {
				if let Err(e) = update_genesis_preset(cmd, cli, &spinner) {
					return display_message(&e.to_string(), false, cli);
				};
			};
		}
		// No pallet provided, prompts user to select the pallets fetched from runtime.
		if cmd.pallet.is_none() {
			update_pallets(cmd, cli, &mut pallet_extrinsics, &spinner)?;
		}
		// No extrinsic provided, prompts user to select the extrinsics fetched from runtime.
		if cmd.extrinsic.is_none() {
			update_extrinsics(cmd, cli, &mut pallet_extrinsics, &spinner)?;
		}

		// Only prompt user to update parameters when `skip_menu` is not provided.
		if !self.skip_menu {
			loop {
				let option = guide_user_to_select_menu_option(cmd, cli)?;
				match option.update(cmd, &mut pallet_extrinsics, cli, &spinner) {
					Ok(true) => break,
					Ok(false) => continue,
					Err(e) => cli.info(&e.to_string())?,
				}
			}
		}

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;
		if let Err(e) = run_pallet_benchmarking(cmd) {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}
}

#[derive(Debug, EnumIter, EnumIs, EnumMessageDerive, Eq, PartialEq, Copy, Clone)]
pub(crate) enum BenchmarkPalletMenuOption {
	// Example documentation.
	#[strum(message = "Additional trie layer")]
	AdditionalTrieLayer,
	// Example documentation.
	#[strum(message = "Extrinsics")]
	Extrinsics,
	// Example documentation.
	#[strum(message = "Genesis builder policy")]
	GenesisBuilder,
	// Example documentation.
	#[strum(message = "High")]
	High,
	// Example documentation.
	#[strum(message = "Low")]
	Low,
	// Example documentation.
	#[strum(message = "Map size")]
	MapSize,
	// Example documentation.
	#[strum(message = "Pallets")]
	Pallets,
	// Example documentation.
	#[strum(message = "Repeats")]
	Repeat,
	// Example documentation.
	#[strum(message = "Runtime path")]
	Runtime,
	// Example documentation.
	#[strum(message = "Genesis config preset")]
	GenesisConfigPreset,
	// Example documentation.
	#[strum(message = "Steps")]
	Steps,
	#[strum(message = "> Save all parameter changes and continue")]
	SaveAndContinue,
}

impl BenchmarkPalletMenuOption {
	pub fn read_command(self, cmd: &PalletCmd) -> anyhow::Result<String> {
		use BenchmarkPalletMenuOption::*;
		Ok(match self {
			Steps => cmd.steps.to_string(),
			Repeat => cmd.repeat.to_string(),
			MapSize => cmd.worst_case_map_values.to_string(),
			Low => self.get_range_values(&cmd.lowest_range_values),
			High => self.get_range_values(&cmd.highest_range_values),
			AdditionalTrieLayer => cmd.additional_trie_layers.to_string(),
			Pallets => self.get_joined_string(cmd.pallet.as_ref().expect("No pallet provided")),
			Extrinsics =>
				self.get_joined_string(cmd.extrinsic.as_ref().expect("No extrinsic provided")),
			GenesisConfigPreset => cmd.genesis_builder_preset.clone(),
			GenesisBuilder =>
				serde_json::to_string(&cmd.genesis_builder.expect("No chainspec provided"))
					.expect("Failed to serialize genesis builder policy"),
			Runtime => cmd
				.runtime
				.as_ref()
				.expect("No runtime provided")
				.as_path()
				.to_str()
				.unwrap()
				.to_string(),
			SaveAndContinue => String::default(),
		})
	}

	fn update(
		self,
		cmd: &mut PalletCmd,
		pallet_extrinsics: &mut PalletExtrinsicsCollection,
		cli: &mut impl cli::traits::Cli,
		spinner: &ProgressBar,
	) -> anyhow::Result<bool> {
		use BenchmarkPalletMenuOption::*;
		match self {
			GenesisBuilder => update_genesis_builder_policy(cmd, cli).map(|_| ())?,
			GenesisConfigPreset =>
				cmd.genesis_builder_preset =
					guide_user_to_input_genesis_preset(cli, &cmd.genesis_builder_preset)?,
			Pallets => update_pallets(cmd, cli, pallet_extrinsics, &spinner)?,
			Extrinsics => update_extrinsics(cmd, cli, pallet_extrinsics, &spinner)?,
			Steps => cmd.steps = self.input_parameter(cmd, cli, true)?.parse()?,
			Repeat => cmd.repeat = self.input_parameter(cmd, cli, true)?.parse()?,
			AdditionalTrieLayer =>
				cmd.additional_trie_layers = self.input_parameter(cmd, cli, true)?.parse()?,
			MapSize => cmd.worst_case_map_values = self.input_parameter(cmd, cli, true)?.parse()?,
			High => cmd.highest_range_values = self.input_range_values(cmd, cli, true)?,
			Low => cmd.lowest_range_values = self.input_range_values(cmd, cli, true)?,
			Runtime => cmd.runtime = Some(guide_user_to_select_runtime_path(cli)?),
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
		cli.input(format!(
			r#"Provide value to the parameter "{}""#,
			self.get_message().unwrap_or_default()
		))
		.required(is_required)
		.placeholder(&default_value)
		.default_input(&default_value)
		.interact()
		.map(|v| v.trim().to_string())
		.map_err(|e| anyhow::anyhow!(e.to_string()))
	}

	fn input_range_values(
		self,
		cmd: &PalletCmd,
		cli: &mut impl cli::traits::Cli,
		is_required: bool,
	) -> anyhow::Result<Vec<u32>> {
		let default_value = self.read_command(cmd)?;
		let input = cli
			.input(format!(
				r#"Provide range values to the parameter "{}" (number separated by commas)"#,
				self.get_message().unwrap_or_default()
			))
			.required(is_required)
			.placeholder(&default_value)
			.default_input(&default_value)
			.interact()
			.map(|v| v.trim().to_string())
			.map_err(anyhow::Error::from)?;
		let mut parsed_inputs = vec![];
		for num in input.split(",") {
			parsed_inputs.push(num.parse()?);
		}
		Ok(parsed_inputs)
	}

	fn get_range_values<T: ToString>(self, range_values: &[T]) -> String {
		if range_values.is_empty() {
			return "None".to_string();
		}
		range_values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
	}

	fn get_joined_string(self, s: &String) -> String {
		if s == &"*".to_string() || s.is_empty() {
			"All selected".to_string()
		} else {
			let count = s.split(",").collect::<Vec<&str>>().len();
			format!("{count} selected")
		}
	}
}

pub fn update_pallets(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	pallet_extrinsics: &mut PalletExtrinsicsCollection,
	spinner: &ProgressBar,
) -> anyhow::Result<()> {
	fetch_pallet_extrinsics_if_not_exist(cmd, pallet_extrinsics, &spinner)?;
	cmd.pallet = Some(guide_user_to_select_pallets(&pallet_extrinsics, cli)?);
	Ok(())
}

pub fn update_extrinsics(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	pallet_extrinsics: &mut PalletExtrinsicsCollection,
	spinner: &ProgressBar,
) -> anyhow::Result<()> {
	fetch_pallet_extrinsics_if_not_exist(cmd, pallet_extrinsics, &spinner)?;
	// Not allow selecting extrinsics when multiple pallets are selected.
	let pallet_count = cmd.pallet.as_deref().unwrap_or_default().matches(",").count();
	cmd.extrinsic = Some(match pallet_count {
		0 => guide_user_to_select_extrinsics(cmd, &pallet_extrinsics, cli)?,
		_ => ALL_SELECTED.to_string(),
	});
	Ok(())
}

pub fn update_genesis_builder_policy(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<String> {
	let policy = guide_user_to_select_genesis_builder(cli)?;
	cmd.genesis_builder = parse_genesis_builder_policy(policy)?.genesis_builder;
	Ok(policy.to_string())
}

pub fn fetch_pallet_extrinsics_if_not_exist(
	cmd: &PalletCmd,
	pallet_extrinsics: &mut PalletExtrinsicsCollection,
	spinner: &ProgressBar,
) -> anyhow::Result<()> {
	if pallet_extrinsics.is_empty() {
		spinner.start("Fetching pallets and extrinsics from your runtime...");
		let runtime_path = cmd.runtime.clone().expect("No runtime found.");
		log::set_max_level(LevelFilter::Off);
		let fetched_extrinsics = list_pallets_and_extrinsics(&runtime_path)?;
		*pallet_extrinsics = fetched_extrinsics;
		log::set_max_level(LevelFilter::Info);
		spinner.clear();
	}
	Ok(())
}

fn update_genesis_preset(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	spinner: &ProgressBar,
) -> anyhow::Result<()> {
	let preset_input = guide_user_to_input_genesis_preset(cli, &cmd.genesis_builder_preset)?;
	let runtime_path = cmd.runtime.as_ref().expect("No runtime found");
	let preset = (!preset_input.is_empty()).then_some(&preset_input);
	spinner.start("Verifying genesis config preset...");
	check_preset(runtime_path, preset)?;
	spinner.clear();
	cmd.genesis_builder_preset = preset_input;
	Ok(())
}

// Locate runtime WASM binary. If it doesn't exist, trigger build.
fn ensure_runtime_binary_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let project_path = guide_user_to_select_runtime_path(cli)?;

	match runtime_binary_path(&target_path, &project_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Runtime binary was not found. The runtime will be built locally.")?;
			cli.warning("NOTE: this may take some time...")?;
			build_project(&project_path, None, mode, vec!["runtime-benchmarks"], None)?;
			runtime_binary_path(&target_path, &project_path).map_err(|e| e.into())
		},
	}
}

fn guide_user_to_select_pallets(
	pallet_extrinsics: &PalletExtrinsicsCollection,
	cli: &mut impl cli::traits::Cli,
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
	let pallets = search_for_pallets(pallet_extrinsics, &input, MAX_PALLET_LIMIT);
	let mut prompt = cli.multiselect("Select the pallets to benchmark:").required(true);
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, "");
	}
	Ok(prompt.interact()?.join(","))
}

fn guide_user_to_select_extrinsics(
	cmd: &mut PalletCmd,
	pallet_extrinsics: &PalletExtrinsicsCollection,
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
		pallet_extrinsics,
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
	if !project_path.join("Cargo.toml").exists() {
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

fn guide_user_to_select_runtime(
	project_path: &PathBuf,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<PathBuf> {
	let mut prompt = cli.select("Select the runtime:");
	let mut found_runtime = false;
	for entry in fs::read_dir(project_path)? {
		let path = entry?.path();
		let manifest = from_path(Some(&path))?;
		let package = manifest.package();
		let name = package.clone().name;
		let description = package.description().unwrap_or_default();
		prompt = prompt.item(path, &name, description);
		found_runtime = true;
	}
	if !found_runtime {
		return Err(anyhow::anyhow!("No runtime found."));
	}
	prompt.interact().map_err(Into::into)
}

fn guide_user_to_select_genesis_builder(cli: &mut impl cli::traits::Cli) -> anyhow::Result<&str> {
	let mut prompt = cli.select("Select the genesis builder policy:").initial_value("none");
	for (policy, description) in [
    	(GENESIS_CONFIG_NO_POLICY, "Do not provide any genesis state"),
    	(GENESIS_CONFIG_RUNTIME_POLICY, "Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API. \
         This will use the `development` preset by default.")
	] {
		prompt = prompt.item(policy, policy, description);
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_select_menu_option(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<BenchmarkPalletMenuOption> {
	let mut prompt = cli.select("Select the parameter to update:");
	for (index, param) in BenchmarkPalletMenuOption::iter().enumerate() {
		let label = param.get_message().unwrap_or_default();
		let hint = param.get_documentation().unwrap_or_default();
		let formatted_label = if param.is_save_and_continue() {
			label
		} else {
			let value = param.read_command(cmd)?;
			&format!("({index}) - {label}: {value}")
		};
		prompt = prompt.item(param, formatted_label, hint);
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_input_genesis_preset(
	cli: &mut impl cli::traits::Cli,
	default_value: &str,
) -> anyhow::Result<String> {
	cli.input("Provide the genesis config preset of the runtime (e.g. development, local_testnet or your custom preset name)")
	    .required(false)
		.placeholder(default_value)
		.default_input(default_value)
		.interact().map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use clap::Parser;
	use duct::cmd;
	use std::env;
	use tempfile::tempdir;

	#[test]
	fn benchmark_pallet_works() -> anyhow::Result<()> {
		let mut cli =
			expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()), 0)
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
		let mut cli = 	expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()), 0)
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
		let mut cli =  expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()), 0)
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
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_select_genesis_policy_works() -> anyhow::Result<()> {
		// Select genesis builder policy `none`.
		let mut cli = expect_select_genesis_builder(MockCli::new(), 0);
		guide_user_to_select_genesis_builder(&mut cli)?;
		cli.verify()?;

		// Select genesis builder policy `runtime`.
		cli = expect_select_genesis_builder(MockCli::new(), 1);
		guide_user_to_select_genesis_builder(&mut cli)?;
		guide_user_to_input_genesis_preset(&mut cli, "development")?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_input_genesis_preset_works() -> anyhow::Result<()> {
		let preset = String::from("development");
		let mut cli = expect_input_genesis_preset(MockCli::new(), &preset);
		guide_user_to_input_genesis_preset(&mut cli, &preset)?;
		cli.verify()?;
		Ok(())
	}

	fn expect_pallet_benchmarking_intro(cli: MockCli) -> MockCli {
		cli.expect_intro("Benchmarking your pallets").expect_warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)
	}

	fn expect_select_genesis_builder(cli: MockCli, item: usize) -> MockCli {
		let policies = vec![
 			(GENESIS_CONFIG_NO_POLICY.to_string(), "Do not provide any genesis state".to_string()),
 			(GENESIS_CONFIG_RUNTIME_POLICY.to_string(), "Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API. \
 			This will use the `development` preset by default.".to_string())
    	];
		cli.expect_select(
			"Select the genesis builder policy:",
			Some(true),
			true,
			Some(policies),
			item,
		)
	}

	fn expect_input_genesis_preset(cli: MockCli, input: &str) -> MockCli {
		cli.expect_input(
			"Provide the genesis config preset of the runtime (e.g. development, local_testnet or your custom preset name)",
			input.to_string()
		)
	}

	// Construct the path to the mock runtime WASM file.
	fn get_mock_runtime_path(with_benchmark_features: bool) -> std::path::PathBuf {
		let path = format!(
			"../../tests/runtimes/{}.wasm",
			if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
		);
		env::current_dir().unwrap().join(path).canonicalize().unwrap()
	}
}
