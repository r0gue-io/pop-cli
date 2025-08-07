// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self},
	common::{
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		prompt::display_message,
		runtime::Feature,
	},
};
use clap::Args;
use pop_chains::{bench::BlockCmd, generate_binary_benchmarks, BenchmarkingCliCommand};
use pop_common::Profile;
use std::{
	env::current_dir,
	path::{Path, PathBuf},
};

const EXCLUDED_ARGS: [&str; 1] = ["--profile"];

#[derive(Args)]
pub(crate) struct BenchmarkBlock {
	/// Command to benchmark the execution time of historic blocks.
	#[clap(flatten)]
	pub command: BlockCmd,
	/// Build profile.
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
}

impl BenchmarkBlock {
	pub(crate) fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.benchmark(cli, &current_dir().unwrap_or(PathBuf::from("./")))
	}

	fn benchmark(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		target_path: &Path,
	) -> anyhow::Result<()> {
		cli.intro("Benchmarking the execution time of historic blocks")?;

		if self.profile.is_none() {
			self.profile = Some(guide_user_to_select_profile(cli)?);
		};
		let binary_path = ensure_node_binary_exists(
			cli,
			target_path,
			self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
			vec![Feature::Benchmark.as_ref()],
		)?;

		cli.warning("NOTE: this may take some time...")?;

		let result = generate_binary_benchmarks(
			&binary_path,
			BenchmarkingCliCommand::Block,
			|args| args,
			&EXCLUDED_ARGS,
		);

		// Display the benchmarking command.
		cliclack::log::remark("\n")?;
		cli.info(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut args = vec!["pop bench block".to_string()];
		let mut arguments: Vec<String> = std::env::args().skip(3).collect();
		#[cfg(test)]
		{
			arguments.retain(|arg| arg != "--show-output" && arg != "--nocapture");
		}
		if !argument_exists(&arguments, "--profile") {
			if let Some(ref profile) = self.profile {
				arguments.push(format!("--profile={}", profile));
			}
		}
		args.extend(arguments);
		args.join(" ")
	}
}

fn argument_exists(args: &[String], arg: &str) -> bool {
	args.iter().any(|a| a.contains(arg))
}

#[cfg(test)]
mod tests {
	use crate::cli::MockCli;
	use clap::Parser;
	use duct::cmd;
	use pop_common::Profile;
	use std::fs::{self, File};
	use tempfile::tempdir;

	use super::*;

	#[test]
	fn benchmark_block_works() -> anyhow::Result<()> {
		let name = "node";
		let temp_dir = tempdir()?;
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let target_path = Profile::Debug.target_directory(temp_dir.path());

		fs::create_dir(&temp_dir.path().join("target"))?;
		fs::create_dir(&target_path)?;
		File::create(target_path.join("node"))?;

		// With `profile` provided.
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution time of historic blocks")
			.expect_warning("NOTE: this may take some time...")
			.expect_info("pop bench block --profile=debug")
			.expect_outro_cancel(
				// As we only mock the node to test the interactive flow, the returned error is
				// expected.
				"Failed to run benchmarking: Permission denied (os error 13)",
			);
		BenchmarkBlock {
			command: BlockCmd::try_parse_from(vec!["", "--from=0", "--to=1"])?,
			profile: Some(Profile::Debug),
		}
		.benchmark(&mut cli, temp_dir.path())?;
		cli.verify()?;

		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the execution time of historic blocks")
			.expect_select(
				"Choose the build profile of the binary that should be used: ",
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info("pop bench block --profile=debug")
			.expect_outro_cancel(
				// As we only mock the node to test the interactive flow, the returned error is
				// expected.
				"Failed to run benchmarking: Permission denied (os error 13)",
			);
		BenchmarkBlock {
			command: BlockCmd::try_parse_from(vec!["", "--from=0", "--to=1"])?,
			profile: None,
		}
		.benchmark(&mut cli, temp_dir.path())?;
		cli.verify()
	}
}
