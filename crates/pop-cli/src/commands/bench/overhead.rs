use crate::cli::{self, traits::Cli};
use clap::Args;
use frame_benchmarking_cli::OverheadCmd;
use pop_parachains::generate_overhead_benchmarks;

use super::display_message;

#[derive(Args)]
pub struct BenchmarkOverhead {
	#[clap(flatten)]
	pub command: OverheadCmd,
}

impl BenchmarkOverhead {
	pub fn execute(&mut self) -> anyhow::Result<()> {
		let mut cli = cli::Cli;

		cli.warning("NOTE: this may take some time...")?;
		cli.info("Benchmarking and generating weight file...")?;

		if let Err(e) = generate_overhead_benchmarks(&self.command) {
			return display_message(&e.to_string(), false, &mut cli);
		}
		display_message("Benchmark completed successfully!", true, &mut cli)?;
		Ok(())
	}
}
