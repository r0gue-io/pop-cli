// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self};
use block::BenchmarkBlock;
use clap::{Args, Subcommand};
use machine::BenchmarkMachine;
use overhead::BenchmarkOverhead;
use pallet::BenchmarkPallet;
use std::fmt::{Display, Formatter, Result};
use storage::BenchmarkStorage;

mod block;
mod machine;
mod overhead;
mod pallet;
mod storage;

/// Arguments for benchmarking a project.
#[derive(Args)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the execution time of historic blocks.
	#[clap(alias = "b")]
	Block(BenchmarkBlock),
	/// Benchmark the machine performance.
	#[clap(alias = "m")]
	Machine(BenchmarkMachine),
	/// Benchmark the execution overhead per-block and per-extrinsic.
	#[clap(alias = "o")]
	Overhead(BenchmarkOverhead),
	/// Benchmark the extrinsic weight of pallets.
	#[clap(alias = "p")]
	Pallet(BenchmarkPallet),
	/// Benchmark the storage speed of a chain snapshot.
	#[clap(alias = "s")]
	Storage(BenchmarkStorage),
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;
		match args.command {
			Command::Block(mut cmd) => cmd.execute(&mut cli),
			Command::Machine(mut cmd) => cmd.execute(&mut cli),
			Command::Overhead(mut cmd) => cmd.execute(&mut cli).await,
			Command::Pallet(mut cmd) => cmd.execute(&mut cli).await,
			Command::Storage(mut cmd) => cmd.execute(&mut cli),
		}
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Command::*;
		match self {
			Block(_) => write!(f, "block"),
			Machine(_) => write!(f, "machine"),
			Overhead(_) => write!(f, "overhead"),
			Pallet(_) => write!(f, "pallet"),
			Storage(_) => write!(f, "storage"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Others can not be tested yet due to private external types.
	#[test]
	fn command_display_works() {
		assert_eq!(Command::Pallet(Default::default()).to_string(), "pallet");
	}
}
