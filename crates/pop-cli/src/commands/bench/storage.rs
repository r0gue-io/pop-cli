use crate::{
	cli::{self},
	common::{
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		prompt::display_message,
	},
};
use clap::Args;
use frame_benchmarking_cli::StorageCmd;
use pop_parachains::generate_binary_benchmarks;

#[derive(Args)]
pub struct BenchmarkStorage {
	#[clap(flatten)]
	pub command: StorageCmd,
}

impl BenchmarkStorage {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking the storage speed of a chain snapshot")?;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		let profile = guide_user_to_select_profile(cli)?;
		let binary_path = ensure_node_binary_exists(cli, &profile, vec!["runtime-benchmarks"])?;
		if let Err(e) = generate_binary_benchmarks(&binary_path, "storage").await {
			return display_message(&format!("Failed to run storage benchmark: {}", e), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}
}
