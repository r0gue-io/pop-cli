// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Cli},
	common::prompt::display_message,
};
use clap::{Args, Subcommand};
use frame_benchmarking_cli::PalletCmd;
use pop_parachains::generate_benchmarks;

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
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Pallet(PalletCmd),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;

		let result = match args.command {
			Command::Pallet(cmd) => Command::bechmark_pallet(cmd, &mut cli),
		};
		match result {
			Ok(()) => display_message("Benchmark completed successfully!", true, &mut cli),
			Err(e) => display_message(&e.to_string(), false, &mut cli),
		}
	}

	fn bechmark_pallet(cmd: PalletCmd, cli: &mut impl Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking your pallets")?;
		cli.warning(
			"NOTE: the `pop bench pallet` is not yet battle tested - double check the results.",
		)?;

		if let Some(spec) = cmd.shared_params.chain {
			return display_message(
				&format!(
					"Chain specs are not supported. Please remove `--chain={spec}` \
					       and use `--runtime=<PATH>` instead"
				),
				false,
				cli,
			);
		}
		cli.warning("NOTE: this may take some time...")?;

		if let Err(e) = generate_benchmarks(&cmd) {
			return display_message(&e.to_string(), false, cli);
		}

		if let Some(output_path) = cmd.output {
			console::Term::stderr().clear_last_lines(1)?;
			cli.info(format!(
				"Weight file is generated to {}",
				output_path.as_path().display().to_string()
			))?;
		}
		Ok(())
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

		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path().to_str().unwrap(),
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;
		Command::bechmark_pallet(cmd, &mut cli)?;
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

		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--chain",
			spec,
			"--pallet",
			"pallet_timestamp",
			"--extrinsic",
			"",
		])?;

		Command::bechmark_pallet(cmd, &mut cli)?;
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

		let cmd = PalletCmd::try_parse_from(&[
			"",
			"--runtime",
			get_mock_runtime_path().to_str().unwrap(),
			"--pallet",
			"unknown-pallet-name",
			"--extrinsic",
			"",
		])?;

		Command::bechmark_pallet(cmd, &mut cli)?;
		cli.verify()?;
		Ok(())
	}

	// Construct the path to the mock runtime WASM file.
	fn get_mock_runtime_path() -> std::path::PathBuf {
		env::current_dir().unwrap().join("tests/files/base_parachain.wasm")
	}
}
