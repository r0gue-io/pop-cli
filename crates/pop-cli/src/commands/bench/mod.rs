// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self,
		traits::{Cli, Select},
	},
	common::prompt::display_message,
};
use clap::{Args, Subcommand};
use cliclack::{spinner, ProgressBar};
use frame_benchmarking_cli::PalletCmd;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_project, get_preset_names, get_runtime_path, parse_genesis_builder_policy,
	run_pallet_benchmarking, runtime_binary_path,
};
use std::{env::current_dir, fs, path::PathBuf};

const GENESIS_BUILDER_NO_POLICY: &str = "none";
const GENESIS_BUILDER_RUNTIME_POLICY: &str = "runtime";

/// Arguments for benchmarking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the extrinsic weight of FRAME Pallets
	#[clap(alias = "p")]
	Pallet(PalletCmd),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;
		match args.command {
			Command::Pallet(mut cmd) => Command::bechmark_pallet(&mut cmd, &mut cli),
		}
	}

	fn bechmark_pallet(cmd: &mut PalletCmd, cli: &mut impl Cli) -> anyhow::Result<()> {
		if cmd.list.is_some() || cmd.json_output {
			if let Err(e) = run_pallet_benchmarking(cmd) {
				return display_message(&e.to_string(), false, cli);
			}
		}
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
			let policy = guide_user_to_select_genesis_builder(cli)?;
			cmd.genesis_builder = parse_genesis_builder_policy(policy)?.genesis_builder;
			if policy == GENESIS_BUILDER_RUNTIME_POLICY {
				let runtime_path = cmd.runtime.as_ref().expect("No runtime found.");
				cmd.genesis_builder_preset = guide_user_to_select_genesis_preset(
					cli,
					&spinner,
					runtime_path,
					&cmd.genesis_builder_preset,
				)?;
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

// Locate runtime WASM binary. If it doesn't exist, trigger build.
fn ensure_runtime_binary_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let mut project_path = get_runtime_path(&cwd)?;

	// If there is no TOML file exist, list all directories in the "runtime" folder and prompt the
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
	spinner: &ProgressBar,
	runtime_path: &PathBuf,
	default_value: &str,
) -> anyhow::Result<String> {
	spinner.start("Fetching available genesis builder presets of your runtime...");
	let mut prompt = cli
		.select("Select the genesis builder preset:")
		.initial_value(default_value.to_string());
	let preset_names = get_preset_names(runtime_path)?;
	if preset_names.is_empty() {
		return Err(anyhow::anyhow!("No preset found for the runtime"))
	}
	spinner.stop(format!("Found {} genesis builder presets", preset_names.len()));
	for preset in preset_names {
		prompt = prompt.item(preset.to_string(), preset, "");
	}
	Ok(prompt.interact()?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use clap::Parser;
	use cliclack::spinner;
	use duct::cmd;
	use std::env;
	use tempfile::tempdir;

	#[test]
	fn benchmark_pallet_works() -> anyhow::Result<()> {
		let mut cli =
			expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()), 0)
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
		Command::bechmark_pallet(&mut cmd, &mut cli)?;
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

		Command::bechmark_pallet(&mut cmd, &mut cli)?;
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
		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path(false).to_str().unwrap(),
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;
		Command::bechmark_pallet(&mut cmd, &mut cli)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn benchmark_pallet_fails_with_error() -> anyhow::Result<()> {
		let mut cli =  expect_select_genesis_builder(expect_pallet_benchmarking_intro(MockCli::new()), 0)
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
		Command::bechmark_pallet(&mut cmd, &mut cli)?;
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
		let spinner = spinner();
		let mut cli = expect_select_genesis_builder(MockCli::new(), 0);
		guide_user_to_select_genesis_builder(&mut cli)?;
		cli.verify()?;

		// Select genesis builder policy `runtime`.
		let runtime_path = get_mock_runtime_path(false);
		cli = expect_select_genesis_preset(
			expect_select_genesis_builder(MockCli::new(), 1),
			&runtime_path,
			0,
		);
		guide_user_to_select_genesis_builder(&mut cli)?;
		guide_user_to_select_genesis_preset(&mut cli, &spinner, &runtime_path, "development")?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_input_genesis_preset_works() -> anyhow::Result<()> {
		let spinner = spinner();
		let runtime_path = get_mock_runtime_path(false);
		let mut cli = expect_select_genesis_preset(MockCli::new(), &runtime_path, 0);
		guide_user_to_select_genesis_preset(&mut cli, &spinner, &runtime_path, "development")?;
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
           	(GENESIS_BUILDER_NO_POLICY.to_string(), "Do not provide any genesis state".to_string()),
           	(GENESIS_BUILDER_RUNTIME_POLICY.to_string(), "Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API".to_string())
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
		env::current_dir().unwrap().join(path).canonicalize().unwrap()
	}
}
