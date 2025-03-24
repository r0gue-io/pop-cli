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
	let res = cli.command.execute().await;

	#[cfg(feature = "telemetry")]
	if let Some(tel) = maybe_tel.clone() {
		// Record command and subcommand properly for telemetry
		match std::env::args().collect::<Vec<_>>().as_slice() {
            // Only try to use canonical command if we have enough args to parse
            [_, _cmd, ..] => {
                // Create a new command instance and parse from the original args
                #[allow(unused_imports)]
                use clap::CommandFactory;
                if let Ok(matches) = Cli::command().try_get_matches_from(std::env::args()) {
                    let canonical_command = get_canonical_command(&matches);
                    let _ = record_cli_command(
                        tel.clone(),
                        &canonical_command,
                        json!({}),
                    )
                    .await;
                } else {
                    // Fall back to legacy behavior
                    let (command, subcommand) = parse_args(args().collect());
                    if let Ok(sub_data) = &res {
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
            },
            // Not enough args, fall back to legacy behavior
            _ => {
                let (command, subcommand) = parse_args(args().collect());
                if let Ok(sub_data) = &res {
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

/// Parses command line arguments.
#[cfg(feature = "telemetry")]
fn parse_args(args: Vec<String>) -> (String, String) {
	// command is always present as clap will print help if not set
	let command = args.get(1).expect("expected command missing").to_string();
	// subcommand may not exist
	let subcommand = args.get(2).unwrap_or(&"".to_string()).to_string();
	(command.clone(), subcommand.clone())
}

#[cfg(feature = "telemetry")]
fn get_canonical_command(matches: &clap::ArgMatches) -> String {
    match matches.subcommand() {
        Some(("build", sub_matches)) | Some(("b", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("spec", _)) | Some(("s", _)) => "build_spec".to_string(),
                _ => "build".to_string(),
            }
        },
        Some(("test", sub_matches)) | Some(("t", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("contract", _)) | Some(("c", _)) => "test_contract".to_string(),
                _ => "test".to_string(),
            }
        },
        Some(("new", sub_matches)) | Some(("n", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("parachain", _)) => "new_parachain".to_string(),
                Some(("contract", _)) => "new_contract".to_string(),
                Some(("pallet", _)) => "new_pallet".to_string(),
                _ => "new".to_string(),
            }
        },
        Some(("up", sub_matches)) | Some(("u", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("network", _)) | Some(("n", _)) => "up_network".to_string(), 
                Some(("parachain", _)) | Some(("p", _)) => "up_parachain".to_string(),
                Some(("contract", _)) | Some(("c", _)) => "up_contract".to_string(),
                _ => "up".to_string(),
            }
        },
        Some(("call", sub_matches)) | Some(("c", sub_matches)) => {
            match sub_matches.subcommand() {
                Some(("chain", _)) | Some(("p", _)) | Some(("parachain", _)) => "call_chain".to_string(),
                Some(("contract", _)) | Some(("c", _)) => "call_contract".to_string(),
                _ => "call".to_string(),
            }
        },
        Some(("clean", _)) | Some(("C", _)) => "clean".to_string(),
        Some(("install", _)) | Some(("i", _)) => "install".to_string(),
        _ => "unknown".to_string(),
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
		use clap::{ArgMatches, CommandFactory};
		
		/// Test structure for command definitions
		struct CommandTest {
			/// Full command name
			command: &'static str,
			/// Command alias (shorthand)
			alias: &'static str,
			/// Expected canonical result for this command
			canonical: &'static str,
			/// Subcommands to test with this command
			subcommands: Vec<SubcommandTest>,
		}
		
		/// Test structure for subcommand definitions
		struct SubcommandTest {
			/// Full subcommand name
			subcommand: &'static str,
			/// Subcommand alias (shorthand)
			alias: &'static str,
			/// Expected canonical result for this command+subcommand combination
			canonical: &'static str,
		}
		
		// Define all commands and their subcommands to test
		let command_tests = vec![
			CommandTest {
				command: "build",
				alias: "b",
				canonical: "build",
				subcommands: vec![
					SubcommandTest {
						subcommand: "spec",
						alias: "s",
						canonical: "build_spec",
					},
				],
			},
			CommandTest {
				command: "new",
				alias: "n",
				canonical: "new",
				subcommands: vec![
					SubcommandTest {
						subcommand: "parachain",
						alias: "parachain", // No specific alias for parachain
						canonical: "new_parachain",
					},
					SubcommandTest {
						subcommand: "contract",
						alias: "contract", // No specific alias for contract
						canonical: "new_contract",
					},
					SubcommandTest {
						subcommand: "pallet",
						alias: "pallet", // No specific alias for pallet
						canonical: "new_pallet",
					},
				],
			},
			CommandTest {
				command: "call",
				alias: "c",
				canonical: "call",
				subcommands: vec![
					SubcommandTest {
						subcommand: "chain",
						alias: "p", // 'p' is an alias for chain
						canonical: "call_chain",
					},
					SubcommandTest {
						subcommand: "contract",
						alias: "c",
						canonical: "call_contract",
					},
				],
			},
			CommandTest {
				command: "test",
				alias: "t",
				canonical: "test",
				subcommands: vec![
					SubcommandTest {
						subcommand: "contract",
						alias: "c",
						canonical: "test_contract",
					},
				],
			},
			CommandTest {
				command: "up",
				alias: "u",
				canonical: "up",
				subcommands: vec![
					SubcommandTest {
						subcommand: "network",
						alias: "n",
						canonical: "up_network",
					},
					SubcommandTest {
						subcommand: "parachain",
						alias: "p",
						canonical: "up_parachain",
					},
					SubcommandTest {
						subcommand: "contract",
						alias: "c",
						canonical: "up_contract",
					},
				],
			},
			CommandTest {
				command: "clean",
				alias: "C",
				canonical: "clean",
				subcommands: vec![],
			},
			CommandTest {
				command: "install",
				alias: "i",
				canonical: "install",
				subcommands: vec![],
			},
		];
		
		/// Helper function to create ArgMatches for a given command sequence
		fn get_matches(args: &[&str]) -> Option<ArgMatches> {
			let mut full_args = vec!["pop"];
			full_args.extend(args);
			
			match Cli::command().try_get_matches_from(&full_args) {
				Ok(matches) => Some(matches),
				Err(_) => None,
			}
		}
		
		// Add dummy required arguments for commands that need them
		let dummy_args = &["--dummy-required-arg", "value"];
		
		// Test all commands and their aliases
		for test in &command_tests {
			// Main command test
			if let Some(matches) = get_matches(&[test.command]) {
				let result = get_canonical_command(&matches);
				assert_eq!(result, test.canonical);
			}
			
			// Command alias test
			if let Some(matches) = get_matches(&[test.alias]) {
				let result = get_canonical_command(&matches);
				assert_eq!(result, test.canonical);
			}
			
			// Test all subcommands
			for subtest in &test.subcommands {
				// Test combinations:
				// 1. Command + Subcommand
				if let Some(matches) = get_matches(&[test.command, subtest.subcommand]) {
					let result = get_canonical_command(&matches);
					assert_eq!(result, subtest.canonical);
				} else if let Some(matches) = get_matches(&[test.command, subtest.subcommand, dummy_args[0], dummy_args[1]]) {
					// Try with dummy args for commands requiring them
					let result = get_canonical_command(&matches);
					assert_eq!(result, subtest.canonical);
				}
				
				// 2. Command + Subcommand alias
				if subtest.subcommand != subtest.alias { // Only test if there's a real alias
					if let Some(matches) = get_matches(&[test.command, subtest.alias]) {
						let result = get_canonical_command(&matches);
						assert_eq!(result, subtest.canonical);
					} else if let Some(matches) = get_matches(&[test.command, subtest.alias, dummy_args[0], dummy_args[1]]) {
						let result = get_canonical_command(&matches);
						assert_eq!(result, subtest.canonical);
					}
				}
				
				// 3. Command alias + Subcommand
				if let Some(matches) = get_matches(&[test.alias, subtest.subcommand]) {
					let result = get_canonical_command(&matches);
					assert_eq!(result, subtest.canonical);
				} else if let Some(matches) = get_matches(&[test.alias, subtest.subcommand, dummy_args[0], dummy_args[1]]) {
					let result = get_canonical_command(&matches);
					assert_eq!(result, subtest.canonical);
				}
				
				// 4. Command alias + Subcommand alias
				if subtest.subcommand != subtest.alias { // Only test if there's a real alias
					if let Some(matches) = get_matches(&[test.alias, subtest.alias]) {
						let result = get_canonical_command(&matches);
						assert_eq!(result, subtest.canonical);
					} else if let Some(matches) = get_matches(&[test.alias, subtest.alias, dummy_args[0], dummy_args[1]]) {
						let result = get_canonical_command(&matches);
						assert_eq!(result, subtest.canonical);
					}
				}
			}
		}
	}

	#[cfg(feature = "telemetry")]
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
