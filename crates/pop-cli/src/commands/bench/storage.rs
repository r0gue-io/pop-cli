// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self},
	common::{
		bench::{check_omni_bencher_and_prompt, overwrite_weight_dir_command},
		prompt::display_message,
	},
};
use clap::Args;
use pop_chains::{BenchmarkingCliCommand, bench::StorageCmd, generate_binary_benchmarks};
use serde::Serialize;
use std::path::PathBuf;
use tempfile::tempdir;

const EXCLUDED_ARGS: [&str; 2] = ["--skip-confirm", "-y"];

#[derive(Args, Serialize)]
pub(crate) struct BenchmarkStorage {
	/// Command to benchmark the storage speed of a chain snapshot.
	#[serde(skip_serializing)]
	#[clap(flatten)]
	pub command: StorageCmd,
	/// Skip confirmation prompt when sourcing the `frame-omni-bencher` binary.
	#[clap(short = 'y', long)]
	pub(crate) skip_confirm: bool,
}

impl BenchmarkStorage {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.benchmark(cli).await
	}

	async fn benchmark(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking the storage speed of a chain snapshot")?;

		let spinner = cli.spinner();
		let binary_path = check_omni_bencher_and_prompt(cli, &spinner, self.skip_confirm).await?;
		spinner.clear();

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		let result = self.run(binary_path);

		// Display the benchmarking command.
		cli.plain("\n")?;
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
			arguments.retain(|arg| {
				!matches!(arg.as_str(), "--show-output" | "--nocapture" | "--ignored")
			});
		}
		if self.skip_confirm {
			arguments.push("--skip-confirm".to_string());
		}
		args.extend(arguments);
		args
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clap::Parser;

	#[tokio::test]
	async fn display_works() -> anyhow::Result<()> {
		// With --skip-confirm enabled, the flag should appear in the display string
		let mut command_info = BenchmarkStorage {
			command: StorageCmd::try_parse_from(vec!["", "--state-version", "1"])?,
			skip_confirm: true,
		}
		.display();
		assert_eq!(command_info, "pop bench storage --skip-confirm");

		// Without --skip-confirm, the display string should only include the base command
		command_info = BenchmarkStorage {
			command: StorageCmd::try_parse_from(vec!["", "--state-version", "0"])?,
			skip_confirm: false,
		}
		.display();
		assert_eq!(command_info, "pop bench storage");
		Ok(())
	}
}
