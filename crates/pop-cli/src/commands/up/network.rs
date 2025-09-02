// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Confirm},
	style::{style, Theme},
};
use clap::{
	builder::{PossibleValue, PossibleValuesParser, StringValueParser, TypedValueParser},
	error::ErrorKind,
	Arg, Args, Command,
};
use cliclack::{multi_progress, spinner, ProgressBar};
use console::{Emoji, Style, Term};
use duct::cmd;
pub(crate) use pop_chains::up::Relay;
use pop_chains::{
	clear_dmpq,
	registry::{self, traits::Rollup},
	up::{NetworkConfiguration, Zombienet},
	Error, IndexSet, NetworkNode, RelayChain,
};
use pop_common::Status;
use std::{
	collections::HashMap,
	ffi::OsStr,
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::time::sleep;

/// Launch a local network by specifying a network configuration file.
#[derive(Args, Clone, Default)]
pub(crate) struct ConfigFileCommand {
	/// The Zombienet network configuration file to be used.
	#[arg(value_name = "FILE", conflicts_with = "file")]
	pub path: Option<PathBuf>,
	/// [DEPRECATED] The Zombienet network configuration file to be used (will be removed in
	/// v0.10.0).
	#[arg(short = 'f', long = "file")]
	#[deprecated(since = "0.9.0", note = "will be removed in v0.10.0")]
	#[allow(rustdoc::broken_intra_doc_links)]
	pub(crate) file: Option<PathBuf>,
	/// The version of the binary to be used for the relay chain, as per the release tag (e.g.
	/// "stable2503"). See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	pub(crate) relay_chain: Option<String>,
	/// The version of the runtime to be used for the relay chain, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'R', long)]
	pub(crate) relay_chain_runtime: Option<String>,
	/// The version of the binary to be used for system parachains, as per the release tag (e.g.
	/// "stable2503"). Defaults to the relay chain version if not specified.
	/// See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	pub(crate) system_parachain: Option<String>,
	/// The version of the runtime to be used for system parachains, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'S', long)]
	pub(crate) system_parachain_runtime: Option<String>,
	/// The url of the git repository of a parachain to be used, with branch/release tag/commit specified as #fragment (e.g. <https://github.com/org/repository#ref>).
	/// A specific binary name can also be optionally specified via query string parameter (e.g. <https://github.com/org/repository?binaryname#ref>), defaulting to the name of the repository when not specified.
	#[arg(short, long)]
	pub(crate) parachain: Option<Vec<String>>,
	/// The command to run after the network has been launched.
	#[clap(name = "cmd", short, long)]
	pub(crate) command: Option<String>,
	/// Whether the output should be verbose.
	#[arg(short, long, action)]
	pub(crate) verbose: bool,
	/// Automatically source all necessary binaries required without prompting for confirmation.
	#[clap(short = 'y', long)]
	pub(crate) skip_confirm: bool,
}

impl ConfigFileCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Launch a local network")?;

		// Show warning if file argument provided.
		#[allow(deprecated)]
		if self.file.is_some() {
			cli.warning(
				"DEPRECATION: Please use `pop up network [PATH]` (or simply `pop u n [PATH]`) in the future...",
			)?;
		}

		// Determine network config from args.
		let network_config = match self.path.as_ref() {
			Some(path) => path,
			#[allow(deprecated)]
			None => match self.file.as_deref() {
				None => {
					cli.outro_cancel("🚫 A network configuration file must be specified. See `pop up network --help` for usage.".to_string())?;
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
			cli,
		)
		.await
	}
}

/// Launch a local network with supported parachains.
#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct BuildCommand<const FILTER: u8> {
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
	/// "v1.5.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'S', long)]
	system_parachain_runtime: Option<String>,
	/// The parachain(s) to be included. An optional parachain identifier and/or port can be
	/// affixed via #id and :port specifiers (e.g. `asset-hub#1000:9944`).
	#[arg(short, long, value_delimiter = ',', value_parser = SupportedRollups::<FILTER>::new())]
	parachain: Option<Vec<Box<dyn Rollup>>>,
	/// The port to be used for the first relay chain validator.
	#[clap(short = 'P', long)]
	port: Option<u16>,
	/// The command to run after the network has been launched.
	#[clap(name = "cmd", short, long)]
	command: Option<String>,
	/// Whether the output should be verbose.
	#[arg(short, long, action)]
	verbose: bool,
	/// Automatically source all necessary binaries required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl<const FILTER: u8> BuildCommand<FILTER> {
	/// Executes the command.
	pub(crate) async fn execute(
		&mut self,
		relay: Relay,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
		cli.intro(format!("Launch a local {} network", relay.name()))?;

		let mut rollups = self.parachain.take();

		// Check for any missing dependencies, auto-adding as required.
		if let Some(ref mut rollups) = rollups {
			let provided: Vec<_> = rollups.iter().map(|p| p.as_any().type_id()).collect();
			let dependencies: HashMap<_, _> =
				rollups.iter().filter_map(|p| p.requires()).flatten().collect();
			let all: HashMap<_, _> =
				registry::rollups(&relay).iter().map(|p| (p.as_any().type_id(), p)).collect();

			let missing: Vec<_> = dependencies
				.keys()
				.filter_map(|k| {
					(!provided.contains(k)).then(|| all.get(k)).flatten().map(|p| {
						rollups.push((*p).clone());
						p.name()
					})
				})
				.collect();
			if !missing.is_empty() {
				cli.info(format!(
					"The following dependencies are required for the provided parachain(s) and have automatically been added: {}",
					missing.join(", ")
				))?;
			}
		}

		let network_config = NetworkConfiguration::build(relay, self.port, rollups.as_deref())?;

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
			cli,
		)
		.await
	}
}

#[derive(Clone)]
struct SupportedRollups<const FILTER: u8>(PossibleValuesParser);

impl<const FILTER: u8> SupportedRollups<FILTER> {
	fn new() -> Self {
		let relay = Relay::from(FILTER).expect("expected valid relay variant index as filter");
		Self(PossibleValuesParser::new(
			registry::rollups(&relay)
				.iter()
				.map(|p| PossibleValue::new(p.name().to_string())),
		))
	}
}

impl<const FILTER: u8> TypedValueParser for SupportedRollups<FILTER> {
	type Value = Box<dyn Rollup>;

	fn parse_ref(
		&self,
		cmd: &Command,
		arg: Option<&Arg>,
		value: &OsStr,
	) -> Result<Self::Value, clap::Error> {
		// Parse value as chain with optional rollup id and port specifiers
		let (chain, id, port) = match self.0.parse_ref(cmd, arg, value) {
			Ok(value) => (value, None, None),
			// Check if failure due to rollup id being specified
			Err(e) if e.kind() == ErrorKind::InvalidValue => {
				let value = StringValueParser::new().parse_ref(cmd, arg, value)?;
				// Attempt to parse name and optional id, port from the entered value
				const SPECIFIER: &str = "^([a-z0-9_-]+)(?:#([0-9]+))?(?::([0-9]+))?$";
				let pattern = regex::Regex::new(SPECIFIER).expect("expected valid regex");
				let Some(captures) = pattern.captures(&value) else { return Err(e) };
				let name = captures.get(1).map(|m| m.as_str()).expect("checked above").into();
				let id = captures.get(2).map(|m| m.as_str()).and_then(|id| id.parse().ok());
				let port = captures.get(3).map(|m| m.as_str()).and_then(|id| id.parse().ok());
				(name, id, port)
			},
			Err(e) => return Err(e),
		};

		// Attempt to resolve from supported rollups
		let relay = Relay::from(FILTER).expect("expected valid relay variant index as filter");
		registry::rollups(&relay)
			.iter()
			.find(|p| {
				let chain = chain.as_str();
				p.name() == chain || p.chain() == chain
			})
			.map(|p| {
				let mut rollup = p.clone();
				// Override id and/or port if provided
				if let Some(id) = id {
					rollup.set_id(id);
				}
				if let Some(port) = port {
					rollup.set_port(port);
				}
				rollup
			})
			.ok_or(clap::Error::new(ErrorKind::InvalidValue).with_cmd(cmd))
	}

	fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
		self.0.possible_values()
	}
}

/// Executes the command.
#[allow(clippy::too_many_arguments)]
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
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	// Initialize from arguments
	let cache = crate::cache()?;
	let mut zombienet = match Zombienet::new(
		&cache,
		config,
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
					cli.outro_cancel(format!("🚫 A configuration error occurred: `{message}`"))?;
					Ok(())
				},
				Error::MissingBinary(name) => {
					cli.outro_cancel(format!("🚫 The `{name}` binary is specified in the network configuration file, but cannot be resolved to a source. Are you missing a `--parachain` argument?"))?;
					Ok(())
				},
				_ => Err(e.into()),
			},
	};

	// Source any missing/stale binaries
	if source_binaries(&mut zombienet, &cache, verbose, skip_confirm, cli).await? {
		return Ok(());
	}

	// Output the binaries and versions used if verbose logging enabled.
	if verbose {
		let binaries = zombienet
			.binaries()
			.map(|b| {
				format!(
					"{}{}",
					b.name(),
					b.version().map(|v| format!(" ({v})")).unwrap_or_default()
				)
			})
			.fold(Vec::new(), |mut set, binary| {
				if !set.contains(&binary) {
					set.push(binary);
				}
				set
			});
		cli.info(format!("Binaries used: {}", binaries.join(", ")))?;
	}

	// Finally, spawn the network and wait for a signal to terminate
	let progress = spinner();
	progress.start("🚀 Launching local network...");
	match zombienet.spawn().await {
		Ok(network) => {
			let mut result = "🚀 Network launched successfully - Ctrl+C to terminate".to_string();
			let base_dir = network.base_dir().expect("base_dir expected to exist");
			let bar = Style::new().magenta().dim().apply_to(Emoji("│", "|"));

			let output = |node: &NetworkNode| -> String {
				let name = node.name();
				let mut output = format!(
					"\n{bar}       {name}:
{bar}         endpoint: {0}
{bar}         portal: https://polkadot.js.org/apps/?rpc={0}#/explorer / https://dev.papi.how/explorer#networkId=custom&endpoint={0}
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
			result.push_str(&format!(
				"\n{bar}  ⛓️ {} (⌛️epoch/session duration: {})️",
				network.relaychain().chain(),
				if network.relaychain().chain().contains("paseo") { "1 min" } else { "2 mins" }
			));
			for node in validators {
				result.push_str(&output(node));
			}
			// Add rollup info
			let mut rollups = network.parachains();
			rollups.sort_by_key(|p| p.para_id());
			for rollup in rollups {
				result.push_str(&format!(
					"\n{bar}  ⛓️ {}",
					rollup.chain_id().map_or(format!("id: {}", rollup.para_id()), |chain| format!(
						"{chain}: {}",
						rollup.para_id()
					))
				));
				let mut collators = rollup.collators();
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
						cli.error(format!("🚫 Using `{relay_chain}` with HRMP channels is currently unsupported. Please use `paseo-local` or `westend-local`."))?;
					},
					Some(_) => {
						let progress = spinner();
						progress.start("Connecting to relay chain to prepare channels...");
						// Allow relay node time to start
						sleep(Duration::from_secs(10)).await;
						progress.set_message("Preparing channels...");
						let relay_endpoint = network.relaychain().nodes()[0].wait_client().await?;
						let ids: Vec<_> =
							network.parachains().iter().map(|p| p.para_id()).collect();
						tokio::spawn(async move {
							if let Err(e) = clear_dmpq(relay_endpoint, &ids).await {
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
			cli.outro("Done")?;
		},
		Err(e) => {
			cli.outro_cancel(format!("🚫 Could not launch local network: {e}"))?;
		},
	}

	Ok(())
}

async fn source_binaries(
	zombienet: &mut Zombienet,
	cache: &Path,
	verbose: bool,
	skip_confirm: bool,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<bool> {
	// Check for any missing or stale binaries
	let binaries = zombienet.binaries().filter(|b| !b.exists() || b.stale()).fold(
		Vec::new(),
		|mut set, binary| {
			if !set.contains(&binary) {
				set.push(binary);
			}
			set
		},
	);
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
		cli.warning(format!("⚠️ The following binaries required to launch the network cannot be found locally:\n   {list}"))?;

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
			!cli.confirm(format!(
				"📦 Would you like to source them automatically now? It may take some time...\n   {list}"))
				.initial_value(true)
				.interact()?
		{
			cli.outro_cancel(
				"🚫 Cannot launch the specified network until all required binaries are available.",
			)?;
			return Ok(true);
		}
	}

	// Check if any stale binaries
	let stale: IndexSet<_> = binaries
		.iter()
		.filter_map(|b| b.stale().then_some((b.name(), b.version(), b.latest())))
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
		cli.warning(format!(
			"ℹ️ The following binaries have newer versions available:\n   {list}"
		))?;
		if !skip_confirm {
			latest = cli
				.confirm(
					"📦 Would you like to source them automatically now? It may take some time..."
						.to_string(),
				)
				.initial_value(true)
				.interact()?;
		} else {
			latest = true;
		}
	}

	#[allow(clippy::manual_inspect)]
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
		cli.info(format!(
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
				cli.info(format!("📦 Sourcing {}...", binary.name()))?;
				Term::stderr().clear_last_lines(1)?;
				if let Err(e) = binary.source(release, &reporter, verbose).await {
					reporter.update(&format!("Sourcing failed: {e}"));
					cli.outro_cancel(
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
					let progress = multi.add(spinner());
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
				cli.outro_cancel(
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
	#[cfg(not(test))]
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
		use cliclack::{Theme, ThemeState};
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
