// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "contract")]
use crate::cli;
use crate::common::builds::ensure_project_path;
use clap::{Args, Subcommand};
use pop_common::test_project;
use serde::Serialize;
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
#[derive(Args, Default, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
	#[cfg(any(feature = "contract", feature = "chain"))]
	#[command(subcommand)]
	pub(crate) command: Option<Command>,
	/// Directory path for your project [default: current directory]
	#[serde(skip_serializing)]
	#[arg(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	#[command(flatten)]
	#[cfg(feature = "contract")]
	pub(crate) contract: contract::TestContractCommand,
	/// Run with the specified test filter.
	#[arg(value_name = "FILTER", index = 2)]
	pub(crate) test: Option<String>,
}

/// Test a Rust project.
#[derive(Subcommand, Serialize)]
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
	pub(crate) async fn execute(args: &mut TestArgs) -> anyhow::Result<()> {
		Self::test(
			args,
			#[cfg(feature = "contract")]
			&mut cli::Cli,
		)
		.await
	}

	async fn test(
		args: &mut TestArgs,
		#[cfg(feature = "contract")] cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
		// If user gave only one positional and it doesn’t resolve to a directory,
		// treat it as the test filter and default the project path to CWD.
		if args.test.is_none() &&
			args.path.is_none() &&
			let Some(ref pb) = args.path_pos &&
			!pb.is_dir()
		{
			// Reinterpret the first positional as the test filter
			args.test = Some(pb.to_string_lossy().into_owned());
			args.path_pos = None; // no positional path; will default to CWD
		}

		let project_path = ensure_project_path(args.path.clone(), args.path_pos.clone());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(&project_path)? {
			args.contract.path = project_path.clone();
			args.contract.test = args.test.clone();
			return contract::TestContractCommand::execute(&mut args.contract, cli).await;
		}

		test_project(&project_path, args.test.clone())?;

		#[cfg(feature = "chain")]
		if pop_chains::is_supported(&project_path) {
			return Ok(());
		}
		Ok(())
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
		let mut args = create_test_args(project_path)?;

		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;
		#[allow(unused_mut)]
		let mut cli = MockCli::new();
		Command::test(
			&mut args,
			#[cfg(feature = "contract")]
			&mut cli,
		)
		.await?;
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
