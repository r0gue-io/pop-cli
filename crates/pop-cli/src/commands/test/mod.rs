// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub mod contract;

/// Arguments for testing.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Test a smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Test a smart contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::TestContractCommand),
}
