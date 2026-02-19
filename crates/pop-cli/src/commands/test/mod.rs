// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "contract")]
use crate::cli;
#[cfg(feature = "contract")]
use crate::output::invalid_input_error;
use crate::{
	common::builds::ensure_project_path,
	output::{OutputMode, build_error, build_error_with_details},
};
use clap::{Args, Subcommand};
use duct::cmd;
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

#[derive(Debug, Serialize)]
pub(crate) struct TestOutput {
	pub(crate) command: String,
	pub(crate) success: bool,
}

#[cfg_attr(not(feature = "chain"), allow(dead_code))]
#[derive(Debug, Serialize)]
pub(crate) struct RuntimeTestOutput {
	pub(crate) subcommand: String,
	pub(crate) success: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) output_path: Option<String>,
}

impl RuntimeTestOutput {
	#[cfg_attr(not(feature = "chain"), allow(dead_code))]
	pub(crate) fn success(subcommand: impl Into<String>, output_path: Option<String>) -> Self {
		Self { subcommand: subcommand.into(), success: true, output_path }
	}
}

impl Command {
	pub(crate) async fn execute(
		args: &mut TestArgs,
		output_mode: OutputMode,
	) -> anyhow::Result<TestOutput> {
		Self::test(
			args,
			output_mode,
			#[cfg(feature = "contract")]
			&mut cli::Cli,
		)
		.await
	}

	async fn test(
		args: &mut TestArgs,
		output_mode: OutputMode,
		#[cfg(feature = "contract")] cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<TestOutput> {
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
		let command = render_test_command(args.test.as_deref());

		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(&project_path)? {
			if output_mode == OutputMode::Json && args.contract.e2e {
				return Err(invalid_input_error(
					"`pop --json test --e2e` is not supported; run without `--json`",
				));
			}

			if output_mode == OutputMode::Json {
				run_cargo_test_json(&project_path, args.test.clone())?;
				return Ok(TestOutput { command, success: true });
			}

			args.contract.path = project_path.clone();
			args.contract.test = args.test.clone();
			contract::TestContractCommand::execute(&mut args.contract, cli).await?;
			return Ok(TestOutput { command, success: true });
		}

		if output_mode == OutputMode::Json {
			run_cargo_test_json(&project_path, args.test.clone())?;
		} else {
			test_project(&project_path, args.test.clone()).await?;
		}

		#[cfg(feature = "chain")]
		if pop_chains::is_supported(&project_path) {
			return Ok(TestOutput { command, success: true });
		}
		Ok(TestOutput { command, success: true })
	}
}

fn render_test_command(maybe_test_filter: Option<&str>) -> String {
	match maybe_test_filter {
		Some(test_filter) => format!("cargo test {test_filter}"),
		None => "cargo test".to_string(),
	}
}

fn run_cargo_test_json(
	path: &std::path::Path,
	maybe_test_filter: Option<String>,
) -> anyhow::Result<()> {
	let mut args = vec!["test".to_string()];
	if let Some(test_filter) = maybe_test_filter {
		args.push(test_filter);
	}
	let output = cmd("cargo", args)
		.dir(path)
		.stdout_capture()
		.stderr_capture()
		.unchecked()
		.run()
		.map_err(|error| build_error(format!("Failed to execute test command: {error}")))?;

	if output.status.success() {
		return Ok(());
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	let stderr = String::from_utf8_lossy(&output.stderr);
	let details = format!("stdout:\n{}\n\nstderr:\n{}", stdout.trim(), stderr.trim());
	Err(build_error_with_details("Failed to execute test command", details))
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
	use crate::{
		cli::MockCli,
		output::{BuildCommandError, OutputMode},
	};
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
			OutputMode::Human,
			#[cfg(feature = "contract")]
			&mut cli,
		)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn json_test_failure_includes_build_details() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let error = run_cargo_test_json(temp_dir.path(), None).unwrap_err();
		let build_error =
			error.downcast_ref::<BuildCommandError>().expect("expected BuildCommandError");
		assert!(build_error.details().is_some());
		Ok(())
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
