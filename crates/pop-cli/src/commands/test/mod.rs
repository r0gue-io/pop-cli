// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	common::{
		builds::get_project_path,
		Project::{self, *},
		TestFeature::{self, Unit},
	},
};
use clap::{Args, Subcommand};
use pop_common::test_project;
use std::path::PathBuf;

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "parachain")]
pub mod create_snapshot;
#[cfg(feature = "parachain")]
pub mod execute_block;
#[cfg(feature = "parachain")]
pub mod fast_forward;
#[cfg(feature = "parachain")]
pub mod on_runtime_upgrade;

/// Arguments for testing.
#[derive(Args, Default)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
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
	#[cfg(feature = "parachain")]
	OnRuntimeUpgrade(on_runtime_upgrade::TestOnRuntimeUpgradeCommand),
	/// Executes the given block against some state
	#[cfg(feature = "parachain")]
	ExecuteBlock(execute_block::TestExecuteBlockCommand),
	/// Executes a runtime upgrade (optional), then mines a number of blocks while performing
	/// try-state checks
	#[cfg(feature = "parachain")]
	FastForward(fast_forward::TestFastForwardCommand),
	/// Create a chain state snapshot.
	#[cfg(feature = "parachain")]
	CreateSnapshot(create_snapshot::TestCreateSnapshotCommand),
	/// [DEPRECATED] Test a smart contract (will be removed in v0.8.0).
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::TestContractCommand),
}

impl Command {
	pub(crate) async fn execute(args: TestArgs) -> anyhow::Result<(Project, TestFeature)> {
		Self::test(args, &mut cli::Cli).await
	}

	async fn test(
		args: TestArgs,
		cli: &mut impl cli::traits::Cli,
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

		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(project_path.as_deref())? {
			return Ok((Chain, Unit));
		}
		Ok((Unknown, Unit))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use pop_contracts::{mock_build_process, new_environment};
	use std::env;

	fn create_test_args(project_path: PathBuf) -> anyhow::Result<TestArgs> {
		Ok(TestArgs { path: Some(project_path), ..Default::default() })
	}

	#[tokio::test]
	async fn detects_contract_correctly() -> anyhow::Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;
		let args = create_test_args(temp_dir.path().join("testing"))?;
		let mut cli = MockCli::new()
			.expect_intro("Starting unit tests")
			.expect_outro("Unit testing complete");
		assert_eq!(Command::test(args, &mut cli).await?, (Contract, Unit));
		cli.verify()
	}

	#[tokio::test]
	async fn detects_rust_project_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "hello_world";
		let path = temp_dir.path();
		let project_path = path.join(name);
		let args = create_test_args(project_path)?;

		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		let mut cli = MockCli::new();
		assert_eq!(Command::test(args, &mut cli).await?, (Unknown, Unit));
		cli.verify()
	}
}
