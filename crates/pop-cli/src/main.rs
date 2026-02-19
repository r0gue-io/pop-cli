// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]

use anyhow::{Result, anyhow};
use clap::Parser;
use commands::*;
use output::{
	CliError, CliResponse, ErrorCode, OutputMode, PromptRequiredError, UnsupportedJsonError,
};
#[cfg(feature = "telemetry")]
use pop_telemetry::{Telemetry, config_file_path, record_cli_command, record_cli_used};
use std::{
	fmt::{self, Display, Formatter},
	fs::create_dir_all,
	path::PathBuf,
};

mod cli;
mod commands;
mod common;
#[cfg(feature = "chain")]
mod deployment_api;
pub(crate) mod output;
mod style;
#[cfg(feature = "wallet-integration")]
mod wallet_integration;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "telemetry")]
	let maybe_tel = init().unwrap_or(None);

	// Detect --json from argv before parsing so clap errors can be wrapped.
	let json_requested = std::env::args().take_while(|a| a != "--").any(|a| a == "--json");
	let output_mode = if json_requested { OutputMode::Json } else { OutputMode::Human };

	let mut cli = match Cli::try_parse() {
		Ok(cli) => cli,
		Err(e) => {
			if output_mode == OutputMode::Json {
				let code = match e.kind() {
					clap::error::ErrorKind::InvalidValue |
					clap::error::ErrorKind::UnknownArgument |
					clap::error::ErrorKind::InvalidSubcommand |
					clap::error::ErrorKind::MissingRequiredArgument |
					clap::error::ErrorKind::NoEquals |
					clap::error::ErrorKind::ValueValidation |
					clap::error::ErrorKind::WrongNumberOfValues |
					clap::error::ErrorKind::TooFewValues |
					clap::error::ErrorKind::TooManyValues |
					clap::error::ErrorKind::MissingSubcommand => ErrorCode::InvalidInput,
					_ => ErrorCode::Internal,
				};
				CliResponse::err(CliError::new(code, e.to_string())).print_json_err();
				std::process::exit(2);
			}
			e.exit();
		},
	};
	#[cfg(feature = "telemetry")]
	let event = cli.command.to_string();
	let result = cli.command.execute(output_mode).await;
	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel {
		let data = serde_json::json!(cli.command);
		// Best effort to send on first try, no action if failure.
		let _ = record_cli_command(tel, &event, data).await;
	}
	match result {
		Ok(_) => Ok(()),
		Err(e) => {
			if output_mode == OutputMode::Json {
				let code = if e.downcast_ref::<UnsupportedJsonError>().is_some() {
					ErrorCode::UnsupportedJson
				} else if e.downcast_ref::<PromptRequiredError>().is_some() {
					ErrorCode::PromptRequired
				} else {
					ErrorCode::Internal
				};
				CliResponse::err(CliError::new(code, e.to_string())).print_json_err();
				std::process::exit(1);
			}
			Err(e)
		},
	}
}

/// An all-in-one tool for Polkadot development.
#[derive(Parser)]
#[command(author, version, about, styles=style::get_styles())]
pub struct Cli {
	#[command(subcommand)]
	command: Command,
	/// Emit output as a JSON envelope (for scripting, CI, and MCP).
	#[arg(long, global = true)]
	json: bool,
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

		#[test]
		fn build_command() {
			use crate::commands::build::Command as BuildCommand;
			// Build command display.
			assert_eq!(
				Cli { json: false, command: Command::Build(Default::default()) }.to_string(),
				"build"
			);
			// Build command with spec subcommand.
			assert_eq!(
				Cli {
					json: false,
					command: Command::Build(build::BuildArgs {
						command: Some(BuildCommand::Spec(Default::default())),
						..Default::default()
					})
				}
				.to_string(),
				"build spec"
			);
		}

		#[test]
		fn up_command() {
			// Up command display.
			assert_eq!(
				Cli { json: false, command: Command::Up(Default::default()) }.to_string(),
				"up"
			);
		}

		#[test]
		fn clean_command() {
			use clean::{CleanArgs, CleanCommandArgs, Command as CleanCommand};
			// Clean command display.
			assert_eq!(
				Cli {
					json: false,
					command: Command::Clean(CleanArgs {
						command: CleanCommand::Cache(CleanCommandArgs { all: false, pid: None }),
					})
				}
				.to_string(),
				"clean"
			);
		}

		#[test]
		fn install_command() {
			// Install command display.
			assert_eq!(
				Cli { json: false, command: Command::Install(Default::default()) }.to_string(),
				"install"
			);
		}

		#[test]
		fn new_command() {
			use crate::commands::new::{Command as NewCommand, NewArgs};
			// New command display.
			assert_eq!(
				Cli {
					json: false,
					command: Command::New(NewArgs {
						command: Some(NewCommand::Chain(Default::default())),
						list: false,
					})
				}
				.to_string(),
				"new chain"
			);
			// New command display without subcommand.
			assert_eq!(
				Cli { json: false, command: Command::New(NewArgs { command: None, list: false }) }
					.to_string(),
				"new"
			);
		}

		#[test]
		fn bench_command() {
			use crate::commands::bench::{BenchmarkArgs, Command::Pallet};
			// Bench command display.
			assert_eq!(
				Cli {
					json: false,
					command: Command::Bench(BenchmarkArgs { command: Pallet(Default::default()) })
				}
				.to_string(),
				"bench pallet"
			);
		}

		#[test]
		fn call_command() {
			use crate::commands::call::{CallArgs, Command as CallCommand};
			// Call chain command display.
			assert_eq!(
				Cli {
					json: false,
					command: Command::Call(CallArgs {
						command: Some(CallCommand::Chain(Default::default()))
					})
				}
				.to_string(),
				"call chain"
			);
			// Call contract command display.
			assert_eq!(
				Cli {
					json: false,
					command: Command::Call(CallArgs {
						command: Some(CallCommand::Contract(Default::default()))
					})
				}
				.to_string(),
				"call contract"
			);
		}
	}
}
