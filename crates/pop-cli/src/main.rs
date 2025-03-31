// SPDX-License-Identifier: GPL-3.0

#[cfg(not(any(feature = "contract", feature = "parachain")))]
compile_error!("feature \"contract\" or feature \"parachain\" must be enabled");

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
#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
mod common;
#[cfg(feature = "parachain")]
mod deployment_api;
mod style;
#[cfg(feature = "telemetry")]
use tracing_subscriber::EnvFilter;
mod wallet_integration;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "telemetry")]
	let maybe_tel = init().unwrap_or(None);

	let cli = Cli::parse();
	#[cfg(feature = "telemetry")]
	let command = cli.command.to_string();
	let result = cli.command.execute().await;
	let data = match result.as_ref() {
		Ok(t) => t.to_string(),
		Err(e) => e.to_string(),
	};

	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel {
		let _ = record_cli_command(tel.clone(), &command, &data).await;
	}
	result.map(|_| ())
}

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
	// Disable these log targets because they are spammy.
	let unwanted_targets =
		&["cranelift_codegen", "wasm_cranelift", "wasmtime_jit", "wasmtime_cranelift", "wasm_jit"];

	let mut env_filter =
		EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

	for target in unwanted_targets {
		env_filter = env_filter.add_directive(format!("{}=off", target).parse().unwrap());
	}

	tracing_subscriber::fmt()
		.with_env_filter(env_filter)
		.with_writer(std::io::stderr)
		.init();

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

	// Integration test that simulates the full command execution flow and replaces existing tests
	#[cfg(feature = "telemetry")]
	mod integration {
		use super::*;
		use anyhow::anyhow;
		use common::{Feature::*, Project::*, Telemetry::*};

		// Helper function to simulate what happens in main()
		fn simulate_command_flow<T: Display>(
			command: Command,
			result: Result<T>,
		) -> (String, String) {
			let cli = Cli { command };
			let command_string = cli.to_string();

			let data = result.as_ref().map_or_else(|e| e.to_string(), |t| t.to_string());

			(command_string, data)
		}

		#[test]
		fn test_command() {
			assert_eq!(
				Cli {
					command: Command::Test(test::TestArgs {
						command: None,
						path: None,
						path_pos: None,
						#[cfg(feature = "contract")]
						contract: Default::default(),
					})
				}
				.to_string(),
				"test"
			);
			// Test successful execution.
			let (command, data) = simulate_command_flow(
				Command::Test(Default::default()),
				Ok(Test { project: Contract, feature: Unit }),
			);
			assert_eq!(command, "test");
			assert_eq!(data, "contract unit");
			// Test failed execution.
			let error = "test failed: build error";
			let (command, data) = simulate_command_flow(
				Command::Test(Default::default()),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "test");
			assert_eq!(data, error);
		}

		#[test]
		fn build_command() {
			// Build command with no subcommand.
			assert_eq!(Cli { command: Command::Build(Default::default()) }.to_string(), "build");
			// Build command with spec subcommand.
			use crate::commands::build::Command as BuildCommand;
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
			// Test successful execution.
			let (command, data) =
				simulate_command_flow(Command::Build(Default::default()), Ok(Build(Contract)));
			assert_eq!(command, "build");
			assert_eq!(data, "contract");
			// Test failed execution.
			let error = "build failed: compilation error";
			let (command, data) = simulate_command_flow(
				Command::Build(Default::default()),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "build");
			assert_eq!(data, error);
		}

		#[test]
		fn up_command() {
			// Up command display.
			assert_eq!(Cli { command: Command::Up(Default::default()) }.to_string(), "up");
			// Test up with different project types.
			let (command, data) =
				simulate_command_flow(Command::Up(Default::default()), Ok(Up(Contract)));
			assert_eq!(command, "up");
			assert_eq!(data, "contract");
			// Test up error.
			let error = "up failed: network error";
			let (command, data) = simulate_command_flow(
				Command::Up(Default::default()),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "up");
			assert_eq!(data, error);
		}

		#[test]
		fn clean_command() {
			// Clean command.
			use clean::{CleanArgs, CleanCommandArgs, Command as CleanCommand};
			assert_eq!(
				Cli {
					command: Command::Clean(CleanArgs {
						command: CleanCommand::Cache(CleanCommandArgs { all: false }),
					})
				}
				.to_string(),
				"clean"
			);
			// Clean error case.
			let error = "clean failed: permission denied";
			let (command, data) = simulate_command_flow(
				Command::Clean(Default::default()),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "clean");
			assert_eq!(data, error);
		}

		#[test]
		fn install_command() {
			// Install command.
			use crate::commands::install::InstallArgs;
			assert_eq!(
				Cli { command: Command::Install(InstallArgs { skip_confirm: false }) }.to_string(),
				"install"
			);

			// Install success.
			let (command, data) = simulate_command_flow(
				Command::Install(Default::default()),
				Ok(Install(common::Os::Linux)),
			);
			assert_eq!(command, "install");
			assert_eq!(data, "linux");

			// Install error
			let error = "install failed: download error";
			let (command, data) = simulate_command_flow(
				Command::Install(Default::default()),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "install");
			assert_eq!(data, error);
		}

		#[test]
		fn new_command() {
			use crate::{
				commands::new::{Command as NewCommand, NewArgs},
				common::Template,
			};

			assert_eq!(
				Cli {
					command: Command::New(NewArgs {
						command: NewCommand::Parachain(Default::default())
					})
				}
				.to_string(),
				"new chain"
			);
			// Test new with template.
			let (command, data) = simulate_command_flow(
				Command::New(NewArgs { command: NewCommand::Contract(Default::default()) }),
				Ok(New(Template::Contract(Default::default()))),
			);
			assert_eq!(command, "new contract");
			assert_eq!(data, "Standard");
			// New error
			let error = "new failed: template error";
			let (command, data) = simulate_command_flow(
				Command::New(NewArgs { command: NewCommand::Contract(Default::default()) }),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "new contract");
			assert_eq!(data, error);
		}

		#[test]
		fn bench_command() {
			use crate::commands::bench::{BenchmarkArgs, Command::Pallet};

			assert_eq!(
				Cli {
					command: Command::Bench(BenchmarkArgs { command: Pallet(Default::default()) })
				}
				.to_string(),
				"bench pallet"
			);
			// Bench error.
			let error = "bench failed: runtime error";
			let (command, data) = simulate_command_flow(
				Command::Bench(BenchmarkArgs { command: Pallet(Default::default()) }),
				Err(anyhow!(error)) as Result<common::Telemetry>,
			);
			assert_eq!(command, "bench pallet");
			assert_eq!(data, error);
		}
	}
}
