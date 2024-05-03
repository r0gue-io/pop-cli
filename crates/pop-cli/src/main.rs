// SPDX-License-Identifier: GPL-3.0

#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod style;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use cliclack::log;
use std::{fs::create_dir_all, path::PathBuf};
use tokio::task::JoinHandle;

#[derive(Parser)]
#[command(author, version, about, styles=style::get_styles())]
pub struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
#[command(subcommand_required = true)]
enum Commands {
	/// Generate a new parachain, pallet or smart contract.
	#[clap(alias = "n")]
	New(commands::new::NewArgs),
	/// Build a parachain or smart contract.
	#[clap(alias = "b")]
	Build(commands::build::BuildArgs),
	/// Call a smart contract.
	#[clap(alias = "c")]
	#[cfg(feature = "contract")]
	Call(commands::call::CallArgs),
	/// Deploy a parachain or smart contract.
	#[clap(alias = "u")]
	Up(commands::up::UpArgs),
	/// Test a smart contract.
	#[clap(alias = "t")]
	#[cfg(feature = "contract")]
	Test(commands::test::TestArgs),
}

fn init_config() -> Result<()> {
	match pop_telemetry::write_default_config() {
		Ok(maybe_path) => {
			if let Some(path) = maybe_path {
				log::info(format!("Initialized config file at {}", &path.to_str().unwrap()))?;
			}
		},
		Err(err) => {
			log::warning(format!(
				"Unable to initialize config file, continuing... {}",
				err.to_string()
			))?;
		},
	}
	Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
	init_config()?;

	// handle for await not used here as telemetry should complete before any of the commands do.
	tokio::spawn(pop_telemetry::record_cli_used());

	// If error occurs, this will be used to ensure error telemetry is complete before destructing.
	// This await handle is used as the error will not have sufficient time to report before destruction.
	let mut tel_error_handle: Option<JoinHandle<pop_telemetry::Result<()>>> = None;

	let cli = Cli::parse();
	let res = match cli.command {
		Commands::New(args) => Ok(match &args.command {
			#[cfg(feature = "parachain")]
			commands::new::NewCommands::Parachain(cmd) => cmd.execute().await.map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"new": "parachain"}),
				)));
				err
			})?,
			#[cfg(feature = "parachain")]
			commands::new::NewCommands::Pallet(cmd) => cmd.execute().await.map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"new": "pallet"}),
				)));
				err
			})?,
			#[cfg(feature = "contract")]
			commands::new::NewCommands::Contract(cmd) => cmd.execute().await.map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"new": "contract"}),
				)));
				err
			})?,
		}),
		Commands::Build(args) => match &args.command {
			#[cfg(feature = "parachain")]
			commands::build::BuildCommands::Parachain(cmd) => cmd.execute().map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"build": "parachain"}),
				)));
				err
			}),
			#[cfg(feature = "contract")]
			commands::build::BuildCommands::Contract(cmd) => cmd.execute().map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"build": "contract"}),
				)));
				err
			}),
		},
		#[cfg(feature = "contract")]
		Commands::Call(args) => Ok(match &args.command {
			commands::call::CallCommands::Contract(cmd) => cmd.execute().await.map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"call": "contract"}),
				)));
				err
			})?,
		}),
		Commands::Up(args) => Ok(match &args.command {
			#[cfg(feature = "parachain")]
			commands::up::UpCommands::Parachain(cmd) => cmd.execute().await.map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"up": "parachain"}),
				)));
				err
			})?,
			#[cfg(feature = "contract")]
			commands::up::UpCommands::Contract(cmd) => cmd.execute().await.map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"up": "contract"}),
				)));
				err
			})?,
		}),
		#[cfg(feature = "contract")]
		Commands::Test(args) => match &args.command {
			commands::test::TestCommands::Contract(cmd) => cmd.execute().map_err(|err| {
				tel_error_handle = Some(tokio::spawn(pop_telemetry::record_cli_command(
					"error",
					serde_json::json!({"test": "contract"}),
				)));
				err
			}),
		},
	};

	if let Some(handle) = tel_error_handle {
		let _ = handle.await;
	}

	res
}
#[cfg(feature = "parachain")]
fn cache() -> Result<PathBuf> {
	let cache_path = dirs::cache_dir()
		.ok_or(anyhow!("the cache directory could not be determined"))?
		.join("pop");
	// Creates pop dir if needed
	create_dir_all(cache_path.as_path())?;
	Ok(cache_path)
}
