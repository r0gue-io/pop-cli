// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]

#[cfg(all(feature = "polkavm-contracts", feature = "wasm-contracts"))]
compile_error!("only feature \"polkavm-contracts\" OR \"wasm-contracts\" must be enabled");

use anyhow::{anyhow, Result};
use clap::Parser;
use commands::*;
#[cfg(feature = "telemetry")]
use pop_telemetry::{config_file_path, record_cli_command, record_cli_used, Telemetry};
use std::{
	fmt::{self, Display, Formatter},
	fs::create_dir_all,
	path::PathBuf,
};

mod cli;
mod commands;
mod common;
#[cfg(feature = "parachain")]
mod deployment_api;
mod style;
#[cfg(feature = "wallet-integration")]
mod wallet_integration;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "telemetry")]
	let maybe_tel = init().unwrap_or(None);

	let cli = Cli::parse();
	#[cfg(feature = "telemetry")]
	let event = cli.command.to_string();
	let result = cli.command.execute().await;
	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel {
		let data = result.as_ref().map_or_else(|e| e.to_string(), |t| t.to_string());
		// Best effort to send on first try, no action if failure.
		let _ = record_cli_command(tel, &event, &data).await;
	}
	result.map(|_| ())
}

/// An all-in-one tool for Polkadot development.
#[derive(Parser)]
#[command(author, version, about, styles=style::get_styles())]
pub struct Cli {
	#[command(subcommand)]
	command: Command,
}

impl Display for Cli {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.command)
	}
}

/// Determines the cache to be used.
fn cache() -> Result<PathBuf> {
	let cache_path = dirs::cache_dir()
		.ok_or(anyhow!("the cache directory could not be determined"))?
		.join("pop");
	// Creates pop dir if needed
	create_dir_all(cache_path.as_path())?;
	Ok(cache_path)
}

/// Initializes telemetry.
#[cfg(feature = "telemetry")]
fn init() -> Result<Option<Telemetry>> {
	let maybe_config_path = config_file_path();

	let maybe_tel = maybe_config_path.ok().map(|path| Telemetry::new(&path));

	// Handle for await not used here as telemetry should complete before any of the commands do.
	// Sends a generic ping saying the CLI was used.
	if let Some(tel) = maybe_tel.clone() {
		tokio::spawn(record_cli_used(tel));
	}

	// if config file errors set telemetry to None, otherwise Some(tel)
	Ok(maybe_tel)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn verify_cli() {
		// https://docs.rs/clap/latest/clap/_derive/_tutorial/chapter_4/index.html
		use clap::CommandFactory;
		Cli::command().debug_assert()
	}

	#[test]
	fn test_cache() -> Result<(), Box<dyn std::error::Error>> {
		let path = cache()?;
		assert_eq!(path.file_name().unwrap().to_str().unwrap().to_string(), "pop");
		Ok(())
	}

	#[cfg(feature = "telemetry")]
	mod telemetry {
		use super::*;
		use anyhow::anyhow;
		use common::{
			Data::{self, *},
			Os::*,
			Project::*,
			Template,
			TestFeature::*,
		};

		// Helper function to simulate what happens in main().
		fn simulate_command_flow<T: Display>(
			command: Command,
			result: Result<T>,
		) -> (String, String) {
			let cli = Cli { command };
			let event = cli.to_string();

			let data = result.as_ref().map_or_else(|e| e.to_string(), |t| t.to_string());

			(event, data)
		}

		#[test]
		fn test_command() {
			// Test command display.
			assert_eq!(
				Cli {
					command: Command::Test(test::TestArgs {
						command: None,
						path: None,
						path_pos: None,
						#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
						contract: Default::default(),
					})
				}
				.to_string(),
				"test"
			);
			// Successful execution.
			let (command, data) = simulate_command_flow(
				Command::Test(Default::default()),
				Ok(Test { project: Contract, feature: Unit }),
			);
			assert_eq!(command, "test");
			assert_eq!(data, "contract unit");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Test(Default::default()),
				Err(anyhow!("build error")) as Result<Data>,
			);
			assert_eq!(command, "test");
			assert_eq!(data, "build error");
		}

		#[test]
		fn build_command() {
			use crate::commands::build::Command as BuildCommand;
			// Build command display.
			assert_eq!(Cli { command: Command::Build(Default::default()) }.to_string(), "build");
			// Build command with spec subcommand.
			assert_eq!(
				Cli {
					command: Command::Build(build::BuildArgs {
						command: Some(BuildCommand::Spec(Default::default())),
						..Default::default()
					})
				}
				.to_string(),
				"build spec"
			);
			// Successful execution.
			let (command, data) =
				simulate_command_flow(Command::Build(Default::default()), Ok(Build(Contract)));
			assert_eq!(command, "build");
			assert_eq!(data, "contract");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Build(Default::default()),
				Err(anyhow!("compilation error")) as Result<Data>,
			);
			assert_eq!(command, "build");
			assert_eq!(data, "compilation error");
		}

		#[test]
		fn up_command() {
			// Up command display.
			assert_eq!(Cli { command: Command::Up(Default::default()) }.to_string(), "up");
			// Successful execution.
			let (command, data) =
				simulate_command_flow(Command::Up(Default::default()), Ok(Up(Contract)));
			assert_eq!(command, "up");
			assert_eq!(data, "contract");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Up(Default::default()),
				Err(anyhow!("network error")) as Result<Data>,
			);
			assert_eq!(command, "up");
			assert_eq!(data, "network error");
		}

		#[test]
		fn clean_command() {
			use clean::{CleanArgs, CleanCommandArgs, Command as CleanCommand};
			// Clean command display.
			assert_eq!(
				Cli {
					command: Command::Clean(CleanArgs {
						command: CleanCommand::Cache(CleanCommandArgs { all: false }),
					})
				}
				.to_string(),
				"clean"
			);
			// Successful execution.
			let (command, data) =
				simulate_command_flow(Command::Clean(Default::default()), Ok(Null));
			assert_eq!(command, "clean");
			assert_eq!(data, "");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Clean(Default::default()),
				Err(anyhow!("permission denied")) as Result<Data>,
			);
			assert_eq!(command, "clean");
			assert_eq!(data, "permission denied");
		}

		#[test]
		fn install_command() {
			// Install command display.
			assert_eq!(
				Cli { command: Command::Install(Default::default()) }.to_string(),
				"install"
			);
			// Successful execution.
			let (command, data) =
				simulate_command_flow(Command::Install(Default::default()), Ok(Install(Linux)));
			assert_eq!(command, "install");
			assert_eq!(data, "linux");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Install(Default::default()),
				Err(anyhow!("download error")) as Result<Data>,
			);
			assert_eq!(command, "install");
			assert_eq!(data, "download error");
		}

		#[test]
		fn new_command() {
			use crate::commands::new::{Command as NewCommand, NewArgs};
			// New command display.
			assert_eq!(
				Cli {
					command: Command::New(NewArgs {
						command: NewCommand::Parachain(Default::default())
					})
				}
				.to_string(),
				"new chain"
			);
			// Successful execution.
			let (command, data) = simulate_command_flow(
				Command::New(NewArgs { command: NewCommand::Contract(Default::default()) }),
				Ok(New(Template::Contract(Default::default()))),
			);
			assert_eq!(command, "new contract");
			assert_eq!(data, "Standard");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::New(NewArgs { command: NewCommand::Contract(Default::default()) }),
				Err(anyhow!("template error")) as Result<Data>,
			);
			assert_eq!(command, "new contract");
			assert_eq!(data, "template error");
		}

		#[test]
		fn bench_command() {
			use crate::commands::bench::{BenchmarkArgs, Command::Pallet};
			// Bench command display.
			assert_eq!(
				Cli {
					command: Command::Bench(BenchmarkArgs { command: Pallet(Default::default()) })
				}
				.to_string(),
				"bench pallet"
			);
			// Successful execution.
			let (command, data) = simulate_command_flow(
				Command::Bench(BenchmarkArgs { command: Pallet(Default::default()) }),
				Ok(Null),
			);
			assert_eq!(command, "bench pallet");
			assert_eq!(data, "");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Bench(BenchmarkArgs { command: Pallet(Default::default()) }),
				Err(anyhow!("runtime error")) as Result<Data>,
			);
			assert_eq!(command, "bench pallet");
			assert_eq!(data, "runtime error");
		}

		#[test]
		fn call_command() {
			use crate::commands::call::{CallArgs, Command as CallCommand};
			// Call chain command display.
			assert_eq!(
				Cli {
					command: Command::Call(CallArgs {
						command: CallCommand::Chain(Default::default())
					})
				}
				.to_string(),
				"call chain"
			);
			// Call contract command display.
			assert_eq!(
				Cli {
					command: Command::Call(CallArgs {
						command: CallCommand::Contract(Default::default())
					})
				}
				.to_string(),
				"call contract"
			);
			// Successful execution.
			let (command, data) = simulate_command_flow(
				Command::Call(CallArgs { command: CallCommand::Chain(Default::default()) }),
				Ok(Null),
			);
			assert_eq!(command, "call chain");
			assert_eq!(data, "");
			// Error handling.
			let (command, data) = simulate_command_flow(
				Command::Call(CallArgs { command: CallCommand::Chain(Default::default()) }),
				Err(anyhow!("connection error")) as Result<Data>,
			);
			assert_eq!(command, "call chain");
			assert_eq!(data, "connection error");
		}
	}
}
