// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{
	clear_screen, confirm, intro, log, multi_progress, outro, outro_cancel, set_theme, ProgressBar,
};
use console::{Emoji, Style};
use duct::cmd;
use pop_parachains::{NetworkNode, Status, Zombienet};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Args)]
pub(crate) struct ZombienetCommand {
	/// The Zombienet configuration file to be used.
	#[arg(short, long)]
	file: String,
	/// The version of Polkadot to be used for the relay chain, as per the release tag (e.g.
	/// "v1.7.0").
	#[arg(short, long)]
	relay_chain: Option<String>,
	/// The version of Polkadot to be used for a system parachain, as per the release tag (e.g.
	/// "v1.7.0").
	#[arg(short, long)]
	system_parachain: Option<String>,
	/// The url of the git repository of a parachain to be used, with branch/release tag specified as #fragment (e.g. 'https://github.com/org/repository#tag'). A specific binary name can also be optionally specified via query string parameter (e.g. 'https://github.com/org/repository?binaryname#tag'), defaulting to the name of the repository when not specified.
	#[arg(short, long)]
	parachain: Option<Vec<String>>,
	/// The command to run after the network has been launched.
	#[clap(name = "cmd", short = 'c', long)]
	command: Option<String>,
	/// Whether the output should be verbose.
	#[arg(short, long, action)]
	verbose: bool,
}
impl ZombienetCommand {
	pub(crate) async fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Deploy a parachain", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		// Parse arguments
		let cache = crate::cache()?;
		let mut zombienet = Zombienet::new(
			cache.clone(),
			&self.file,
			self.relay_chain.as_ref(),
			self.system_parachain.as_ref(),
			self.parachain.as_ref(),
		)
		.await?;
		// Check if any binaries need to be sourced
		let missing = zombienet.missing_binaries();
		if missing.len() > 0 {
			log::warning(format!(
				"⚠️ The following missing binaries are required: {}",
				missing.iter().map(|b| b.name.as_str()).collect::<Vec<_>>().join(", ")
			))?;
			if !confirm("📦 Would you like to source them automatically now?")
				.initial_value(true)
				.interact()?
			{
				outro_cancel("🚫 Cannot deploy parachain to local network until all required binaries are available.")?;
				return Ok(());
			}
			log::info(format!("ℹ️ They will be cached at {}", &cache.to_str().unwrap()))?;
			// Source binaries
			for binary in missing {
				let multi = multi_progress(format!("📦 Sourcing {}...", binary.name));
				let progress = multi.add(cliclack::spinner());
				let progress_reporter = ProgressReporter(&progress);
				for attempt in (0..=1).rev() {
					if let Err(e) = binary.source(&cache, progress_reporter).await {
						match attempt {
							0 => {
								progress.error(format!("🚫 Sourcing failed: {e}"));
								multi.stop();
								return Ok(());
							},
							_ => {
								progress.error("🚫 Sourcing attempt failed, retrying...");
								sleep(Duration::from_secs(1)).await;
							},
						}
					}
				}
				progress.stop(format!("✅ Sourcing {} complete.", binary.name));
				multi.stop();
			}
		}
		// Finally spawn network and wait for signal to terminate
		let spinner = cliclack::spinner();
		spinner.start("🚀 Launching local network...");
		//tracing_subscriber::fmt().init();
		match zombienet.spawn().await {
			Ok(network) => {
				let mut result =
					"🚀 Network launched successfully - ctrl-c to terminate".to_string();
				let base_dir = network.base_dir().expect("base_dir expected to exist");
				let bar = Style::new().magenta().dim().apply_to(Emoji("│", "|"));

				let output = |node: &NetworkNode| -> String {
					let name = node.name();
					let mut output = format!(
						"\n{bar}       {name}:
{bar}         portal: https://polkadot.js.org/apps/?rpc={}#/explorer
{bar}         logs: tail -f {base_dir}/{name}/{name}.log",
						node.ws_uri(),
					);
					if self.verbose {
						output += &format!(
							"\n{bar}         command: {} {}",
							node.spec().command(),
							node.args().join(" ")
						);
					}
					output
				};
				// Add relay info
				let mut validators = network.relaychain().nodes();
				validators.sort_by_key(|n| n.name());
				result.push_str(&format!("\n{bar}  ⛓️ {}", network.relaychain().chain()));
				for node in validators {
					result.push_str(&output(node));
				}
				// Add parachain info
				let mut parachains = network.parachains();
				parachains.sort_by_key(|p| p.para_id());
				for parachain in network.parachains() {
					result.push_str(&format!(
						"\n{bar}  ⛓️ {}",
						parachain.chain_id().map_or(
							format!("para_id: {}", parachain.para_id()),
							|chain| format!("{chain}: {}", parachain.para_id())
						)
					));
					let mut collators = parachain.collators();
					collators.sort_by_key(|n| n.name());
					for node in collators {
						result.push_str(&output(node));
					}
				}

				if let Some(command) = &self.command {
					run_custom_command(&spinner, command).await?;
				}

				spinner.stop(result);
				tokio::signal::ctrl_c().await?;
				outro("Done")?;
			},
			Err(e) => {
				outro_cancel(format!("Could not spawn network: {e}"))?;
			},
		}

		Ok(())
	}
}

pub(crate) async fn run_custom_command(
	spinner: &ProgressBar,
	command: &str,
) -> Result<(), anyhow::Error> {
	spinner.set_message(format!("Running command: {}", command.to_string()));
	sleep(Duration::from_secs(15)).await;

	// Split the command into the base command and arguments
	let mut parts = command.split_whitespace();
	let base_command = parts.next().expect("Command cannot be empty");
	let args: Vec<&str> = parts.collect();

	cmd(base_command, &args)
		.run()
		.map_err(|e| anyhow::Error::new(e).context("Error running the command."))?;

	clear_screen()?;
	Ok(())
}

/// Reports any observed status updates to a progress bar.
#[derive(Copy, Clone)]
struct ProgressReporter<'a>(&'a ProgressBar);

impl Status for ProgressReporter<'_> {
	fn update(&self, status: &str) {
		self.0.start(status.replace("   Compiling", "Compiling"))
	}
}

// Write a test for run_custom_command
#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_run_custom_command() -> Result<(), anyhow::Error> {
		let spinner = ProgressBar::new(1);

		// Define the command to be executed
		let command = "echo 2 + 2";

		// Call the run_custom_command function
		run_custom_command(&spinner, command).await?;

		Ok(())
	}
}
