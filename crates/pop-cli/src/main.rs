// SPDX-License-Identifier: GPL-3.0

#[cfg(not(any(feature = "contract", feature = "parachain")))]
compile_error!("feature \"contract\" or feature \"parachain\" must be enabled");

use anyhow::{anyhow, Result};
use clap::Parser;
use commands::*;
use serde_json::json;
use std::{fs::create_dir_all, path::PathBuf};
#[cfg(feature = "telemetry")]
use {
	pop_telemetry::{config_file_path, record_cli_command, record_cli_used, Telemetry},
	std::env::args,
};

mod cli;
#[cfg(any(feature = "parachain", feature = "contract"))]
mod commands;
mod common;
mod style;
mod wallet_integration;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "telemetry")]
	let maybe_tel = init().unwrap_or(None);

	let cli = Cli::parse();
	let res = cli.command.execute().await;

	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel.clone() {
		// `args` is guaranteed to have at least 3 elements as clap will display help message if not
		// set.
		let (command, subcommand) = parse_args(args().collect());

		if let Ok(sub_data) = &res {
			// Best effort to send on first try, no action if failure.
			let _ = record_cli_command(
				tel.clone(),
				&command,
				json!({&subcommand: sub_data.to_string()}),
			)
			.await;
		} else {
			let _ = record_cli_command(tel, "error", json!({&command: &subcommand})).await;
		}
	}

	// map result from Result<Value> to Result<()>
	res.map(|_| ())
}

#[derive(Parser)]
#[command(author, version, about, styles=style::get_styles())]
pub struct Cli {
	#[command(subcommand)]
	command: Command,
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
	env_logger::init();
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

/// Parses command line arguments.
#[cfg(feature = "telemetry")]
fn parse_args(args: Vec<String>) -> (String, String) {
	// command is always present as clap will print help if not set
	let command = args.get(1).expect("expected command missing").to_string();
	// subcommand may not exist
	let subcommand = args.get(2).unwrap_or(&"".to_string()).to_string();
	(command.clone(), subcommand.clone())
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

	#[test]
	fn parse_args_works() {
		for args in vec![
			vec!["pop", "install"],
			vec!["pop", "new", "parachain"],
			vec!["pop", "new", "parachain", "extra"],
		] {
			// map args<&str> to args<String>
			let (command, subcommand) = parse_args(args.iter().map(|s| s.to_string()).collect());
			assert_eq!(command, args[1]);
			if args.len() > 2 {
				assert_eq!(subcommand, args[2]);
			}
		}
	}
}
