// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self,
		traits::{Cli, Select},
	},
	common::prompt::display_message,
};
use clap::{Args, Subcommand};
use frame_benchmarking_cli::PalletCmd;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_project, parse_genesis_builder_policy, run_pallet_benchmarking, runtime_binary_path,
};
use std::{env::current_dir, fs, path::PathBuf};

/// Arguments for bencharmking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or parachain.
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
			Command::Pallet(mut cmd) => {
				if cmd.list.is_some() || cmd.json_output {
					if let Err(e) = run_pallet_benchmarking(&mut cmd) {
						return display_message(&e.to_string(), false, &mut cli);
					}
				}
				Command::bechmark_pallet(&mut cmd, &mut cli)
			},
		}
	}

	fn bechmark_pallet(cmd: &mut PalletCmd, cli: &mut impl Cli) -> anyhow::Result<()> {
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
			cmd.runtime = Some(ensure_runtime_binary_exists(cli, &Profile::Release)?);
		}
		// No genesis builder, prompts user to select the genesis builder policy.
		if cmd.genesis_builder.is_none() {
			let policy = guide_user_to_select_genesis_builder(cli)?;
			cmd.genesis_builder = parse_genesis_builder_policy(policy)?.genesis_builder;
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
	let mut project_path = cwd.join("runtime");

	// Runtime folder does not exist.
	if !project_path.exists() {
		return Err(anyhow::anyhow!("No runtime found."));
	}

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
    	("none", "Do not provide any genesis state"),
    	("runtime", "Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API. \
         This will use the `development` preset by default.")
	] {
		prompt = prompt.item(policy, policy, description);
	}
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
		Command::bechmark_pallet(&mut cmd, &mut cli)?;
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
	fn guide_user_to_select_genesis_builder_works() -> anyhow::Result<()> {
		let mut cli = expect_select_genesis_builder(MockCli::new());
		guide_user_to_select_genesis_builder(&mut cli)?;
		cli.verify()?;
		Ok(())
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
		env::current_dir().unwrap().join(format!(
			"tests/files/{}.wasm",
			if with_benchmark_features { "base_parachain_benchmark" } else { "base_parachain" }
		))
	}
}
