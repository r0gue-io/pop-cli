use std::{env::current_dir, path::PathBuf};

use crate::{
	cli::{self},
	common::{
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		prompt::display_message,
	},
};
use clap::Args;
use frame_benchmarking_cli::MachineCmd;
use pop_parachains::generate_binary_benchmarks;

#[derive(Args)]
pub struct BenchmarkMachine {
	#[clap(flatten)]
	pub command: MachineCmd,
}

impl BenchmarkMachine {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let cwd = current_dir().unwrap_or(PathBuf::from("./"));

		cli.intro("Benchmark the hardware")?;

		let profile = guide_user_to_select_profile(cli)?;
		let binary_path =
			ensure_node_binary_exists(cli, &cwd, &profile, vec!["runtime-benchmarks"])?;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		if let Err(e) = generate_binary_benchmarks(&binary_path, "machine").await {
			return display_message(&format!("Failed to run storage benchmark: {}", e), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}
}
