// SPDX-License-Identifier: GPL-3.0

use crate::common::builds::get_project_path;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "parachain")]
mod network;

/// Arguments for launching or deploying.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	/// Path to the project directory.
	#[arg(short, long, global = true)]
	pub path: Option<PathBuf>,

	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, global = true, conflicts_with = "path")]
	pub path_pos: Option<PathBuf>,

	#[command(flatten)]
	#[cfg(feature = "contract")] // Display all arguments related to contract deployemnt
	pub contract: contract::UpContractCommand,

	#[command(subcommand)]
	pub(crate) command: Option<Command>,
}

/// Launch a local network or deploy a smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	#[cfg(feature = "parachain")]
	/// Launch a local network.
	#[clap(alias = "n")]
	Network(network::ZombienetCommand),
	/// [DEPRECATED] Launch a local network.
	#[clap(alias = "p", hide = true)]
	Parachain(network::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// [DEPRECATED] Deploy a smart contract.
	#[clap(alias = "c", hide = true)]
	Contract(contract::UpContractCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: UpArgs) -> anyhow::Result<()> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());
		// If only contract feature enabled, deploy a contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			cmd.valid = true;
			cmd.execute().await?;
		}
		// TODO: if pop_parachains::is_supported(project_path.as_deref())?
		Ok(())
	}
}
