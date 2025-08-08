// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self},
	common::{
		bench::overwrite_weight_dir_command,
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		prompt::display_message,
		runtime::Feature::Benchmark,
	},
};
use clap::Args;
use pop_chains::{bench::StorageCmd, generate_binary_benchmarks, BenchmarkingCliCommand};
use pop_common::Profile;
use std::{
	env::current_dir,
	path::{Path, PathBuf},
};
use tempfile::tempdir;

const EXCLUDED_ARGS: [&str; 1] = ["--profile"];

#[derive(Args)]
pub(crate) struct BenchmarkStorage {
	/// Command to benchmark the storage speed of a chain snapshot.
	#[clap(flatten)]
	pub command: StorageCmd,
	/// Build profile.
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
}

impl BenchmarkStorage {
	pub(crate) fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.benchmark(cli, &current_dir().unwrap_or(PathBuf::from("./")))
	}

	fn benchmark(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		target_path: &Path,
	) -> anyhow::Result<()> {
		cli.intro("Benchmarking the storage speed of a chain snapshot")?;

		if self.profile.is_none() {
			self.profile = Some(guide_user_to_select_profile(cli)?);
		};
		let binary_path = ensure_node_binary_exists(
			cli,
			target_path,
			self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
			vec![Benchmark.as_ref()],
		)?;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		let result = self.run(binary_path);

		// Display the benchmarking command.
		cliclack::log::remark("\n")?;
		cli.info(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	fn run(&mut self, binary_path: PathBuf) -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let original_weight_path = self
			.command
			.params
			.weight_params
			.weight_path
			.clone()
			.unwrap_or_else(|| PathBuf::from("."));
		self.command.params.weight_params.weight_path = Some(temp_dir.path().to_path_buf());

		// Run the benchmark with updated arguments.
		generate_binary_benchmarks(
			&binary_path,
			BenchmarkingCliCommand::Storage,
			|args| {
				args.into_iter()
					.map(|arg| {
						if arg.starts_with("--weight-path") {
							format!("--weight-path={}", temp_dir.path().display())
						} else {
							arg
						}
					})
					.collect()
			},
			&EXCLUDED_ARGS,
		)?;

		// Restore the original weight path.
		self.command.params.weight_params.weight_path = Some(original_weight_path.clone());
		// Overwrite the weight files with the correct executed command.
		overwrite_weight_dir_command(
			temp_dir.path(),
			&original_weight_path,
			&self.collect_display_arguments(),
		)?;
		Ok(())
	}

	fn display(&self) -> String {
		self.collect_display_arguments().join(" ")
	}

	fn collect_display_arguments(&self) -> Vec<String> {
		let mut args = vec!["pop".to_string(), "bench".to_string(), "storage".to_string()];
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
		args
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
	fn benchmark_storage_works() -> anyhow::Result<()> {
		let name = "node";
		let temp_dir = tempdir()?;
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let target_path = Profile::Debug.target_directory(temp_dir.path());

		fs::create_dir(&temp_dir.path().join("target"))?;
		fs::create_dir(&target_path)?;
		File::create(target_path.join("node"))?;

		// With `profile` provided.
		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the storage speed of a chain snapshot")
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking and generating weight file...")
			.expect_info("pop bench storage --profile=debug")
			.expect_outro_cancel(
				// As we only mock the node to test the interactive flow, the returned error is
				// expected.
				"Failed to run benchmarking: Permission denied (os error 13)",
			);
		BenchmarkStorage {
			command: StorageCmd::try_parse_from(vec!["", "--state-version=1"])?,
			profile: Some(Profile::Debug),
		}
		.benchmark(&mut cli, temp_dir.path())?;
		cli.verify()?;

		let mut cli = MockCli::new()
			.expect_intro("Benchmarking the storage speed of a chain snapshot")
			.expect_select(
				"Choose the build profile of the binary that should be used: ",
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info("Benchmarking and generating weight file...")
			.expect_info("pop bench storage --profile=debug")
			.expect_outro_cancel(
				// As we only mock the node to test the interactive flow, the returned error is
				// expected.
				"Failed to run benchmarking: Permission denied (os error 13)",
			);
		BenchmarkStorage {
			command: StorageCmd::try_parse_from(vec!["", "--state-version=1"])?,
			profile: None,
		}
		.benchmark(&mut cli, temp_dir.path())?;
		cli.verify()
	}
}
