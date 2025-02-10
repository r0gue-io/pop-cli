use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct BenchmarkPalletCommand {
	/// Path for the contract project [default: current directory]
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
}

impl BenchmarkPalletCommand {
	/// Executes the command.
	pub(crate) fn execute(&self) -> anyhow::Result<&'static str> {
		unimplemented!()
	}
}
