// SPDX-License-Identifier: GPL-3.0

use std::{env::current_dir, path::PathBuf};

use crate::{
	cli::{self, traits::Cli},
	common::prompt::display_message,
};
use clap::{Args, Subcommand};
use cliclack::spinner;
use frame_benchmarking_cli::PalletCmd;
use pop_common::Profile;
use pop_parachains::{build_project, generate_benchmarks, runtime_binary_path};

/// Arguments for bencharmking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
	/// Directory path for your runtime [default: "runtime"]
	#[clap(alias = "r", short, long, default_value = "runtime")]
	runtime_path: PathBuf,
}

/// Benchmark a pallet or parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the extrinsic weight of FRAME Pallets
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Pallet(PalletCmd),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;

		let result = match args.command {
			Command::Pallet(mut cmd) => Command::bechmark_pallet(&mut cmd, &mut cli),
		};
		match result {
			Ok(()) => display_message("Benchmark completed successfully!", true, &mut cli),
			Err(e) => display_message(&e.to_string(), false, &mut cli),
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
		// No runtime path provided, auto-detect the runtime WASM binary. If not found, build the
		// runtime.
		if cmd.runtime.is_none() {
			cmd.runtime = Some(ensure_wasm_blob_exists(cli, &Profile::Release)?);
		}

		cli.warning("NOTE: this may take some time...")?;

		let spinner = spinner();
		spinner.start("Benchmarking and generating weight file....");

		if let Err(e) = generate_benchmarks(&cmd) {
			return display_message(&e.to_string(), false, cli);
		}

		if let Some(ref output_path) = cmd.output {
			console::Term::stderr().clear_last_lines(1)?;
			cli.info(format!(
				"Weight file is generated to {}",
				output_path.as_path().display().to_string()
			))?;
		}
		Ok(())
	}
}

// Locate runtime WASM binary, if it doesn't exist trigger build.
fn ensure_wasm_blob_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let target_path = mode.target_directory(&cwd).join("wbuild");
	let project_path = cwd.join("runtime");
	match runtime_binary_path(&target_path, &project_path) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Runtime was not found. The runtime will be built locally.".to_string())?;
			cli.warning("NOTE: this may take some time...")?;
			build_project(&project_path, None, mode, vec!["runtime-benchmarks"], None)?;
			runtime_binary_path(&target_path, &project_path).map_err(|e| e.into())
		},
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::cli::MockCli;
	use clap::Parser;
	use std::env;

	#[test]
	fn benchmark_pallet_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking your pallets")
			.expect_warning(
				"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
			)
			.expect_warning("NOTE: this may take some time...");

		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path().to_str().unwrap(),
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
		let expected = format!(
			"Chain specs are not supported. Please remove `--chain={spec}` \
		          and use `--runtime=<PATH>` instead"
		);
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking your pallets")
			.expect_warning(
				"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
			)
			.expect_outro_cancel(expected.clone());

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
	fn benchmark_pallet_fails_with_error() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking your pallets")
			.expect_warning(
				"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
			)
			.expect_outro_cancel(format!(
				"Failed to run benchmarking: Invalid input: No benchmarks found which match your input."
			));

		let mut cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path().to_str().unwrap(),
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
	fn benchmark_pallet_detects_runtime_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking your pallets")
			.expect_warning(
				"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
			)
			.expect_outro_cancel(format!(
				"Failed to run benchmarking: Invalid input: No benchmarks found which match your input."
			));

		let mut cmd =
			PalletCmd::try_parse_from(&["", "--pallet", "unknown-pallet-name", "--extrinsic", ""])?;

		Command::bechmark_pallet(&mut cmd, &mut cli)?;
		cli.verify()?;
		Ok(())
	}

	// Construct the path to the mock runtime WASM file.
	fn get_mock_runtime_path() -> std::path::PathBuf {
		env::current_dir().unwrap().join("tests/files/base_parachain.wasm")
	}
}
