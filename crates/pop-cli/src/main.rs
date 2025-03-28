// SPDX-License-Identifier: GPL-3.0

#[cfg(not(any(feature = "contract", feature = "parachain")))]
compile_error!("feature \"contract\" or feature \"parachain\" must be enabled");

use anyhow::{anyhow, Result};
use clap::Parser;
use commands::*;
#[cfg(feature = "telemetry")]
use pop_telemetry::{config_file_path, record_cli_command, record_cli_used, Telemetry};
use serde_json::json;
use std::{fs::create_dir_all, path::PathBuf};

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

	// Get the canonical command name before executing the command
	#[cfg(feature = "telemetry")]
	let canonical_command = get_canonical_command(&cli);

	let res = cli.command.execute().await;

	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel.clone() {
		// Record result
		if let Ok(sub_data) = &res {
			let _ = record_cli_command(
				tel.clone(),
				&canonical_command,
				json!({"result": sub_data.to_string()}),
			)
			.await;
		} else {
			let _ = record_cli_command(
				tel,
				&canonical_command,
				json!({"error": "command execution failed"}),
			)
			.await;
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

#[cfg(feature = "telemetry")]
fn get_canonical_command(cli: &Cli) -> String {
	match &cli.command {
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Command::Build(args) => match &args.command {
			Some(build::Command::Spec(_)) => "build_spec".to_string(),
			None => "build".to_string(),
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Command::Test(args) => match &args.command {
			#[cfg(feature = "contract")]
			Some(test::Command::Contract(_)) => "test_contract".to_string(),
			None => "test".to_string(),
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Command::New(args) => match &args.command {
			#[cfg(feature = "parachain")]
			new::Command::Parachain(_) => "new_parachain".to_string(),
			#[cfg(feature = "contract")]
			new::Command::Contract(_) => "new_contract".to_string(),
			#[cfg(feature = "parachain")]
			new::Command::Pallet(_) => "new_pallet".to_string(),
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Command::Up(args) => match &args.command {
			Some(up::Command::Network(_)) => "up_network".to_string(),
			#[cfg(feature = "parachain")]
			Some(up::Command::Parachain(_)) => "up_parachain".to_string(),
			#[cfg(feature = "contract")]
			Some(up::Command::Contract(_)) => "up_contract".to_string(),
			None => "up".to_string(),
		},
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Command::Call(args) => match &args.command {
			#[cfg(feature = "parachain")]
			call::Command::Chain(_) => "call_chain".to_string(),
			#[cfg(feature = "contract")]
			call::Command::Contract(_) => "call_contract".to_string(),
		},
		Command::Clean(_) => "clean".to_string(),
		#[cfg(any(feature = "parachain", feature = "contract"))]
		Command::Install(_) => "install".to_string(),
	}
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
	#[test]
	fn test_get_canonical_command() {
		use clap::Parser;

		/// Helper function to parse command line arguments into a Cli struct
		fn parse_cli(args: &[&str]) -> Option<Cli> {
			let mut full_args = vec!["pop"];
			full_args.extend(args);

			match Cli::try_parse_from(&full_args) {
				Ok(cli) => Some(cli),
				Err(_) => None,
			}
		}

		// Command tests mapping
		let test_cases = [
			// Build commands
			(vec!["build"], "build"),
			(vec!["b"], "build"),
			(vec!["build", "spec"], "build_spec"),
			(vec!["b", "spec"], "build_spec"),
			(vec!["build", "s"], "build_spec"),
			(vec!["b", "s"], "build_spec"),
			// Test commands
			(vec!["test"], "test"),
			(vec!["t"], "test"),
			(vec!["test", "contract"], "test_contract"),
			(vec!["t", "contract"], "test_contract"),
			(vec!["test", "c"], "test_contract"),
			(vec!["t", "c"], "test_contract"),
			// New commands
			(vec!["new"], "new"),
			(vec!["n"], "new"),
			(vec!["new", "parachain"], "new_parachain"),
			(vec!["n", "parachain"], "new_parachain"),
			(vec!["new", "contract"], "new_contract"),
			(vec!["n", "contract"], "new_contract"),
			(vec!["new", "pallet"], "new_pallet"),
			(vec!["n", "pallet"], "new_pallet"),
			// Up commands
			(vec!["up"], "up"),
			(vec!["u"], "up"),
			(vec!["up", "network"], "up_network"),
			(vec!["u", "network"], "up_network"),
			(vec!["up", "n"], "up_network"),
			(vec!["u", "n"], "up_network"),
			(vec!["up", "parachain"], "up_parachain"),
			(vec!["u", "parachain"], "up_parachain"),
			(vec!["up", "p"], "up_parachain"),
			(vec!["u", "p"], "up_parachain"),
			(vec!["up", "contract"], "up_contract"),
			(vec!["u", "contract"], "up_contract"),
			(vec!["up", "c"], "up_contract"),
			(vec!["u", "c"], "up_contract"),
			// Call commands
			(vec!["call"], "call"),
			(vec!["c"], "call"),
			(vec!["call", "chain"], "call_chain"),
			(vec!["c", "chain"], "call_chain"),
			(vec!["call", "p"], "call_chain"),
			(vec!["c", "p"], "call_chain"),
			(vec!["call", "parachain"], "call_chain"),
			(vec!["c", "parachain"], "call_chain"),
			(vec!["call", "contract"], "call_contract"),
			(vec!["c", "contract"], "call_contract"),
			(vec!["call", "c"], "call_contract"),
			(vec!["c", "c"], "call_contract"),
			// Clean and Install commands
			(vec!["clean"], "clean"),
			(vec!["C"], "clean"),
			(vec!["install"], "install"),
			(vec!["i"], "install"),
		];

		// Test each command with all possible variations
		for (args, expected_canonical) in &test_cases {
			if let Some(cli) = parse_cli(args) {
				assert_eq!(
					get_canonical_command(&cli),
					*expected_canonical,
					"Command '{}' should return canonical name '{}'",
					args.join(" "),
					expected_canonical
				);
			} else {
				// Some commands may require extra arguments, which is fine
				// We can skip those for this test
				println!(
					"Skipping command '{}' as it requires additional arguments",
					args.join(" ")
				);
			}
		}
	}
}
