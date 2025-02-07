// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::builds::get_project_path,
};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "parachain")]
mod network;

/// Arguments for launching or deploying a project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	/// Path to the project directory.
	// TODO: Introduce the short option in v0.8.0 once deprecated parachain command is removed.
	#[arg(long, global = true)]
	pub path: Option<PathBuf>,

	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, global = true, conflicts_with = "path")]
	pub path_pos: Option<PathBuf>,

	#[command(flatten)]
	#[cfg(feature = "contract")]
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
	#[cfg(feature = "parachain")]
	/// [DEPRECATED] Launch a local network (will be removed in v0.8.0).
	#[clap(alias = "p", hide = true)]
	Parachain(network::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// [DEPRECATED] Deploy a smart contract (will be removed in v0.8.0).
	#[clap(alias = "c", hide = true)]
	Contract(contract::UpContractCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: UpArgs) -> anyhow::Result<&'static str> {
		Self::execute_project_deployment(args, &mut Cli).await
	}

	/// Identifies the project type and executes the appropriate deployment process.
	async fn execute_project_deployment(
		args: UpArgs,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<&'static str> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());
		// If only contract feature enabled, deploy a contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			cmd.valid = true; // To handle deprecated command, remove in v0.8.0.
			cmd.execute().await?;
			return Ok("contract");
		}
		if pop_parachains::is_supported(project_path.as_deref())? {
			cli.warning("Parachain deployment is currently not implemented.")?;
			return Ok("parachain");
		}
		cli.warning("No contract or parachain detected. Ensure you are in a valid project directory.")?;
		Ok("")
	}
}
