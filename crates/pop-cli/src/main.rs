// SPDX-License-Identifier: GPL-3.0

#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod style;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use cliclack::log;
use commands::*;
use pop_telemetry::{config_file_path, record_cli_command, record_cli_used, Telemetry};
use serde_json::{json, Value};
use std::{fs::create_dir_all, path::PathBuf};
use tokio::spawn;

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
	env_logger::init();
	init_config()?;
	// environment variable `POP_TELEMETRY_ENDPOINT` is evaluated at compile time
	let endpoint =
		option_env!("POP_TELEMETRY_ENDPOINT").unwrap_or("http://127.0.0.1:3000/api/send");

	let maybe_config_file = config_file_path();
	// if config file errors set telemetry to None, otherwise Some(tel)
	let maybe_tel = maybe_config_file.ok().map(|path| Telemetry::new(endpoint.to_string(), path));

	// handle for await not used here as telemetry should complete before any of the commands do.
	// Sends a generic ping saying the CLI was used
	spawn(record_cli_used(maybe_tel.clone()));

	// type to represent static telemetry data. I.e., does not contain data dynamically chosen by user
	// like in pop new parachain.
	let mut tel_data: (&str, &str, Value) = ("", "", json!(""));

	let cli = Cli::parse();
	let res = match cli.command {
		Commands::New(args) => match args.command {
			#[cfg(feature = "parachain")]
			new::NewCommands::Parachain(cmd) => match cmd.execute().await {
				Ok(template) => {
					// telemetry should never cause a panic or early exit
					tel_data = (
						"new",
						"parachain",
						json!({template.provider().unwrap_or("provider-missing"): template.name()}),
					);
					Ok(())
				},
				Err(e) => Err(e),
			},
			#[cfg(feature = "parachain")]
			new::NewCommands::Pallet(cmd) => {
				// when there are more pallet selections, this will likely have to move deeper into the stack
				tel_data = ("new", "pallet", json!("template"));

				cmd.execute().await
			},
			#[cfg(feature = "contract")]
			new::NewCommands::Contract(cmd) => {
				// When more contract selections are added this will likely need to go deeper in the stack
				tel_data = ("new", "contract", json!("default"));

				cmd.execute().await
			},
		},
		Commands::Build(args) => match &args.command {
			#[cfg(feature = "parachain")]
			build::BuildCommands::Parachain(cmd) => {
				tel_data = ("build", "parachain", json!(""));

				cmd.execute()
			},
			#[cfg(feature = "contract")]
			build::BuildCommands::Contract(cmd) => {
				tel_data = ("build", "contract", json!(""));

				cmd.execute()
			},
		},
		#[cfg(feature = "contract")]
		Commands::Call(args) => match &args.command {
			call::CallCommands::Contract(cmd) => {
				tel_data = ("call", "contract", json!(""));

				cmd.execute().await
			},
		},
		Commands::Up(args) => match &args.command {
			#[cfg(feature = "parachain")]
			up::UpCommands::Parachain(cmd) => {
				tel_data = ("up", "parachain", json!(""));

				cmd.execute().await
			},
			#[cfg(feature = "contract")]
			up::UpCommands::Contract(cmd) => {
				tel_data = ("up", "contract", json!(""));

				cmd.execute().await
			},
		},
		#[cfg(feature = "contract")]
		Commands::Test(args) => match &args.command {
			test::TestCommands::Contract(cmd) => match cmd.execute() {
				Ok(feature) => {
					tel_data = ("test", "contract", json!(feature));
					Ok(())
				},
				Err(e) => Err(e),
			},
		},
	};

	// Best effort to send on first try, no action if failure
	let _ =
		record_cli_command(maybe_tel.clone(), tel_data.0, json!({tel_data.1: tel_data.2})).await;

	// Send if error
	if res.is_err() {
		let _ =
			record_cli_command(maybe_tel.clone(), "error", json!({tel_data.0: tel_data.1})).await;
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
	let path = config_file_path()?;
	match pop_telemetry::write_default_config(&path) {
		Ok(written) => {
			if written {
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

#[test]
fn verify_cli() {
	// https://docs.rs/clap/latest/clap/_derive/_tutorial/chapter_4/index.html
	use clap::CommandFactory;
	Cli::command().debug_assert()
}
