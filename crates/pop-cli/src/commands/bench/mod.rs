// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use frame_benchmarking_cli::PalletCmd;

/// Arguments for bencharmking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the extrinsic weight of FRAME Pallets
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Pallet(PalletCmd),
}
