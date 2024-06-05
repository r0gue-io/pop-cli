// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::Args;
use cliclack::{
	clear_screen, confirm, intro, log, multi_progress, outro, outro_cancel, set_theme, ProgressBar,
	Theme as _, ThemeState,
};
use console::{Emoji, Style, Term};
use duct::cmd;
use pop_parachains::{Binary, Error, NetworkNode, Source, Status, Zombienet};
use std::{fs::remove_dir_all, path::PathBuf, time::Duration};
use tokio::time::sleep;

#[derive(Args)]
pub(crate) struct ZombienetCommand {
	/// The Zombienet network configuration file to be used.
	#[arg(short, long)]
	file: String,
	/// The version of Polkadot to be used for the relay chain, as per the release tag (e.g.
	/// "v1.11.0").
	#[arg(short, long)]
	relay_chain: Option<String>,
	/// The version of Polkadot to be used for a system parachain, as per the release tag (e.g.
	/// "v1.11.0"). Defaults to the relay chain version if not specified.
	#[arg(short, long)]
	system_parachain: Option<String>,
	/// The url of the git repository of a parachain to be used, with branch/release tag specified as #fragment (e.g. 'https://github.com/org/repository#tag').
	/// A specific binary name can also be optionally specified via query string parameter (e.g. 'https://github.com/org/repository?binaryname#tag'), defaulting to the name of the repository when not specified.
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
		intro(format!("{}: Launch a local network", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		// Parse arguments
		let cache = crate::cache()?;
		let mut zombienet = match Zombienet::new(
			cache.clone(),
			&self.file,
			self.relay_chain.as_ref(),
			self.system_parachain.as_ref(),
			self.parachain.as_ref(),
		)
		.await
		{
			Ok(n) => n,
			Err(e) => {
				return match e {
					Error::MissingBinary(name) => {
						outro_cancel(format!("🚫 The `{name}` binary is specified in the network configuration file, but cannot be resolved to a source. Are you missing a `--parachain` argument?"))?;
						Ok(())
					},
					_ => Err(e.into()),
				}
			},
		};

		// Check if any missing binaries
		let missing: Vec<_> = zombienet
			.missing_binaries()
			.into_iter()
			.filter_map(|b| match &b.source {
				Source::None | Source::Artifact => None,
				_ => Some(b),
			})
			.collect();
		if !missing.is_empty() {
			let list = style(format!(
				"> {}",
				missing.iter().map(|b| b.name.clone()).collect::<Vec<_>>().join(", ")
			))
			.dim()
			.to_string();
			log::warning(format!("⚠️ The following binaries specified in the network configuration file cannot be found locally:\n   {list}"))?;
			Self::source_binaries(missing, &cache, self.verbose).await?;
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
				outro_cancel(format!("🚫 Could not launch local network: {e}"))?;
			},
		}

		Ok(())
	}

	async fn source_binaries(
		missing: Vec<&Binary>,
		cache: &PathBuf,
		verbose: bool,
	) -> anyhow::Result<()> {
		// Prompt for automatic sourcing of binaries
		let list = style(format!(
			"> {}",
			missing
				.iter()
				.map(|binary| {
					let version = binary.version();
					if version != "" {
						format!("{} {}", binary.name, binary.version())
					} else {
						binary.name.clone()
					}
				})
				.collect::<Vec<_>>()
				.join(", ")
		))
		.dim()
		.to_string();
		if !confirm(format!(
			"📦 Would you like to source them automatically now? It may take some time...\n   {list}"))
		.initial_value(true)
		.interact()?
		{
			outro_cancel(
				"🚫 Cannot launch the specified network until all required binaries are available.",
			)?;
			return Ok(());
		}

		log::info(format!(
			"ℹ️ Binaries will be cached at {}",
			&cache.to_str().expect("expected local cache is invalid")
		))?;

		// Check for pre-existing working directory
		let working_dir = cache.join(".src");
		if working_dir.exists() && confirm(
			"📦 A previous working directory has been detected. Would you like to remove it now?",
		)
			.initial_value(true)
			.interact()? {
			remove_dir_all(&working_dir)?;
		}

		// Source binaries
		match verbose {
			true => {
				let reporter = VerboseReporter;
				for binary in missing {
					log::info(format!("📦 Sourcing {}...", binary.name))?;
					Term::stderr().clear_last_lines(1)?;
					if let Err(e) = binary.source(&working_dir, reporter, verbose).await {
						reporter.update(&format!("Sourcing failed: {e}"));
						outro_cancel(
							"🚫 Cannot launch the network until all required binaries are available.",
						)?;
						return Ok(());
					}
				}
				reporter.update("");
			},
			false => {
				let multi = multi_progress("📦 Sourcing binaries...".to_string());
				let queue: Vec<_> = missing
					.iter()
					.map(|binary| {
						let progress = multi.add(cliclack::spinner());
						progress.start(format!("{}: waiting...", binary.name));
						(binary, progress)
					})
					.collect();
				let mut error = false;
				for (binary, progress) in &queue {
					let prefix = format!("{}: ", binary.name);
					let progress_reporter = ProgressReporter(&prefix, &progress);
					if let Err(e) = binary.source(&working_dir, progress_reporter, verbose).await {
						progress.error(format!("🚫 {}: {e}", binary.name));
						error = true;
					}
					progress.stop(format!("✅  {}", binary.name));
				}
				multi.stop();
				if error {
					outro_cancel(
						"🚫 Cannot launch the network until all required binaries are available.",
					)?;
					return Ok(());
				}
			},
		};

		// Remove working directory once completed successfully
		if working_dir.exists() {
			remove_dir_all(working_dir)?
		}
		return Ok(());
	}
}

pub(crate) async fn run_custom_command(
	spinner: &ProgressBar,
	command: &str,
) -> Result<(), anyhow::Error> {
	spinner.set_message(format!("Spinning up network & running command: {}", command.to_string()));
	sleep(Duration::from_secs(15)).await;

	// Split the command into the base command and arguments
	let mut parts = command.split_whitespace();
	let base_command = parts.next().expect("Command cannot be empty");
	let args: Vec<&str> = parts.collect();

	cmd(base_command, &args)
		.run()
		.map_err(|e| anyhow::Error::new(e).context("Error running the command."))?;

	Ok(())
}

/// Reports any observed status updates to a progress bar.
#[derive(Copy, Clone)]
struct ProgressReporter<'a>(&'a str, &'a ProgressBar);

impl Status for ProgressReporter<'_> {
	fn update(&self, status: &str) {
		self.1
			.start(&format!("{}{}", self.0, status.replace("   Compiling", "Compiling")))
	}
}

/// Reports any observed status updates as indented messages.
#[derive(Copy, Clone)]
struct VerboseReporter;

impl Status for VerboseReporter {
	fn update(&self, status: &str) {
		const S_BAR: Emoji = Emoji("│", "|");
		let message = format!(
			"{bar}  {status}",
			bar = Theme.bar_color(&ThemeState::Submit).apply_to(S_BAR),
			status = style(status).dim()
		);
		if let Err(e) = Term::stderr().write_line(&message) {
			println!("An error occurred logging the status message of '{status}': {e}")
		}
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
