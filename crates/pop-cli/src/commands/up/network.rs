// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Confirm},
	style::{Theme, style},
};
use clap::{
	Arg, Args, Command,
	builder::{PossibleValue, PossibleValuesParser, StringValueParser, TypedValueParser},
	error::ErrorKind,
};
use cliclack::{ProgressBar, multi_progress, spinner};
use console::{Emoji, Style, Term};
use duct::cmd;
pub(crate) use pop_chains::up::Relay;
use pop_chains::{
	Error, IndexSet, NetworkNode, RelayChain, clear_dmpq,
	registry::{self, traits::Chain as ChainT},
	up::{NetworkConfiguration, Zombienet},
};
use pop_common::Status;
use serde::Serialize;
use std::{
	collections::HashMap,
	ffi::OsStr,
	path::{Path, PathBuf},
	time::Duration,
};
use tokio::time::sleep;

/// Launch a local network by specifying a network configuration file.
#[derive(Args, Clone, Default, Serialize)]
pub(crate) struct ConfigFileCommand {
	/// The Zombienet network configuration file to be used.
	#[serde(skip_serializing)]
	#[arg(value_name = "FILE")]
	pub path: PathBuf,
	/// The version of the binary to be used for the relay chain, as per the release tag (e.g.
	/// "stable2512"). See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	pub(crate) relay_chain: Option<String>,
	/// The version of the runtime to be used for the relay chain, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'R', long)]
	pub(crate) relay_chain_runtime: Option<String>,
	/// The version of the binary to be used for system parachains, as per the release tag (e.g.
	/// "stable2512"). Defaults to the relay chain version if not specified.
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
	/// Automatically remove the state upon tearing down the network.
	#[clap(long = "rm")]
	pub(crate) auto_remove: bool,
	/// Automatically detach from the terminal and run the network in the background.
	#[clap(short, long)]
	pub(crate) detach: bool,
}

impl ConfigFileCommand {
	/// Executes the command.
	pub(crate) async fn execute(&self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Launch a local network")?;

		let path = self.path.canonicalize()?;
		cd_into_chain_base_dir(&path);
		spawn(
			path.as_path().try_into()?,
			self.relay_chain.as_deref(),
			self.relay_chain_runtime.as_deref(),
			self.system_parachain.as_deref(),
			self.system_parachain_runtime.as_deref(),
			self.parachain.as_ref(),
			self.verbose,
			self.skip_confirm,
			self.auto_remove,
			self.detach,
			self.command.as_deref(),
			cli,
		)
		.await?;
		cli.info(self.display())?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut full_message = "pop up network".to_string();
		full_message.push_str(&format!(" --path {}", self.path.display()));
		if let Some(rc) = &self.relay_chain {
			full_message.push_str(&format!(" --relay-chain {}", rc));
		}
		if let Some(rcr) = &self.relay_chain_runtime {
			full_message.push_str(&format!(" --relay-chain-runtime {}", rcr));
		}
		if let Some(sp) = &self.system_parachain {
			full_message.push_str(&format!(" --system-parachain {}", sp));
		}
		if let Some(spr) = &self.system_parachain_runtime {
			full_message.push_str(&format!(" --system-parachain-runtime {}", spr));
		}
		if let Some(p) = &self.parachain {
			full_message.push_str(&format!(" --parachain {}", p.join(",")));
		}
		if let Some(cmd) = &self.command {
			full_message.push_str(&format!(" --cmd \"{}\"", cmd));
		}
		if self.verbose {
			full_message.push_str(" --verbose");
		}
		if self.skip_confirm {
			full_message.push_str(" --skip-confirm");
		}
		if self.auto_remove {
			full_message.push_str(" --rm");
		}
		if self.detach {
			full_message.push_str(" --detach");
		}
		full_message
	}
}

/// Launch a local network with supported parachains.
#[derive(Args, Clone, Serialize)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct BuildCommand<const FILTER: u8> {
	/// The version of the binary to be used for the relay chain, as per the release tag (e.g.
	/// "stable2512"). See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	relay_chain: Option<String>,
	/// The version of the runtime to be used for the relay chain, as per the release tag (e.g.
	/// "v1.4.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'R', long)]
	relay_chain_runtime: Option<String>,
	/// The version of the binary to be used for system parachains, as per the release tag (e.g.
	/// "stable2512"). Defaults to the relay chain version if not specified.
	/// See <https://github.com/paritytech/polkadot-sdk/releases> for more details.
	#[arg(short, long)]
	system_parachain: Option<String>,
	/// The version of the runtime to be used for system parachains, as per the release tag (e.g.
	/// "v1.5.1"). See <https://github.com/polkadot-fellows/runtimes/releases> for more details.
	#[arg(short = 'S', long)]
	system_parachain_runtime: Option<String>,
	/// The parachain(s) to be included. An optional parachain identifier and/or port can be
	/// affixed via #id and :port specifiers (e.g. `asset-hub#1000:9944`).
	#[serde(skip_serializing)]
	#[arg(short, long, value_delimiter = ',', value_parser = SupportedChains::<FILTER>::new())]
	parachain: Option<Vec<Box<dyn ChainT>>>,
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
	/// Automatically remove the state upon tearing down the network.
	#[clap(long = "rm")]
	auto_remove: bool,
	/// Automatically detach from the terminal and run the network in the background.
	#[clap(short, long)]
	detach: bool,
}

impl<const FILTER: u8> BuildCommand<FILTER> {
	/// Injects a parachain into the command's parachain list.
	pub(crate) fn inject_parachain(&mut self, chain: Box<dyn ChainT>) {
		self.parachain.get_or_insert_with(Vec::new).push(chain);
	}

	/// Executes the command.
	pub(crate) async fn execute(
		&mut self,
		relay: Relay,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
		cli.intro(format!("Launch a local {} network", relay.name()))?;

		let mut chains = self.parachain.take();

		// Check for any missing dependencies, auto-adding as required.
		if let Some(ref mut chains) = chains {
			let provided: Vec<_> = chains.iter().map(|p| p.as_any().type_id()).collect();
			let dependencies: HashMap<_, _> =
				chains.iter().filter_map(|p| p.requires()).flatten().collect();
			let all: HashMap<_, _> =
				registry::chains(&relay).iter().map(|p| (p.as_any().type_id(), p)).collect();

			let missing: Vec<_> = dependencies
				.keys()
				.filter_map(|k| {
					(!provided.contains(k)).then(|| all.get(k)).flatten().map(|p| {
						chains.push((*p).clone());
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

		let network_config =
			NetworkConfiguration::build(relay.clone(), self.port, chains.as_deref())?;

		let parachain_names: Option<Vec<_>> =
			chains.as_ref().map(|p| p.iter().map(|r| r.name().to_string()).collect());

		spawn(
			network_config,
			self.relay_chain.as_deref(),
			self.relay_chain_runtime.as_deref(),
			self.system_parachain.as_deref(),
			self.system_parachain_runtime.as_deref(),
			parachain_names.as_ref(),
			self.verbose,
			self.skip_confirm,
			self.auto_remove,
			self.detach,
			self.command.as_deref(),
			cli,
		)
		.await?;
		cli.info(self.display(relay))?;
		Ok(())
	}

	fn display(&self, relay: Relay) -> String {
		let mut full_message = format!("pop up {}", relay.name().to_lowercase());
		if let Some(rc) = &self.relay_chain {
			full_message.push_str(&format!(" --relay-chain {}", rc));
		}
		if let Some(rcr) = &self.relay_chain_runtime {
			full_message.push_str(&format!(" --relay-chain-runtime {}", rcr));
		}
		if let Some(sp) = &self.system_parachain {
			full_message.push_str(&format!(" --system-parachain {}", sp));
		}
		if let Some(spr) = &self.system_parachain_runtime {
			full_message.push_str(&format!(" --system-parachain-runtime {}", spr));
		}
		if let Some(p) = &self.parachain {
			let names: Vec<_> = p.iter().map(|r| r.name()).collect();
			full_message.push_str(&format!(" --parachain {}", names.join(",")));
		}
		if let Some(port) = self.port {
			full_message.push_str(&format!(" --port {}", port));
		}
		if let Some(cmd) = &self.command {
			full_message.push_str(&format!(" --cmd \"{}\"", cmd));
		}
		if self.verbose {
			full_message.push_str(" --verbose");
		}
		if self.skip_confirm {
			full_message.push_str(" --skip-confirm");
		}
		if self.auto_remove {
			full_message.push_str(" --rm");
		}
		if self.detach {
			full_message.push_str(" --detach");
		}
		full_message
	}
}

#[derive(Clone)]
struct SupportedChains<const FILTER: u8>(PossibleValuesParser);

impl<const FILTER: u8> SupportedChains<FILTER> {
	fn new() -> Self {
		let relay = Relay::from(FILTER).expect("expected valid relay variant index as filter");
		Self(PossibleValuesParser::new(
			registry::chains(&relay)
				.iter()
				.map(|p| PossibleValue::new(p.name().to_string())),
		))
	}
}

impl<const FILTER: u8> TypedValueParser for SupportedChains<FILTER> {
	type Value = Box<dyn ChainT>;

	fn parse_ref(
		&self,
		cmd: &Command,
		arg: Option<&Arg>,
		value: &OsStr,
	) -> Result<Self::Value, clap::Error> {
		// Parse value as chain with optional chain id and port specifiers
		let (chain, id, port) = match self.0.parse_ref(cmd, arg, value) {
			Ok(value) => (value, None, None),
			// Check if failure due to chain id being specified
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

		// Attempt to resolve from supported chains
		let relay = Relay::from(FILTER).expect("expected valid relay variant index as filter");
		registry::chains(&relay)
			.iter()
			.find(|p| {
				let chain = chain.as_str();
				p.name() == chain || p.chain() == chain
			})
			.map(|p| {
				let mut chain = p.clone();
				// Override id and/or port if provided
				if let Some(id) = id {
					chain.set_id(id);
				}
				if let Some(port) = port {
					chain.set_port(port);
				}
				chain
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
	auto_remove: bool,
	detach: bool,
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
		Err(e) => {
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
			};
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
			let mut result = if detach {
				"🚀 Network launched successfully - detached".to_string()
			} else {
				"🚀 Network launched successfully - Ctrl+C to terminate".to_string()
			};
			let base_dir = PathBuf::from(network.base_dir().expect("base_dir expected to exist"));
			let bar = Style::new().magenta().dim().apply_to(Emoji("│", "|"));
			result.push_str(&format!(
				"\n{bar}  base dir: {0}\n{bar}  zombie.json: {0}/zombie.json",
				base_dir.display()
			));

			let output = |node: &NetworkNode| -> String {
				let name = node.name();
				let mut output = format!(
					"\n{bar}       {name}:
{bar}         endpoint: {0}
{bar}         portal: https://polkadot.js.org/apps/?rpc={0}#/explorer / https://dev.papi.how/explorer#networkId=custom&endpoint={0}
{bar}         logs: tail -f {1}/{name}/{name}.log",
					node.ws_uri(),
					base_dir.display()
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
			// Add chain info
			let mut chains = network.parachains();
			chains.sort_by_key(|p| p.para_id());
			for chain in chains {
				result.push_str(&format!(
					"\n{bar}  ⛓️ {}",
					chain.chain_id().map_or(format!("id: {}", chain.para_id()), |c| format!(
						"{c}: {}",
						chain.para_id()
					))
				));
				let mut collators = chain.collators();
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
						let relay_endpoint = network.relaychain().nodes()[0].ws_uri().to_string();
						let ids: Vec<_> =
							network.parachains().iter().map(|p| p.para_id()).collect();
						tokio::spawn(async move {
							if let Err(e) = clear_dmpq(&relay_endpoint, &ids).await {
								progress.stop(format!("🚫 Could not prepare channels: {e}"));
								return Ok::<(), Error>(());
							}
							progress.stop("Channels successfully prepared for initialization.");
							Ok::<(), Error>(())
						});
					},
				}
			}

			if detach {
				network.detach().await;
				std::mem::forget(network);
				cli.info(format!(
					"ℹ️ base dir: {0}\nℹ️ zombie.json: {0}/zombie.json",
					base_dir.display()
				))?;
				if auto_remove {
					cli.warning(format!(
						"⚠️ --rm is ignored when used with --detach. Remove {} after stopping the network.",
						base_dir.display()
					))?;
				}
				cli.outro("✅ Network is running in the background.")?;
				return Ok(());
			}

			tokio::signal::ctrl_c().await?;

			if auto_remove {
				// Remove zombienet directory after network is terminated
				if let Err(e) = std::fs::remove_dir_all(&base_dir) {
					cli.warning(format!("🚫 Failed to remove zombienet directory: {e}"))?;
				}
			} else if let Err(e) = std::fs::write(base_dir.join(".CLEARED"), "") {
				cli.warning(format!("🚫 Failed to mark network as cleared: {e}"))?;
			}

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
		cli.warning(format!(
			"⚠️ The following binaries required to launch the network cannot be found locally:\n   {list}"
		))?;

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
				"📦 Would you like to source them automatically now? It may take some time...\n   {list}"
			))
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

fn cd_into_chain_base_dir(network_file: &Path) {
	let mut parent = network_file;
	loop {
		if pop_chains::is_supported(parent) {
			std::env::set_current_dir(parent).unwrap();
			break;
		}
		match parent.parent() {
			Some(p) => parent = p,
			None => break,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use std::{env, fs};

	#[test]
	fn test_config_file_command_display() {
		let cmd = ConfigFileCommand {
			path: PathBuf::from("config.toml"),
			relay_chain: Some("stable2503".to_string()),
			relay_chain_runtime: Some("v1.4.1".to_string()),
			system_parachain: Some("stable2503".to_string()),
			system_parachain_runtime: Some("v1.4.1".to_string()),
			parachain: Some(vec!["p1".to_string(), "p2".to_string()]),
			command: Some("ls -la".to_string()),
			verbose: true,
			skip_confirm: true,
			auto_remove: true,
			detach: false,
		};
		assert_eq!(
			cmd.display(),
			"pop up network --path config.toml --relay-chain stable2503 --relay-chain-runtime v1.4.1 --system-parachain stable2503 --system-parachain-runtime v1.4.1 --parachain p1,p2 --cmd \"ls -la\" --verbose --skip-confirm --rm"
		);
	}

	#[test]
	fn test_build_command_display() {
		let cmd = BuildCommand::<0> {
			relay_chain: Some("stable2503".to_string()),
			relay_chain_runtime: Some("v1.4.1".to_string()),
			system_parachain: Some("stable2503".to_string()),
			system_parachain_runtime: Some("v1.5.1".to_string()),
			parachain: None,
			port: Some(9944),
			command: Some("ls".to_string()),
			verbose: true,
			skip_confirm: true,
			auto_remove: true,
			detach: false,
		};
		assert_eq!(
			cmd.display(Relay::Paseo),
			"pop up paseo --relay-chain stable2503 --relay-chain-runtime v1.4.1 --system-parachain stable2503 --system-parachain-runtime v1.5.1 --port 9944 --cmd \"ls\" --verbose --skip-confirm --rm"
		);
	}

	#[tokio::test]
	async fn test_run_custom_command() -> Result<(), anyhow::Error> {
		let spinner = ProgressBar::new(1);

		// Define the command to be executed
		let command = "echo 2 + 2";

		// Call the run_custom_command function
		run_custom_command(&spinner, command).await?;

		Ok(())
	}

	#[test]
	fn test_cd_into_chain_base_dir_changes_to_supported_parent() {
		// Save original working directory
		let original_cwd = env::current_dir().expect("cwd");

		// Create a unique temporary directory structure
		let mut base = env::temp_dir();
		base.push(format!(
			"pop_cli_test_{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_millis()
		));
		let project_dir = base.join("project");
		let nested_dir = project_dir.join("nested/a/b");
		fs::create_dir_all(&nested_dir).expect("create nested dirs");

		// Write a minimal Cargo.toml that qualifies as a supported chain project
		let cargo_toml = r#"[package]
name = "dummy-chain"
version = "0.1.0"
edition = "2021"

[dependencies]
cumulus-client-collator = "0.14"
"#;
		fs::write(project_dir.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");

		// Provide a path to a pretend network file deep inside the project
		let network_file = nested_dir.join("network.toml");

		// Execute
		cd_into_chain_base_dir(&network_file);

		// Assert we changed into the project_dir
		let cwd = env::current_dir().expect("cwd after cd");
		assert_eq!(
			cwd.canonicalize().expect("canon cwd"),
			project_dir.canonicalize().expect("canon project_dir")
		);

		// Restore cwd and cleanup
		env::set_current_dir(&original_cwd).expect("restore cwd");
		fs::remove_dir_all(&base).ok();
	}

	#[test]
	fn test_cd_into_chain_base_dir_noop_when_unsupported() {
		// Save original working directory
		let original_cwd = env::current_dir().expect("cwd");

		// Create a unique temporary directory structure without a Cargo.toml
		let mut base = env::temp_dir();
		base.push(format!(
			"pop_cli_test_{}_unsupported",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_millis()
		));
		let nested_dir = base.join("nested/a/b");
		fs::create_dir_all(&nested_dir).expect("create nested dirs");

		// Provide a path to a pretend network file
		let network_file = nested_dir.join("network.toml");

		// Execute
		cd_into_chain_base_dir(&network_file);

		// Assert cwd has not changed (no supported project found up the tree)
		let cwd = env::current_dir().expect("cwd after cd");
		assert_eq!(cwd, original_cwd);

		// Cleanup
		fs::remove_dir_all(&base).ok();
	}
}
