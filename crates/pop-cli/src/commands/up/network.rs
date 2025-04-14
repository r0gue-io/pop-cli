// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use clap::{
	builder::{PossibleValue, PossibleValuesParser, StringValueParser, TypedValueParser},
	error::ErrorKind,
	Arg, Args, Command,
};
use cliclack::{
	clear_screen, confirm, intro, log, multi_progress, outro, outro_cancel, set_theme, spinner,
	ProgressBar, Theme as _, ThemeState,
};
use console::{Emoji, Style, Term};
use duct::cmd;
use pop_common::Status;
pub(crate) use pop_parachains::up::Relay;
use pop_parachains::{
	clear_dmpq,
	up::{parachains::Parachain, NetworkConfiguration, Zombienet},
	Error, IndexSet, NetworkNode, RelayChain,
};
use std::{
	ffi::OsStr,
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::time::sleep;

/// Launch a local network by specifying a network configuration file.
#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct ConfigFileCommand {
	/// [DEPRECATED] The Zombienet network configuration file to be used (will be removed in
	/// v0.9.0).
	#[arg(short = 'f', long = "file")]
	#[deprecated(since = "0.8.0", note = "will be removed in v0.9.0")]
	#[allow(rustdoc::broken_intra_doc_links)]
	file: Option<PathBuf>,
	/// The version of the binary to be used for the relay chain, as per the release tag (e.g.
	/// "stable2503"). See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	relay_chain: Option<String>,
	/// The version of the runtime to be used for the relay chain, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'R', long)]
	relay_chain_runtime: Option<String>,
	/// The version of the binary to be used for system parachains, as per the release tag (e.g.
	/// "stable2503"). Defaults to the relay chain version if not specified.
	/// See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	system_parachain: Option<String>,
	/// The version of the runtime to be used for system parachains, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'S', long)]
	system_parachain_runtime: Option<String>,
	/// The url of the git repository of a parachain to be used, with branch/release tag/commit specified as #fragment (e.g. <https://github.com/org/repository#ref>).
	/// A specific binary name can also be optionally specified via query string parameter (e.g. <https://github.com/org/repository?binaryname#ref>), defaulting to the name of the repository when not specified.
	#[arg(short, long)]
	parachain: Option<Vec<String>>,
	/// The command to run after the network has been launched.
	#[clap(name = "cmd", short, long)]
	command: Option<String>,
	/// Whether the output should be verbose.
	#[arg(short, long, action)]
	verbose: bool,
	/// Automatically source all needed binaries required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl ConfigFileCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, path: Option<&Path>) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Launch a local network", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		// Show warning if file argument provided.
		#[allow(deprecated)]
		if self.file.is_some() {
			log::warning(
				"DEPRECATION: Please use `pop up network [PATH]` (or simply `pop u n [PATH]`) in the future...",
			)?;
		}

		// Determine network config from args.
		let network_config = match path {
			Some(path) => path,
			#[allow(deprecated)]
			None => match self.file.as_deref() {
				None => {
					outro_cancel("🚫 A network configuration file must be specified. See `pop up network --help` for usage.".to_string())?;
					return Ok(())
				},
				Some(path) => path,
			},
		};

		spawn(
			network_config.try_into()?,
			self.relay_chain.as_deref(),
			self.relay_chain_runtime.as_deref(),
			self.system_parachain.as_deref(),
			self.system_parachain_runtime.as_deref(),
			self.parachain.as_ref(),
			self.verbose,
			self.skip_confirm,
			self.command.as_deref(),
		)
		.await
	}
}

/// Launch a local network with supported parachains.
#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct BuildCommand {
	/// The version of the binary to be used for the relay chain, as per the release tag (e.g.
	/// "stable2503"). See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	relay_chain: Option<String>,
	/// The version of the runtime to be used for the relay chain, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'R', long)]
	relay_chain_runtime: Option<String>,
	/// The version of the binary to be used for system parachains, as per the release tag (e.g.
	/// "stable2503"). Defaults to the relay chain version if not specified.
	/// See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	system_parachain: Option<String>,
	/// The version of the runtime to be used for system parachains, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'S', long)]
	system_parachain_runtime: Option<String>,
	/// The parachain(s) to be included. An optional parachain identifier can be affixed via :id
	/// (e.g. `asset-hub:1000`).
	#[arg(short, long, value_delimiter = ',', value_parser = SupportedParachains::new())]
	parachain: Option<Vec<Parachain>>,
	/// The command to run after the network has been launched.
	#[clap(name = "cmd", short, long)]
	command: Option<String>,
	/// Whether the output should be verbose.
	#[arg(short, long, action)]
	verbose: bool,
	/// Automatically source all needed binaries required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl BuildCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, relay: Relay) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!(
			"{}: Launch a local {} network",
			style(" Pop CLI ").black().on_magenta(),
			relay.name()
		))?;
		set_theme(Theme);

		let network_config = NetworkConfiguration::build(relay, self.parachain.as_deref())?;

		spawn(
			network_config,
			self.relay_chain.as_deref(),
			self.relay_chain_runtime.as_deref(),
			self.system_parachain.as_deref(),
			self.system_parachain_runtime.as_deref(),
			None,
			self.verbose,
			self.skip_confirm,
			self.command.as_deref(),
		)
		.await
	}
}

#[derive(Clone)]
struct SupportedParachains(PossibleValuesParser);

impl SupportedParachains {
	fn new() -> Self {
		Self(PossibleValuesParser::new(
			Parachain::supported().map(|p| PossibleValue::new(p.name().to_string())),
		))
	}
}

impl TypedValueParser for SupportedParachains {
	type Value = Parachain;

	fn parse_ref(
		&self,
		cmd: &Command,
		arg: Option<&Arg>,
		value: &OsStr,
	) -> Result<Self::Value, clap::Error> {
		// Parse value as string with optional para id specifier
		let (value, id) = match self.0.parse_ref(cmd, arg, value) {
			Ok(value) => (value, None),
			// Check if failure due to para id being specified
			Err(e) if e.kind() == ErrorKind::InvalidValue => {
				let value = StringValueParser::new().parse_ref(cmd, arg, value)?;
				if !value.contains(":") {
					return Err(e)
				}
				let mut split = value.split(":");
				(
					split.next().expect("checked above").into(),
					split.next().and_then(|v| v.parse::<u32>().ok()),
				)
			},
			Err(e) => return Err(e),
		};

		// Check if system chain
		let supported = Parachain::supported();
		if let Some(para) = supported.iter().find_map(|p| match p {
			Parachain::System { id: default_id, chain } if chain == value.as_str() => {
				// Override id if specified
				Some(Parachain::System { id: id.unwrap_or(*default_id), chain: value.clone() })
			},
			_ => None,
		}) {
			return Ok(para);
		}

		// Otherwise attempt to parse from supported parachains
		supported
			.into_iter()
			.find(|p| p.as_ref() == value.as_str())
			.ok_or(clap::Error::new(ErrorKind::InvalidValue).with_cmd(cmd))
	}

	fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
		self.0.possible_values()
	}
}

/// Executes the command.
pub(crate) async fn spawn(
	config: NetworkConfiguration,
	relay_chain_version: Option<&str>,
	relay_chain_runtime_version: Option<&str>,
	system_parachain_version: Option<&str>,
	system_parachain_runtime_version: Option<&str>,
	parachains: Option<&Vec<String>>,
	verbose: bool,
	skip_confirm: bool,
	command: Option<&str>,
) -> anyhow::Result<()> {
	// Initialize from arguments
	let cache = crate::cache()?;
	let network_config = config.try_into()?;
	let mut zombienet = match Zombienet::new(
		&cache,
		network_config,
		relay_chain_version,
		relay_chain_runtime_version,
		system_parachain_version,
		system_parachain_runtime_version,
		parachains,
	)
	.await
	{
		Ok(n) => n,
		Err(e) =>
			return match e {
				Error::Config(message) => {
					outro_cancel(format!("🚫 A configuration error occurred: `{message}`"))?;
					Ok(())
				},
				Error::MissingBinary(name) => {
					outro_cancel(format!("🚫 The `{name}` binary is specified in the network configuration file, but cannot be resolved to a source. Are you missing a `--parachain` argument?"))?;
					Ok(())
				},
				_ => Err(e.into()),
			},
	};

	// Source any missing/stale binaries
	if source_binaries(&mut zombienet, &cache, verbose, skip_confirm).await? {
		return Ok(());
	}

	// Finally spawn network and wait for signal to terminate
	let progress = spinner();
	progress.start("🚀 Launching local network...");
	match zombienet.spawn().await {
		Ok(network) => {
			let mut result = "🚀 Network launched successfully - ctrl-c to terminate".to_string();
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
				if verbose {
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
			for parachain in parachains {
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

			if let Some(command) = command {
				run_custom_command(&progress, command).await?;
			}

			progress.stop(result);

			// Check for any specified channels
			if zombienet.hrmp_channels() {
				let relay_chain = zombienet.relay_chain();
				match RelayChain::from(relay_chain) {
					None => {
						log::error(format!("🚫 Using `{relay_chain}` with HRMP channels is currently unsupported. Please use `paseo-local` or `westend-local`."))?;
					},
					Some(_) => {
						let progress = spinner();
						progress.start("Connecting to relay chain to prepare channels...");
						// Allow relay node time to start
						sleep(Duration::from_secs(10)).await;
						progress.set_message("Preparing channels...");
						let relay_endpoint = network.relaychain().nodes()[0].wait_client().await?;
						let para_ids: Vec<_> =
							network.parachains().iter().map(|p| p.para_id()).collect();
						tokio::spawn(async move {
							if let Err(e) = clear_dmpq(relay_endpoint, &para_ids).await {
								progress.stop(format!("🚫 Could not prepare channels: {e}"));
								return Ok::<(), Error>(());
							}
							progress.stop("Channels successfully prepared for initialization.");
							Ok::<(), Error>(())
						});
					},
				}
			}

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
	zombienet: &mut Zombienet,
	cache: &Path,
	verbose: bool,
	skip_confirm: bool,
) -> anyhow::Result<bool> {
	// Check for any missing or stale binaries
	let binaries: Vec<_> = zombienet.binaries().filter(|b| !b.exists() || b.stale()).collect();
	if binaries.is_empty() {
		return Ok(false);
	}

	// Check if any missing binaries
	let missing: IndexSet<_> = binaries
		.iter()
		.filter_map(|b| (!b.exists()).then_some((b.name(), b.version())))
		.collect();
	if !missing.is_empty() {
		let list = style(format!(
			"> {}",
			missing.iter().map(|(name, _)| name.to_string()).collect::<Vec<_>>().join(", ")
		))
		.dim()
		.to_string();
		log::warning(format!("⚠️ The following binaries required to launch the network cannot be found locally:\n   {list}"))?;

		// Prompt for automatic sourcing of binaries
		let list = style(format!(
			"> {}",
			missing
				.iter()
				.map(|(name, version)| {
					if let Some(version) = version {
						format!("{name} {version}")
					} else {
						name.to_string()
					}
				})
				.collect::<Vec<_>>()
				.join(", ")
		))
		.dim()
		.to_string();
		if !skip_confirm &&
			!confirm(format!(
				"📦 Would you like to source them automatically now? It may take some time...\n   {list}"))
			.initial_value(true)
			.interact()?
		{
			outro_cancel(
				"🚫 Cannot launch the specified network until all required binaries are available.",
			)?;
			return Ok(true);
		}
	}

	// Check if any stale binaries
	let stale: IndexSet<_> = binaries
		.iter()
		.filter_map(|b| (b.stale()).then_some((b.name(), b.version(), b.latest())))
		.collect();
	let mut latest = false;
	if !stale.is_empty() {
		let list = style(format!(
			"> {}",
			stale
				.iter()
				.map(|(name, version, latest)| {
					format!("{name} {} -> {}", version.unwrap_or("None"), latest.unwrap_or("None"))
				})
				.collect::<Vec<_>>()
				.join(", ")
		))
		.dim()
		.to_string();
		log::warning(format!(
			"ℹ️ The following binaries have newer versions available:\n   {list}"
		))?;
		if !skip_confirm {
			latest = confirm(
				"📦 Would you like to source them automatically now? It may take some time..."
					.to_string(),
			)
			.initial_value(true)
			.interact()?;
		} else {
			latest = true;
		}
	}

	let binaries: Vec<_> = binaries
		.into_iter()
		.filter(|b| !b.exists() || (latest && b.stale()))
		.map(|b| {
			if latest && b.stale() {
				b.use_latest()
			}
			b
		})
		.collect();

	if binaries.is_empty() {
		return Ok(false);
	}

	if binaries.iter().any(|b| !b.local()) {
		log::info(format!(
			"ℹ️ Binaries will be cached at {}",
			&cache.to_str().expect("expected local cache is invalid")
		))?;
	}

	// Source binaries
	let release = true;
	match verbose {
		true => {
			let reporter = VerboseReporter;
			for binary in binaries {
				log::info(format!("📦 Sourcing {}...", binary.name()))?;
				Term::stderr().clear_last_lines(1)?;
				if let Err(e) = binary.source(release, &reporter, verbose).await {
					reporter.update(&format!("Sourcing failed: {e}"));
					outro_cancel(
						"🚫 Cannot launch the network until all required binaries are available.",
					)?;
					return Ok(true);
				}
			}
			reporter.update("");
		},
		false => {
			let multi = multi_progress("📦 Sourcing binaries...".to_string());
			let queue: Vec<_> = binaries
				.into_iter()
				.map(|binary| {
					let progress = multi.add(cliclack::spinner());
					progress.start(format!("{}: waiting...", binary.name()));
					(binary, progress)
				})
				.collect();
			let mut error = false;
			for (binary, progress) in queue {
				let prefix = format!("{}: ", binary.name());
				let progress_reporter = ProgressReporter(prefix, progress);
				if let Err(e) = binary.source(release, &progress_reporter, verbose).await {
					progress_reporter.1.error(format!("🚫 {}: {e}", binary.name()));
					error = true;
				}
				progress_reporter.1.stop(format!("✅  {}", binary.name()));
			}
			multi.stop();
			if error {
				outro_cancel(
					"🚫 Cannot launch the network until all required binaries are available.",
				)?;
				return Ok(true);
			}
		},
	};

	Ok(false)
}

async fn run_custom_command(spinner: &ProgressBar, command: &str) -> Result<(), anyhow::Error> {
	spinner.set_message(format!("Spinning up network & running command: {}", command));
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
struct ProgressReporter(String, ProgressBar);

impl Status for ProgressReporter {
	fn update(&self, status: &str) {
		self.1
			.start(format!("{}{}", self.0, status.replace("   Compiling", "Compiling")))
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
