use crate::{
	cli::{self},
	common::prompt::display_message,
};
use clap::Args;
use frame_benchmarking_cli::StorageCmd;
use pop_parachains::generate_storage_benchmarks;

#[derive(Args)]
pub struct BenchmarkStorage {
	#[clap(flatten)]
	pub command: StorageCmd,

	/// If this is set to true, no interactive prompts will be shown.
	#[clap(short = 'i', long)]
	pub skip_all: bool,
}

impl BenchmarkStorage {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Benchmarking the storage speed of a chain snapshot")?;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		if let Err(e) = self.run().await {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Benchmark completed successfully!", true, cli)?;
		Ok(())
	}

	async fn run(&self) -> anyhow::Result<()> {
		generate_storage_benchmarks(StorageCmd {
			shared_params: self.command.shared_params.clone(),
			database_params: self.command.database_params.clone(),
			pruning_params: self.command.pruning_params.clone(),
			params: self.command.params.clone(),
		})
		.await
	}
}
