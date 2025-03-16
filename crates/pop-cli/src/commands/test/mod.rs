// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

use clap::{Args, Subcommand};
use pop_common::test_project;

use crate::common::builds::get_project_path;

#[cfg(feature = "contract")]
pub mod contract;

/// Arguments for testing.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
	#[command(subcommand)]
	pub command: Option<Command>,
	/// Directory path for your project [default: current directory]
	#[arg(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, global = true, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	#[command(flatten)]
	#[cfg(feature = "contract")]
	pub(crate) contract: contract::TestContractCommand,
}

/// Test a project.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// [DEPRECATED] Test a smart contract (will be removed in v0.8.0).
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::TestContractCommand),
}
impl Command {
	pub(crate) async fn execute(args: TestArgs) -> anyhow::Result<&'static str> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			return contract::TestContractCommand::execute(cmd).await;
		}
		test_project(project_path.as_deref())?;
		Ok("test")
	}
}
