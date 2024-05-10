// SPDX-License-Identifier: GPL-3.0

#[cfg(not(any(feature = "contract", feature = "parachain")))]
compile_error!("feature \"contract\" or feature \"parachain\" must be enabled");

#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
mod style;

#[cfg(feature = "parachain")]
use anyhow::anyhow;
use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::*;
#[cfg(feature = "telemetry")]
use pop_telemetry::{config_file_path, record_cli_command, record_cli_used, Telemetry};
use serde_json::{json, Value};
#[cfg(feature = "parachain")]
use std::{fs::create_dir_all, path::PathBuf};

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
	#[cfg(any(feature = "parachain", feature = "contract"))]
	New(new::NewArgs),
	/// Build a parachain or smart contract.
	#[clap(alias = "b")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Build(build::BuildArgs),
	/// Call a smart contract.
	#[clap(alias = "c")]
	#[cfg(feature = "contract")]
	Call(call::CallArgs),
	/// Deploy a parachain or smart contract.
	#[clap(alias = "u")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Up(up::UpArgs),
	/// Test a smart contract.
	#[clap(alias = "t")]
	#[cfg(feature = "contract")]
	Test(test::TestArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "telemetry")]
	let maybe_tel = init().unwrap_or(None);

	let cli = Cli::parse();
	let res = match cli.command {
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::New(args) => match args.command {
			#[cfg(feature = "parachain")]
			new::NewCommands::Parachain(cmd) => match cmd.execute().await {
				Ok(template) => {
					// telemetry should never cause a panic or early exit
					Ok(json!({template.provider().unwrap_or("provider-missing"): template.name()}))
				},
				Err(e) => Err(e),
			},
			#[cfg(feature = "parachain")]
			new::NewCommands::Pallet(cmd) => {
				// When more contract selections are added the tel data will likely need to go deeper in the stack
				cmd.execute().await.map(|_| json!("template"))
			},
			#[cfg(feature = "contract")]
			new::NewCommands::Contract(cmd) => {
				// When more contract selections are added, the tel data will likely need to go deeper in the stack
				cmd.execute().await.map(|_| json!("default"))
			},
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::Build(args) => match &args.command {
			#[cfg(feature = "parachain")]
			build::BuildCommands::Parachain(cmd) => cmd.execute().map(|_| Value::Null),
			#[cfg(feature = "contract")]
			build::BuildCommands::Contract(cmd) => cmd.execute().map(|_| Value::Null),
		},
		#[cfg(feature = "contract")]
		Commands::Call(args) => match &args.command {
			call::CallCommands::Contract(cmd) => cmd.execute().await.map(|_| Value::Null),
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Commands::Up(args) => match &args.command {
			#[cfg(feature = "parachain")]
			up::UpCommands::Parachain(cmd) => cmd.execute().await.map(|_| Value::Null),
			#[cfg(feature = "contract")]
			up::UpCommands::Contract(cmd) => cmd.execute().await.map(|_| Value::Null),
		},
		#[cfg(feature = "contract")]
		Commands::Test(args) => match &args.command {
			test::TestCommands::Contract(cmd) => match cmd.execute() {
				Ok(feature) => Ok(json!(feature)),
				Err(e) => Err(e),
			},
		},
	};

	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel.clone() {
		// `args` is guaranteed to have at least 3 elements as clap will display help message if not set.
		let args: Vec<_> = std::env::args().collect();
		let command = args.get(1).expect("expected command missing");
		let subcommand = args.get(2).expect("expected sub-command missing");

		if let Ok(sub_data) = &res {
			// Best effort to send on first try, no action if failure.
			let _ =
				record_cli_command(tel.clone(), command, json!({subcommand: sub_data.to_string()}))
					.await;
		} else {
			let _ = record_cli_command(tel, "error", json!({command: subcommand})).await;
		}
	}

	// map result from Result<Value> to Result<()>
	res.map(|_| ())
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

#[cfg(feature = "telemetry")]
fn init() -> Result<Option<Telemetry>> {
	env_logger::init();
	let maybe_config_path = config_file_path();

	let maybe_tel = maybe_config_path.ok().map(|path| Telemetry::new(path));

	// Handle for await not used here as telemetry should complete before any of the commands do.
	// Sends a generic ping saying the CLI was used.
	if let Some(tel) = maybe_tel.clone() {
		tokio::spawn(record_cli_used(tel));
	}

	// if config file errors set telemetry to None, otherwise Some(tel)
	Ok(maybe_tel)
}

#[test]
fn verify_cli() {
	// https://docs.rs/clap/latest/clap/_derive/_tutorial/chapter_4/index.html
	use clap::CommandFactory;
	Cli::command().debug_assert()
}
