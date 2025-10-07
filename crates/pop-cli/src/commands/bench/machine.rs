// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self},
	common::{
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		prompt::display_message,
		runtime::Feature::Benchmark,
	},
};
use clap::Args;
use pop_chains::{BenchmarkingCliCommand, bench::MachineCmd, generate_binary_benchmarks};
use pop_common::Profile;
use std::{
	env::current_dir,
	path::{Path, PathBuf},
};

const EXCLUDED_ARGS: [&str; 1] = ["--profile"];

#[derive(Args)]
pub(crate) struct BenchmarkMachine {
	/// Command to benchmark the hardware.
	#[clap(flatten)]
	pub command: MachineCmd,
	/// Build profile.
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
}

impl BenchmarkMachine {
	pub(crate) fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.benchmark(cli, &current_dir().unwrap_or(PathBuf::from("./")))
	}

	fn benchmark(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		target_path: &Path,
	) -> anyhow::Result<()> {
		cli.intro("Benchmarking the hardware")?;

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
		cli.info("Benchmarking your hardware performance...")?;

		let result = generate_binary_benchmarks(
			&binary_path,
			BenchmarkingCliCommand::Machine,
			|args| args,
			&EXCLUDED_ARGS,
		);

		// Display the benchmarking command.
		cli.plain("\n")?;
		cli.info(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut args = vec!["pop bench machine".to_string()];
		let mut arguments: Vec<String> = std::env::args().skip(3).collect();
		#[cfg(test)]
		{
			arguments.retain(|arg| {
				!matches!(arg.as_str(), "--show-output" | "--nocapture" | "--ignored")
			});
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
	use super::*;

	use clap::Parser;
	use pop_common::Profile;

	#[test]
	fn benchmark_machine_works() -> anyhow::Result<()> {
		let mut command_info = BenchmarkMachine {
			command: MachineCmd::try_parse_from(vec!["", "--allow-fail"])?,
			profile: Some(Profile::Debug),
		}
		.display();
		assert_eq!(command_info, "pop bench machine --profile=debug");

		command_info = BenchmarkMachine {
			command: MachineCmd::try_parse_from(vec!["", "--allow-fail"])?,
			profile: None,
		}
		.display();
		assert_eq!(command_info, "pop bench machine");
		Ok(())
	}
}
