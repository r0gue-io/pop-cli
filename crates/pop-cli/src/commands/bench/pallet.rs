use cliclack::{spinner, ProgressBar};
use frame_benchmarking_cli::PalletCmd;
use log::LevelFilter;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_project, generate_benchmarks, list_pallets_and_extrinsics, parse_genesis_builder_policy,
	runtime_binary_path, search_for_extrinsics, search_for_pallets,
};
use std::{collections::HashMap, env::current_dir, fs, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};

use crate::cli::{
	self,
	traits::{Input, MultiSelect, Select},
};

use super::display_message;

#[derive(Default)]
pub(crate) struct BenchmarkPallet {
	pub genesis_builder: Option<String>,
}

impl BenchmarkPallet {
	pub fn execute(
		&mut self,
		cmd: &mut PalletCmd,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
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
		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build the
		// runtime.
		if cmd.runtime.is_none() {
			cmd.runtime = Some(ensure_runtime_binary_exists(cli, &Profile::Release)?);
		}
		// No genesis builder, prompts user to select the genesis builder policy.
		let genesis_builder_policy = if self.genesis_builder.is_none() {
			let policy = guide_user_to_select_genesis_builder(cli)?;
			cmd.genesis_builder = parse_genesis_builder_policy(policy)?.genesis_builder;
			policy.to_string()
		} else {
			"none".to_string()
		};

		// Pallet or extrinsic is not provided, prompts user to select pallets or extrinsics.
		if cmd.pallet.is_none() || cmd.extrinsic.is_none() {
			guide_user_to_select_pallets_or_extrinsics(cmd, cli, spinner)?;
		}

		guide_user_to_update_parameter(cmd, cli, genesis_builder_policy.to_string())?;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;
		if let Err(e) = generate_benchmarks(cmd) {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}
}

#[derive(Debug, EnumIter, EnumMessageDerive, Copy, Clone)]
pub(crate) enum BenchmarkPalletParameters {
	// Example documentation.
	#[strum(message = "Steps", detailed_message = "steps")]
	Steps,
	// Example documentation.
	#[strum(message = "Repeats", detailed_message = "repeat")]
	Repeat,
	// Example documentation.
	#[strum(
		message = "Map size",
		detailed_message = "",
		props(field_name = "worst_case_map_values")
	)]
	MapSize,
	// Example documentation.
	#[strum(message = "High", detailed_message = "highest_range_values")]
	High,
	// Example documentation.
	#[strum(message = "Low", detailed_message = "lowest_range_values")]
	Low,
	// Example documentation.
	#[strum(message = "Additional trie layer", detailed_message = "")]
	// Example documentation.
	AdditionalTrieLayer,
	// Example documentation.
	#[strum(message = "Pallets", detailed_message = "pallet")]
	Pallets,
	// Example documentation.
	#[strum(message = "Extrinsics", detailed_message = "extrinsics")]
	Extrinsics,
	// Example documentation.
	#[strum(message = "Genesis builder policy", detailed_message = "genesis_builder")]
	GenesisBuilder,
	// Example documentation.
	#[strum(message = "Runtime path", detailed_message = "runtime")]
	Runtime,
}

impl BenchmarkPalletParameters {
	pub fn get_value(self, cmd: &PalletCmd, genesis_builder: &String) -> anyhow::Result<String> {
		use BenchmarkPalletParameters::*;
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
			GenesisBuilder => genesis_builder.clone(),
			Runtime => cmd
				.runtime
				.as_ref()
				.expect("No runtime provided")
				.as_path()
				.to_str()
				.unwrap()
				.to_string(),
		})
	}

	fn get_range_values<T: ToString>(self, range_values: &Vec<T>) -> String {
		if range_values.is_empty() {
			return "None".to_string();
		}
		range_values.iter().map(|i| i.to_string()).collect::<String>()
	}

	fn get_joined_string(self, s: &String) -> String {
		if s == &"*".to_string() || s == &"".to_string() {
			"All selected".to_string()
		} else {
			let count = s.split(",").collect::<Vec<&str>>().len();
			format!("{count} selected")
		}
	}
}

// Locate runtime WASM binary. If it doesn't exist, trigger build.
fn ensure_runtime_binary_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let mut project_path = cwd.join("runtime");

	// If there is no TOML file exist, list all directories in the folder and prompt the
	// user to select a runtime.
	if !project_path.join("Cargo.toml").exists() {
		let runtime = guide_user_to_select_runtime(&project_path, cli)?;
		project_path = project_path.join(runtime);
	}

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

fn guide_user_to_select_pallets_or_extrinsics(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	spinner: ProgressBar,
) -> anyhow::Result<()> {
	spinner.start("Fetching pallets and extrinsics from your runtime...");
	log::set_max_level(LevelFilter::Off);
	let runtime_path = cmd.runtime.clone().unwrap();
	let pallet_extrinsics = list_pallets_and_extrinsics(&runtime_path)?;
	spinner.clear();
	let mut selected_pallets = vec![];
	if cmd.pallet.is_none() {
		selected_pallets = guide_user_to_select_pallets(cmd, &pallet_extrinsics, cli)?;
	};
	if cmd.extrinsic.is_none() {
		if selected_pallets.len() == 1 {
			guide_user_to_select_extrinsics(cmd, &pallet_extrinsics, cli)?;
		} else {
			cmd.extrinsic = Some("*".to_string());
		}
	}
	log::set_max_level(LevelFilter::Info);
	Ok(())
}

fn guide_user_to_select_pallets(
	cmd: &mut PalletCmd,
	pallet_extrinsics: &HashMap<String, Vec<String>>,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<Vec<String>> {
	// Prompt for pallet search input.
	let input = cli
		.input(r#"Search for pallets by name separated by commas. ("*" to select all)"#)
		.placeholder("nfts, assets, system")
		.required(false)
		.interact()?;

	if input == "*" {
		cmd.pallet = Some("*".to_string());
		return Ok(vec![]);
	}

	// Prompt user to select pallets.
	let pallets = search_for_pallets(&pallet_extrinsics, &input);
	let mut prompt = cli.multiselect("Select the pallets to benchmark:").required(true);
	for pallet in pallets {
		prompt = prompt.item(pallet.clone(), &pallet, &"");
	}
	let selected = prompt.interact()?;
	cmd.pallet = Some(selected.join(","));
	Ok(selected)
}

fn guide_user_to_select_extrinsics(
	cmd: &mut PalletCmd,
	pallet_extrinsics: &HashMap<String, Vec<String>>,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	let pallets = cmd.pallet.as_ref().expect("No pallet provided").split(",");

	// Prompt for extrinsic search input.
	let input = cli
		.input(r#"Search for extrinsics by name separated by commas. ("*" to select all)"#)
		.placeholder("transfer, mint, burn")
		.required(false)
		.interact()?;

	if input == "*" {
		cmd.extrinsic = Some("*".to_string());
		return Ok(());
	}

	// Prompt user to select extrinsics.
	let extrinsics =
		search_for_extrinsics(&pallet_extrinsics, pallets.map(String::from).collect(), &input);
	let mut prompt = cli.multiselect("Select the extrinsics to benchmark:").required(true);
	for extrinsic in extrinsics {
		prompt = prompt.item(extrinsic.clone(), &extrinsic, &"");
	}
	let selected = prompt.interact()?;
	cmd.extrinsic = Some(selected.join(","));
	Ok(())
}

fn guide_user_to_select_runtime(
	project_path: &PathBuf,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<PathBuf> {
	let runtimes = fs::read_dir(project_path).unwrap();
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

fn guide_user_to_select_genesis_builder(cli: &mut impl cli::traits::Cli) -> anyhow::Result<&str> {
	let mut prompt = cli.select("Select the genesis builder policy:").initial_value("none");
	for (policy, description) in [
    	("none", "Do not provide any genesis state"),
    	("runtime", "Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API. \
         This will use the `development` preset by default.")
	] {
		prompt = prompt.item(policy, policy, description);
	}
	Ok(prompt.interact()?)
}

fn guide_user_to_update_parameter(
	cmd: &mut PalletCmd,
	cli: &mut impl cli::traits::Cli,
	genesis_builder: String,
) -> anyhow::Result<usize> {
	let mut prompt = cli.select("Select the parameter to update:");
	for (index, param) in BenchmarkPalletParameters::iter().enumerate() {
		let label = param.get_message().unwrap();
		let value = param.get_value(cmd, &genesis_builder)?;
		prompt = prompt.item(
			index,
			format!("({index}) - {label} : {value}"),
			param.get_documentation().unwrap_or_default(),
		);
	}
	prompt = prompt.item(
		BenchmarkPalletParameters::iter().len(),
		"> Save all parameter changes and continue",
		"",
	);
	Ok(prompt.interact()?)
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
			expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()))
				.expect_warning("NOTE: this may take some time...")
				.expect_outro("Benchmark completed successfully!");

		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(true).to_str().unwrap(),
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;
		println!("{:?}", cmd.runtime);
		BenchmarkPallet::default().execute(&mut cmd, &mut cli)?;
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

		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--chain",
			spec,
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;

		BenchmarkPallet::default().execute(&mut cmd, &mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn benchmark_pallet_without_runtime_benchmarks_feature_fails() -> anyhow::Result<()> {
		let mut cli = 	expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()))
			.expect_outro_cancel(
			        "Failed to run benchmarking: Invalid input: Could not call runtime API to Did not find the benchmarking metadata. \
			        This could mean that you either did not build the node correctly with the `--features runtime-benchmarks` flag, \
					or the chain spec that you are using was not created by a node that was compiled with the flag: \
					Other: Exported method Benchmark_benchmark_metadata is not found"
			);
		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(false).to_str().unwrap(),
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;
		BenchmarkPallet::default().execute(&mut cmd, &mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn benchmark_pallet_fails_with_error() -> anyhow::Result<()> {
		let mut cli =  expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()))
			.expect_outro_cancel("Failed to run benchmarking: Invalid input: No benchmarks found which match your input.");
		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(true).to_str().unwrap(),
			"--pallet",
			"unknown-pallet-name",
			"--extrinsic",
			"",
		])?;
		BenchmarkPallet::default().execute(&mut cmd, &mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_select_runtime_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = temp_dir.path().join("runtime");
		let runtimes = ["runtime-1", "runtime-2", "runtime-3"];
		let mut cli = MockCli::new().expect_select(
			"Select the runtime to build:",
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
	fn guide_user_to_select_genesis_builder_works() -> anyhow::Result<()> {
		let mut cli = expect_select_genesis_builder(MockCli::new());
		guide_user_to_select_genesis_builder(&mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_select_pallets_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let runtime_path = get_mock_runtime_path(true);
		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			runtime_path.to_str().unwrap(),
			"--pallet",
			"",
			"--extrinsic",
			"",
		])?;
		let pallet_extrinsics = list_pallets_and_extrinsics(&runtime_path)?;
		guide_user_to_select_pallets(&mut cmd, &pallet_extrinsics, &mut cli)?;
		Ok(())
	}

	#[test]
	fn parse_genesis_builder_policy_works() {
		["none", "spec", "runtime"]
			.map(|policy| assert!(parse_genesis_builder_policy(policy).is_ok()));
	}

	fn expect_pallet_benchmarking_intro(cli: MockCli) -> MockCli {
		cli.expect_intro("Benchmarking your pallets").expect_warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)
	}

	fn expect_select_genesis_builder(cli: MockCli) -> MockCli {
		let policies = vec![
           	("none".to_string(), "Do not provide any genesis state".to_string()),
           	("runtime".to_string(), "Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API. \
            This will use the `development` preset by default.".to_string())
	];
		cli.expect_select("Select the genesis builder policy:", Some(true), true, Some(policies), 0)
	}

	// Construct the path to the mock runtime WASM file.
	fn get_mock_runtime_path(with_benchmark_features: bool) -> std::path::PathBuf {
		env::current_dir()
			.unwrap()
			.join(format!(
				"../../../../../tests/runtimes/{}.wasm",
				if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
			))
			.canonicalize()
			.unwrap()
	}
}
