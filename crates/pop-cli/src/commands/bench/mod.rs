// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self},
	common::prompt::display_message,
};
use clap::{Args, Subcommand};
use pallet::BenchmarkPalletArgs;

mod pallet;

/// Arguments for benchmarking a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true, ignore_errors = true)]
pub struct BenchmarkArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Benchmark a pallet or a parachain.
#[derive(Subcommand)]
pub enum Command {
	/// Benchmark the extrinsic weight of FRAME Pallets
	#[clap(alias = "p", disable_help_flag = true)]
	Pallet(BenchmarkPalletArgs),
}

impl Command {
	/// Executes the command.
	pub(crate) fn execute(args: BenchmarkArgs) -> anyhow::Result<()> {
		let mut cli = cli::Cli;

		match args.command {
			Command::Pallet(mut sub_args) => sub_args.execute(&mut cli),
		}
	}
}
