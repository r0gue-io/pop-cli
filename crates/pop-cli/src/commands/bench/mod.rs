// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self};
use block::BenchmarkBlock;
use clap::{Args, Subcommand};
use machine::BenchmarkMachine;
use overhead::BenchmarkOverhead;
use pallet::BenchmarkPallet;
use serde::Serialize;
use std::fmt::{Display, Formatter, Result};
use storage::BenchmarkStorage;
use tracing_subscriber::EnvFilter;

mod block;
mod machine;
mod overhead;
mod pallet;
mod storage;

/// Arguments for benchmarking a project.
#[derive(Args, Serialize)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand, Serialize)]
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
	pub(crate) async fn execute(
		args: &mut BenchmarkArgs,
		json: bool,
	) -> anyhow::Result<serde_json::Value> {
		// Disable these log targets because they are spammy.
		let unwanted_targets = [
			"cranelift_codegen",
			"wasm_cranelift",
			"wasmtime_jit",
			"wasmtime_cranelift",
			"wasm_jit",
		];

		let env_filter = unwanted_targets.iter().fold(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
			|filter, &target| filter.add_directive(format!("{target}=off").parse().unwrap()),
		);
		tracing_subscriber::fmt()
			.with_env_filter(env_filter)
			.with_writer(std::io::stderr)
			.init();
		let mut cli = cli::Cli { json };
		match &mut args.command {
			Command::Block(cmd) => cmd.execute(&mut cli).await,
			Command::Machine(cmd) => cmd.execute(&mut cli).await,
			Command::Overhead(cmd) => cmd.execute(&mut cli).await,
			Command::Pallet(cmd) => cmd.execute(&mut cli).await,
			Command::Storage(cmd) => cmd.execute(&mut cli).await,
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
