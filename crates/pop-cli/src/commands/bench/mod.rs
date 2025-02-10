// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "parachain")]
pub(crate) mod pallet;

/// Arguments for bencharmking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Option<Command>,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the extrinsic weight of FRAME Pallets
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Pallet(pallet::BenchmarkPalletCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(_args: BenchmarkArgs) -> anyhow::Result<&'static str> {
		unimplemented!()
	}
}
