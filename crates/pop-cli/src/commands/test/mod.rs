// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "contract")]
use crate::cli;
use crate::common::{
	builds::get_project_path,
	Project::{self, *},
	TestFeature::{self, Unit},
};
use clap::{Args, Subcommand};
use pop_common::test_project;
#[cfg(feature = "chain")]
use std::fmt::{Display, Formatter, Result};
use std::path::PathBuf;

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "chain")]
pub mod create_snapshot;
#[cfg(feature = "chain")]
pub mod execute_block;
#[cfg(feature = "chain")]
pub mod fast_forward;
#[cfg(feature = "chain")]
pub mod on_runtime_upgrade;

/// Arguments for testing.
#[derive(Args, Default)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
	#[cfg(any(feature = "contract", feature = "chain"))]
	#[command(subcommand)]
	pub(crate) command: Option<Command>,
	/// Directory path for your project [default: current directory]
	#[arg(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	#[command(flatten)]
	#[cfg(feature = "contract")]
	pub(crate) contract: contract::TestContractCommand,
}

/// Test a Rust project.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Test migrations.
	#[cfg(feature = "chain")]
	OnRuntimeUpgrade(on_runtime_upgrade::TestOnRuntimeUpgradeCommand),
	/// Executes the given block against some state
	#[cfg(feature = "chain")]
	ExecuteBlock(execute_block::TestExecuteBlockCommand),
	/// Executes a runtime upgrade (optional), then mines a number of blocks while performing
	/// try-state checks
	#[cfg(feature = "chain")]
	FastForward(fast_forward::TestFastForwardCommand),
	/// Create a chain state snapshot.
	#[cfg(feature = "chain")]
	CreateSnapshot(create_snapshot::TestCreateSnapshotCommand),
}

impl Command {
	pub(crate) async fn execute(args: TestArgs) -> anyhow::Result<(Project, TestFeature)> {
		Self::test(
			args,
			#[cfg(feature = "contract")]
			&mut cli::Cli,
		)
		.await
	}

	async fn test(
		args: TestArgs,
		#[cfg(feature = "contract")] cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<(Project, TestFeature)> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			let feature = contract::TestContractCommand::execute(cmd, cli).await?;
			return Ok((Contract, feature));
		}

		test_project(project_path.as_deref())?;

		#[cfg(feature = "chain")]
		if pop_chains::is_supported(project_path.as_deref())? {
			return Ok((Chain, Unit));
		}
		Ok((Unknown, Unit))
	}
}

#[cfg(feature = "chain")]
impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			Command::OnRuntimeUpgrade(_) => write!(f, "on runtime upgrade"),
			Command::ExecuteBlock(_) => write!(f, "execute block"),
			Command::FastForward(_) => write!(f, "fast forward"),
			Command::CreateSnapshot(_) => write!(f, "create snapshot"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;

	fn create_test_args(project_path: PathBuf) -> anyhow::Result<TestArgs> {
		Ok(TestArgs { path: Some(project_path), ..Default::default() })
	}

	#[tokio::test]
	async fn detects_rust_project_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "hello_world";
		let path = temp_dir.path();
		let project_path = path.join(name);
		let args = create_test_args(project_path)?;

		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;
		#[allow(unused_mut)]
		let mut cli = MockCli::new();
		assert_eq!(
			Command::test(
				args,
				#[cfg(feature = "contract")]
				&mut cli
			)
			.await?,
			(Unknown, Unit)
		);
		cli.verify()
	}

	#[test]
	#[allow(deprecated)]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::OnRuntimeUpgrade(Default::default()).to_string(), "on runtime upgrade");
		#[cfg(feature = "chain")]
		assert_eq!(Command::ExecuteBlock(Default::default()).to_string(), "execute block");
		#[cfg(feature = "chain")]
		assert_eq!(Command::FastForward(Default::default()).to_string(), "fast forward");
		#[cfg(feature = "chain")]
		assert_eq!(Command::CreateSnapshot(Default::default()).to_string(), "create snapshot");
	}
}
