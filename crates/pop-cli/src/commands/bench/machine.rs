// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, spinner, traits::Spinner},
	common::{bench::check_omni_bencher_and_prompt, prompt::display_message},
};
use clap::Args;
use pop_chains::{BenchmarkingCliCommand, bench::MachineCmd, generate_binary_benchmarks};
use serde::Serialize;

const EXCLUDED_ARGS: [&str; 2] = ["--skip-confirm", "-y"];

#[derive(Args, Serialize)]
pub(crate) struct BenchmarkMachine {
	/// Command to benchmark the hardware.
	#[serde(skip_serializing)]
	#[clap(flatten)]
	pub command: MachineCmd,
	/// Skip confirmation prompt when sourcing the `frame-omni-bencher` binary.
	#[clap(short = 'y', long)]
	pub(crate) skip_confirm: bool,
}

impl BenchmarkMachine {
	pub(crate) async fn execute(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<serde_json::Value> {
		self.benchmark(cli).await
	}

	async fn benchmark(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<serde_json::Value> {
		cli.intro("Benchmarking the hardware")?;

		let spinner = spinner();
		let binary_path = check_omni_bencher_and_prompt(cli, &spinner, self.skip_confirm).await?;
		spinner.clear();

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
		let output = match result {
			Ok(output) => {
				display_message("Benchmark completed successfully!", true, cli)?;
				output
			},
			Err(e) => {
				display_message(&e.to_string(), false, cli)?;
				return Err(e.into());
			},
		};
		Ok(serde_json::to_value(crate::common::output::SuccessData { message: output })?)
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
		if self.skip_confirm {
			arguments.push("--skip-confirm".to_string());
		}
		args.extend(arguments);
		args.join(" ")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use clap::Parser;

	#[test]
	fn display_works() -> anyhow::Result<()> {
		let mut command_info = BenchmarkMachine {
			command: MachineCmd::try_parse_from(vec!["", "--allow-fail"])?,
			skip_confirm: true,
		}
		.display();
		assert_eq!(command_info, "pop bench machine --skip-confirm");

		command_info = BenchmarkMachine {
			command: MachineCmd::try_parse_from(vec!["", "--allow-fail"])?,
			skip_confirm: false,
		}
		.display();
		assert_eq!(command_info, "pop bench machine");
		Ok(())
	}
}
