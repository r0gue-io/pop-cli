// SPDX-License-Identifier: GPL-3.0

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
use crate::cli;
use crate::common::{
	builds::get_project_path,
	Project::{self, *},
	TestFeature::{self, Unit},
};
use clap::{Args, Subcommand};
use pop_common::test_project;
#[cfg(any(feature = "parachain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
use std::fmt::{Display, Formatter, Result};
use std::path::PathBuf;

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
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
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "parachain"))]
	#[command(subcommand)]
	pub(crate) command: Option<Command>,
	/// Directory path for your project [default: current directory]
	#[arg(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	#[command(flatten)]
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
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
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	#[clap(alias = "c")]
	#[deprecated(since = "0.7.0", note = "will be removed in v0.8.0")]
	#[allow(rustdoc::broken_intra_doc_links)]
	Contract(contract::TestContractCommand),
}

impl Command {
	pub(crate) async fn execute(args: TestArgs) -> anyhow::Result<(Project, TestFeature)> {
		Self::test(
			args,
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			&mut cli::Cli,
		)
		.await
	}

	async fn test(
		args: TestArgs,
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<(Project, TestFeature)> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
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

#[cfg(any(feature = "parachain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "parachain")]
			Command::OnRuntimeUpgrade(_) => write!(f, "on runtime upgrade"),
			#[cfg(feature = "parachain")]
			Command::ExecuteBlock(_) => write!(f, "execute block"),
			#[cfg(feature = "parachain")]
			Command::FastForward(_) => write!(f, "fast forward"),
			#[cfg(feature = "parachain")]
			Command::CreateSnapshot(_) => write!(f, "create snapshot"),
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			#[allow(deprecated)]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	use {
		pop_contracts::{mock_build_process, new_environment},
		std::env,
	};

	fn create_test_args(project_path: PathBuf) -> anyhow::Result<TestArgs> {
		Ok(TestArgs { path: Some(project_path), ..Default::default() })
	}

	#[tokio::test]
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
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
		#[allow(unused_mut)]
		let mut cli = MockCli::new();
		assert_eq!(
			Command::test(
				args,
				#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
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
		#[cfg(feature = "parachain")]
		assert_eq!(Command::OnRuntimeUpgrade(Default::default()).to_string(), "on runtime upgrade");
		#[cfg(feature = "parachain")]
		assert_eq!(Command::ExecuteBlock(Default::default()).to_string(), "execute block");
		#[cfg(feature = "parachain")]
		assert_eq!(Command::FastForward(Default::default()).to_string(), "fast forward");
		#[cfg(feature = "parachain")]
		assert_eq!(Command::CreateSnapshot(Default::default()).to_string(), "create snapshot");
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}
}
