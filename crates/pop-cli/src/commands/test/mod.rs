// SPDX-License-Identifier: GPL-3.0

use crate::{cli, common::builds::get_project_path};
use clap::{Args, Subcommand};
use pop_common::test_project;
use std::path::PathBuf;

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub mod contract;

/// Arguments for testing.
#[derive(Args, Default)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
	#[command(subcommand)]
	pub(crate) command: Option<Command>,
	/// Directory path for your project [default: current directory]
	#[arg(short, long, global = true)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, global = true, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	#[command(flatten)]
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	pub(crate) contract: contract::TestContractCommand,
}

/// Test a Rust project.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// [DEPRECATED] Test a smart contract (will be removed in v0.8.0).
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	#[clap(alias = "c")]
	Contract(contract::TestContractCommand),
}
impl Command {
	pub(crate) async fn execute(args: TestArgs) -> anyhow::Result<&'static str> {
		Self::test(args, &mut cli::Cli).await
	}

	async fn test(args: TestArgs, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			return contract::TestContractCommand::execute(cmd, cli).await;
		}
		test_project(project_path.as_deref())?;
		Ok("test")
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
		assert_eq!(Command::test(args, &mut cli).await?, "unit");
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
		assert_eq!(Command::test(args, &mut cli).await?, "test");
		cli.verify()
	}
}
