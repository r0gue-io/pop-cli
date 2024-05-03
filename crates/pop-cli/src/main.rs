// SPDX-License-Identifier: GPL-3.0

#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod style;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use cliclack::log;
use commands::*;
use pop_telemetry::{record_cli_command, record_cli_used};
use serde_json::json;
use std::{fs::create_dir_all, path::PathBuf};
use tokio::{spawn, task::JoinHandle};

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

#[tokio::main]
async fn main() -> Result<()> {
	init_config()?;

	// handle for await not used here as telemetry should complete before any of the commands do.
	// Sends a generic ping saying the CLI was used
	spawn(record_cli_used());

	// type to represent static telemetry data. I.e., does not contain data dynamically chosen by user
	// like in pop new parachain.
	let mut tel_data: (&str, &str, &str) = ("", "", "");

	let cli = Cli::parse();
	let res = match cli.command {
		Commands::New(args) => Ok(match &args.command {
			#[cfg(feature = "parachain")]
			new::NewCommands::Parachain(cmd) => cmd.execute().await?,
			#[cfg(feature = "parachain")]
			new::NewCommands::Pallet(cmd) => {
				// when there are more pallet selections, this will likely have to move deeper into the stack
				tel_data = ("new", "pallet", "template");

				cmd.execute().await?
			},
			#[cfg(feature = "contract")]
			new::NewCommands::Contract(cmd) => {
				// When more contract selections are added this will likely need to go deeped in the stack
				tel_data = ("new", "contract", "default");

				cmd.execute().await?
			},
		}),
		Commands::Build(args) => match &args.command {
			#[cfg(feature = "parachain")]
			build::BuildCommands::Parachain(cmd) => {
				tel_data = ("build", "parachain", "");

				cmd.execute()
			},
			#[cfg(feature = "contract")]
			build::BuildCommands::Contract(cmd) => {
				tel_data = ("build", "contract", "");

				cmd.execute()
			},
		},
		#[cfg(feature = "contract")]
		Commands::Call(args) => Ok(match &args.command {
			call::CallCommands::Contract(cmd) => {
				tel_data = ("call", "contract", "");

				cmd.execute().await?
			},
		}),
		Commands::Up(args) => Ok(match &args.command {
			#[cfg(feature = "parachain")]
			up::UpCommands::Parachain(cmd) => {
				tel_data = ("up", "parachain", "");

				cmd.execute().await?
			},
			#[cfg(feature = "contract")]
			up::UpCommands::Contract(cmd) => {
				tel_data = ("up", "contract", "");

				cmd.execute().await?
			},
		}),
		#[cfg(feature = "contract")]
		Commands::Test(args) => match &args.command {
			test::TestCommands::Contract(cmd) => cmd.execute(),
		},
	};

	let tel_data_handle = spawn(record_cli_command(tel_data.0, json!({tel_data.1: tel_data.2})));
	// Best effort to send on first try, no action if failure
	let _ = tel_data_handle.await;
	// Send if error
	if res.is_err() {
		let _ = spawn(record_cli_command("error", json!({tel_data.0: tel_data.1}))).await;
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
