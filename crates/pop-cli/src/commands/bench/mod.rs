// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self};
use clap::{Args, Subcommand};
use machine::BenchmarkMachine;
use pallet::BenchmarkPallet;
use storage::BenchmarkStorage;

mod machine;
mod pallet;
mod storage;

/// Arguments for benchmarking a project.
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
	#[clap(alias = "p")]
	Pallet(BenchmarkPallet),
	/// Benchmark the storage speed of a chain snapshot.
	#[clap(alias = "s")]
	Storage(BenchmarkStorage),
	/// Benchmark the machine performance.
	#[clap(alias = "m")]
	Machine(BenchmarkMachine),
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;
		match args.command {
			Command::Pallet(mut cmd) => cmd.execute(&mut cli).await,
			Command::Storage(mut cmd) => cmd.execute(&mut cli),
			Command::Machine(mut cmd) => cmd.execute(&mut cli),
		}
	}
}
