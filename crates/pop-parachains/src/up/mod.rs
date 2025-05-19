// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use glob::glob;
use indexmap::IndexMap;
pub use pop_common::{
	git::{GitHub, Repository},
	sourcing::{Binary, GitHub::*, Source, Source::*},
	Profile,
};
use std::{
	collections::BTreeSet,
	fmt::Debug,
	iter::once,
	path::{Path, PathBuf},
};
use strum::VariantArray;
use symlink::{remove_symlink_file, symlink_file};
use toml_edit::DocumentMut;
pub use zombienet_sdk::NetworkConfigBuilder;
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfig, NetworkConfigExt};

mod chain_specs;
mod parachains;
mod relay;

/// Configuration to launch a local network.
pub struct Zombienet {
	/// The config to be used to launch a network.
	network_config: NetworkConfiguration,
	/// The configuration required to launch the relay chain.
	relay_chain: RelayChain,
	/// The configuration required to launch parachains.
	parachains: IndexMap<u32, Parachain>,
	/// Whether any HRMP channels are to be pre-opened.
	hrmp_channels: bool,
}

impl Zombienet {
	/// Initializes the configuration for launching a local network.
	///
	/// # Arguments
	/// * `cache` - The location used for caching binaries.
	/// * `network_config` - The configuration to be used to launch a network. This can be a [Path]
	///   or the result of [NetworkConfigBuilder].
	/// * `relay_chain_version` - The specific binary version used for the relay chain (`None` will
	///   use the latest available version).
	/// * `relay_chain_runtime_version` - The specific runtime version used for the relay chain
	///   runtime (`None` will use the latest available version).
	/// * `system_parachain_version` - The specific binary version used for system parachains
	///   (`None` will use the latest available version).
	/// * `system_parachain_runtime_version` - The specific runtime version used for system
	///   parachains (`None` will use the latest available version).
	/// * `parachains` - The parachain(s) specified.
	pub async fn new(
		cache: &Path,
		network_config: impl TryInto<NetworkConfiguration, Error: Into<Error>>,
		relay_chain_version: Option<&str>,
		relay_chain_runtime_version: Option<&str>,
		system_parachain_version: Option<&str>,
		system_parachain_runtime_version: Option<&str>,
		parachains: Option<&Vec<String>>,
	) -> Result<Self, Error> {
		// Determine network config
		let network_config = network_config.try_into().map_err(|e| e.into())?;
		// Determine relay and parachain requirements based on arguments and config
		let relay_chain = Self::init_relay_chain(
			relay_chain_version,
			relay_chain_runtime_version,
			&network_config,
			cache,
		)
		.await?;
		let parachains = match parachains {
			Some(parachains) => Some(
				parachains
					.iter()
					.map(|url| Repository::parse(url))
					.collect::<Result<Vec<_>, _>>()?,
			),
			None => None,
		};
		let parachains = Self::parachains(
			&relay_chain,
			system_parachain_version,
			system_parachain_runtime_version,
			parachains,
			&network_config,
			cache,
		)
		.await?;
		let hrmp_channels = !network_config.0.hrmp_channels().is_empty();
		Ok(Self { network_config, relay_chain, parachains, hrmp_channels })
	}

	/// The binaries required to launch the network.
	pub fn binaries(&mut self) -> impl Iterator<Item = &mut Binary> {
		once([Some(&mut self.relay_chain.binary), self.relay_chain.chain_spec_generator.as_mut()])
			.chain(
				self.parachains
					.values_mut()
					.map(|p| [Some(&mut p.binary), p.chain_spec_generator.as_mut()]),
			)
			.flatten()
			.flatten()
	}

	/// Determine parachain configuration based on specified version and network configuration.
	///
	/// # Arguments
	/// * `relay_chain` - The configuration required to launch the relay chain.
	/// * `system_parachain_version` - The specific binary version used for system parachains
	///   (`None` will use the latest available version).
	/// * `system_parachain_runtime_version` - The specific runtime version used for system
	///   parachains (`None` will use the latest available version).
	/// * `parachains` - The parachain repositories specified.
	/// * `network_config` - The network configuration to be used to launch a network.
	/// * `cache` - The location used for caching binaries.
	async fn parachains(
		relay_chain: &RelayChain,
		system_parachain_version: Option<&str>,
		system_parachain_runtime_version: Option<&str>,
		parachains: Option<Vec<Repository>>,
		network_config: &NetworkConfiguration,
		cache: &Path,
	) -> Result<IndexMap<u32, Parachain>, Error> {
		let mut paras: IndexMap<u32, Parachain> = IndexMap::new();
		'outer: for parachain in network_config.0.parachains() {
			let id = parachain.id();
			let chain = parachain.chain().map(|c| c.as_str());

			let command = parachain
				.default_command()
				.map(|c| c.as_str())
				.or_else(|| {
					// Check if any collators define command
					for collator in parachain.collators() {
						if let Some(command) = collator.command().map(|i| i.as_str()) {
							return Some(command);
						}
					}

					// Otherwise default to polkadot-parachain
					Some("polkadot-parachain")
				})
				.expect("missing default_command set above")
				.to_lowercase();

			// Check if system parachain
			if let Some(parachain) = parachains::system(
				id,
				&command,
				system_parachain_version,
				system_parachain_runtime_version,
				relay_chain.binary.version().expect("expected relay chain to have version"),
				chain,
				cache,
			)
			.await?
			{
				paras.insert(id, parachain);
				continue;
			}

			// Check if known parachain
			let version = parachains.as_ref().and_then(|r| {
				r.iter()
					.filter_map(|r| {
						(r.package == command).then_some(r.reference.as_ref()).flatten()
					})
					.nth(0)
					.map(|v| v.as_str())
			});
			if let Some(parachain) = parachains::from(id, &command, version, chain, cache).await? {
				paras.insert(id, parachain);
				continue;
			}

			// Check if parachain binary source specified as an argument
			if let Some(parachains) = parachains.as_ref() {
				if let Some(repo) = parachains.iter().find(|r| command == r.package) {
					paras.insert(id, Parachain::from_repository(id, repo, chain, cache)?);
					continue 'outer;
				}
			}

			// Check if command references a local binary
			if ["./", "../", "/"].iter().any(|p| command.starts_with(p)) {
				paras.insert(id, Parachain::from_local(id, command.into(), chain)?);
				continue;
			}

			// Check if command references a parachain template binary without a specified path
			// (e.g. Polkadot SDK parachain template)
			if ["parachain-template-node", "substrate-contracts-node"].contains(&command.as_str()) {
				for profile in Profile::VARIANTS {
					let binary_path = profile.target_directory(Path::new("./")).join(&command);
					if binary_path.exists() {
						paras.insert(id, Parachain::from_local(id, binary_path, chain)?);
						continue 'outer;
					}
				}
				return Err(Error::MissingBinary(command));
			}
			return Err(Error::MissingBinary(command));
		}
		Ok(paras)
	}

	/// Determines relay chain configuration based on specified version and network configuration.
	///
	/// # Arguments
	/// * `version` - The specific binary version used for the relay chain (`None` will use the
	///   latest available version).
	/// * `runtime_version` - The specific runtime version used for the relay chain runtime (`None`
	///   will use the latest available version).
	/// * `network_config` - The network configuration to be used to launch a network.
	/// * `cache` - The location used for caching binaries.
	async fn init_relay_chain(
		version: Option<&str>,
		runtime_version: Option<&str>,
		network_config: &NetworkConfiguration,
		cache: &Path,
	) -> Result<RelayChain, Error> {
		// Attempt to determine relay from configuration
		let relay_chain = network_config.0.relaychain();
		let chain = relay_chain.chain().as_str();
		if let Some(default_command) = relay_chain.default_command().map(|c| c.as_str()) {
			let relay =
				relay::from(default_command, version, runtime_version, chain, cache).await?;
			// Validate any node config is supported
			for node in relay_chain.nodes() {
				if let Some(command) = node.command().map(|c| c.as_str()) {
					if command.to_lowercase() != relay.binary.name() {
						return Err(Error::UnsupportedCommand(format!(
							"the relay chain command is unsupported: {command}",
						)));
					}
				}
			}
			return Ok(relay);
		}
		// Attempt to determine from nodes
		let mut relay: Option<RelayChain> = None;
		for node in relay_chain.nodes() {
			if let Some(command) = node.command().map(|c| c.as_str()) {
				match &relay {
					Some(relay) =>
						if command.to_lowercase() != relay.binary.name() {
							return Err(Error::UnsupportedCommand(format!(
								"the relay chain command is unsupported: {command}",
							)));
						},
					None => {
						relay = Some(
							relay::from(command, version, runtime_version, chain, cache).await?,
						);
					},
				}
			}
		}
		if let Some(relay) = relay {
			return Ok(relay);
		}
		// Otherwise use default
		Ok(relay::default(version, runtime_version, chain, cache).await?)
	}

	/// The name of the relay chain.
	pub fn relay_chain(&self) -> &str {
		&self.relay_chain.chain
	}

	/// Whether any HRMP channels are to be pre-opened.
	pub fn hrmp_channels(&self) -> bool {
		self.hrmp_channels
	}

	/// Launches the local network.
	pub async fn spawn(&mut self) -> Result<Network<LocalFileSystem>, Error> {
		// Symlink polkadot workers
		let relay_chain_binary_path = self.relay_chain.binary.path();
		if !relay_chain_binary_path.exists() {
			return Err(Error::MissingBinary(self.relay_chain.binary.name().to_string()));
		}
		let cache = relay_chain_binary_path
			.parent()
			.expect("expected relay chain binary path to exist");
		let version = self.relay_chain.binary.version().ok_or_else(|| {
			Error::MissingBinary(format!(
				"Could not determine version for `{}` binary",
				self.relay_chain.binary.name()
			))
		})?;
		for worker in &self.relay_chain.workers {
			let dest = cache.join(worker);
			if dest.exists() {
				remove_symlink_file(&dest)?;
			}
			symlink_file(cache.join(format!("{worker}-{version}")), dest)?;
		}

		// Load from config and spawn network
		let network_config = self.network_config.adapt(&self.relay_chain, &self.parachains)?;
		Ok(network_config.spawn_native().await?)
	}
}

/// The network configuration.
///
/// Network configuration can be provided via [Path] or by using the [NetworkConfigBuilder].
pub struct NetworkConfiguration(NetworkConfig, BTreeSet<u32>);

impl NetworkConfiguration {
	/// Adapts user provided configuration to one with resolved binary paths and which is compatible
	/// with [zombienet-sdk](zombienet_sdk) requirements.
	///
	/// # Arguments
	/// * `relay_chain` - The configuration required to launch the relay chain.
	/// * `parachains` - The configuration required to launch the parachain(s).
	fn adapt(
		&self,
		relay_chain: &RelayChain,
		parachains: &IndexMap<u32, Parachain>,
	) -> Result<NetworkConfig, Error> {
		// Resolve paths to relay binary and chain spec generator
		let binary_path = NetworkConfiguration::resolve_path(&relay_chain.binary.path())?;
		let chain_spec_generator = match &relay_chain.chain_spec_generator {
			None => None,
			Some(path) => Some(format!(
				"{} {}",
				NetworkConfiguration::resolve_path(&path.path())?,
				"{{chainName}}"
			)),
		};

		// Use builder to clone network config, adapting binary paths as necessary
		let mut builder = NetworkConfigBuilder::new()
			.with_relaychain(|relay| {
				let source = self.0.relaychain();
				let nodes = source.nodes();

				let mut builder = relay
					.with_chain(source.chain().as_str())
					.with_default_args(source.default_args().into_iter().cloned().collect())
					// Replace default command with resolved binary path
					.with_default_command(binary_path.as_str());

				// Chain spec
				if let Some(command) = source.chain_spec_command() {
					builder = builder.with_chain_spec_command(command);
				}
				if source.chain_spec_command_is_local() {
					builder = builder.chain_spec_command_is_local(true);
				}
				if let Some(location) = source.chain_spec_path() {
					builder = builder.with_chain_spec_path(location.clone());
				}
				// Configure chain spec generator
				if let Some(command) = chain_spec_generator {
					builder = builder.with_chain_spec_command(command);
				}
				// Overrides: genesis/wasm
				if let Some(genesis) = source.runtime_genesis_patch() {
					builder = builder.with_genesis_overrides(genesis.clone());
				}
				if let Some(location) = source.wasm_override() {
					builder = builder.with_wasm_override(location.clone());
				}

				// Add nodes from source
				let mut builder = builder.with_node(|node| {
					let source = nodes.first().expect("expected at least one node");
					node.with_name(source.name()).with_command(binary_path.as_str())
				});
				for source in nodes.iter().skip(1) {
					builder = builder.with_node(|node| {
						node.with_name(source.name()).with_command(binary_path.as_str())
					});
				}

				builder
			})
			// Add global settings
			.with_global_settings(|settings| {
				settings.with_network_spawn_timeout(1_000).with_node_spawn_timeout(300)
			});

		// Process parachains
		let parachains = &parachains;
		for source in self.0.parachains() {
			let id = source.id();
			let collators = source.collators();
			let para =
				parachains.get(&id).expect("expected parachain existence due to preprocessing");

			// Resolve paths to parachain binary and chain spec generator
			let binary_path = NetworkConfiguration::resolve_path(&para.binary.path())?;
			let chain_spec_generator = match &para.chain_spec_generator {
				None => None,
				Some(path) => Some(format!(
					"{} {}",
					NetworkConfiguration::resolve_path(&path.path())?,
					"{{chainName}}"
				)),
			};

			builder = builder.with_parachain(|builder| {
				let mut builder = builder
					.with_id(id)
					.with_default_args(source.default_args().into_iter().cloned().collect())
					// Replace default command with resolved binary path
					.with_default_command(binary_path.as_str());

				// Chain spec
				if let Some(chain) = source.chain() {
					builder = builder.with_chain(chain.as_str());
				}
				if source.chain_spec_command_is_local() {
					builder = builder.chain_spec_command_is_local(true);
				}
				if let Some(location) = source.chain_spec_path() {
					builder = builder.with_chain_spec_path(location.clone());
				}
				// Configure chain spec generator
				if let Some(command) = chain_spec_generator {
					builder = builder.with_chain_spec_command(command);
				}
				// Overrides: genesis/wasm
				if let Some(genesis) = source.genesis_overrides() {
					builder = builder.with_genesis_overrides(genesis.clone());
				}
				if let Some(location) = source.wasm_override() {
					builder = builder.with_wasm_override(location.clone());
				}
				// Configure whether EVM based
				builder = builder.evm_based(self.1.contains(&id) || source.is_evm_based());

				// Add collators from source
				let mut builder = builder.with_collator(|builder| {
					let source = collators.first().expect("expected at least one collator");
					builder.with_name(source.name()).with_command(binary_path.as_str())
				});
				for source in collators.iter().skip(1) {
					builder = builder.with_collator(|builder| {
						builder.with_name(source.name()).with_command(binary_path.as_str())
					});
				}

				builder
			});
		}

		// Process HRMP channels
		for source in self.0.hrmp_channels() {
			builder = builder.with_hrmp_channel(|channel| {
				channel
					.with_sender(source.sender())
					.with_recipient(source.recipient())
					.with_max_capacity(source.max_capacity())
					.with_max_message_size(source.max_message_size())
			})
		}

		builder
			.build()
			.map_err(|e| Error::Config(format!("could not configure network {:?}", e)))
	}

	/// Resolves the canonical path of a command specified within a network configuration file.
	///
	/// # Arguments
	/// * `path` - The path to be resolved.
	fn resolve_path(path: &Path) -> Result<String, Error> {
		path.canonicalize()
			.map_err(|_| {
				Error::Config(format!("the canonical path of {:?} could not be resolved", path))
			})
			.map(|p| p.to_str().map(|p| p.to_string()))?
			.ok_or_else(|| Error::Config("the path is invalid".into()))
	}
}

impl TryFrom<&Path> for NetworkConfiguration {
	type Error = Error;

	fn try_from(file: &Path) -> Result<Self, Self::Error> {
		if !file.exists() {
			return Err(Error::Config(format!("The {file:?} configuration file was not found")));
		}

		// Parse the file to determine if there are any parachains using `force_decorator`
		let contents = std::fs::read_to_string(file)?;
		let config = contents.parse::<DocumentMut>().map_err(|err| Error::TomlError(err.into()))?;
		let evm_based = config
			.get("parachains")
			.and_then(|p| p.as_array_of_tables())
			.map(|tables| {
				tables
					.iter()
					.filter_map(|table| {
						table
							.get("force_decorator")
							.and_then(|i| i.as_str())
							.filter(|v| *v == "generic-evm")
							.and_then(|_| table.get("id"))
							.and_then(|i| i.as_integer())
							.map(|id| id as u32)
					})
					.collect()
			})
			.unwrap_or_default();

		Ok(NetworkConfiguration(
			NetworkConfig::load_from_toml(
				file.to_str().expect("expected file path to be convertible to string"),
			)
			.map_err(|e| Error::Config(e.to_string()))?,
			evm_based,
		))
	}
}

impl TryFrom<NetworkConfig> for NetworkConfiguration {
	type Error = ();

	fn try_from(value: NetworkConfig) -> Result<Self, Self::Error> {
		Ok(NetworkConfiguration(value, Default::default()))
	}
}

/// The configuration required to launch the relay chain.
struct RelayChain {
	/// The binary used to launch a relay chain node.
	binary: Binary,
	/// The additional workers required by the relay chain node.
	workers: [&'static str; 2],
	/// The name of the chain.
	#[allow(dead_code)]
	chain: String,
	/// If applicable, the binary used to generate a chain specification.
	chain_spec_generator: Option<Binary>,
}

/// The configuration required to launch a parachain.
#[derive(Debug, PartialEq)]
struct Parachain {
	/// The parachain identifier on the local network.
	id: u32,
	/// The binary used to launch a parachain node.
	binary: Binary,
	/// The name of the chain.
	chain: Option<String>,
	/// If applicable, the binary used to generate a chain specification.
	chain_spec_generator: Option<Binary>,
}

impl Parachain {
	/// Initializes the configuration required to launch a parachain using a local binary.
	///
	/// # Arguments
	/// * `id` - The parachain identifier on the local network.
	/// * `path` - The path to the local binary.
	/// * `chain` - The chain specified.
	fn from_local(id: u32, path: PathBuf, chain: Option<&str>) -> Result<Parachain, Error> {
		let name = path
			.file_name()
			.and_then(|f| f.to_str())
			.ok_or_else(|| Error::Config(format!("unable to determine file name for {path:?}")))?
			.to_string();
		// Check if package manifest can be found within path
		let manifest = resolve_manifest(&name, &path)?;
		Ok(Parachain {
			id,
			binary: Binary::Local { name, path, manifest },
			chain: chain.map(|c| c.to_string()),
			chain_spec_generator: None,
		})
	}

	/// Initializes the configuration required to launch a parachain using a binary sourced from the
	/// specified repository.
	///
	/// # Arguments
	/// * `id` - The parachain identifier on the local network.
	/// * `repo` - The repository to be used to source the binary.
	/// * `chain` - The chain specified.
	/// * `cache` - The location used for caching binaries.
	fn from_repository(
		id: u32,
		repo: &Repository,
		chain: Option<&str>,
		cache: &Path,
	) -> Result<Parachain, Error> {
		// Check for GitHub repository to be able to download source as an archive
		if repo.url.host_str().is_some_and(|h| h.to_lowercase() == "github.com") {
			let github = GitHub::parse(repo.url.as_str())?;
			let source = Source::GitHub(SourceCodeArchive {
				owner: github.org,
				repository: github.name,
				reference: repo.reference.clone(),
				manifest: None,
				package: repo.package.clone(),
				artifacts: vec![repo.package.clone()],
			});
			Ok(Parachain {
				id,
				binary: Binary::Source {
					name: repo.package.clone(),
					source,
					cache: cache.to_path_buf(),
				},
				chain: chain.map(|c| c.to_string()),
				chain_spec_generator: None,
			})
		} else {
			Ok(Parachain {
				id,
				binary: Binary::Source {
					name: repo.package.clone(),
					source: Git {
						url: repo.url.clone(),
						reference: repo.reference.clone(),
						manifest: None,
						package: repo.package.clone(),
						artifacts: vec![repo.package.clone()],
					},
					cache: cache.to_path_buf(),
				},
				chain: chain.map(|c| c.to_string()),
				chain_spec_generator: None,
			})
		}
	}
}

/// Attempts to resolve the package manifest from the specified path.
///
/// # Arguments
/// * `package` - The name of the package.
/// * `path` - The path to start searching.
fn resolve_manifest(package: &str, path: &Path) -> Result<Option<PathBuf>, Error> {
	let matches_package = |config: &DocumentMut| {
		config
			.get("package")
			.and_then(|i| i.as_table())
			.and_then(|t| t.get("name"))
			.and_then(|i| i.as_str()) ==
			Some(package)
	};

	let mut manifest = Some(path);
	'outer: while let Some(path) = manifest {
		let manifest_path = path.join("Cargo.toml");
		if !manifest_path.exists() {
			manifest = path.parent();
			continue;
		}
		let contents = std::fs::read_to_string(&manifest_path)?;
		let config = contents.parse::<DocumentMut>().map_err(|err| Error::TomlError(err.into()))?;
		// Check if package manifest
		if matches_package(&config) {
			break 'outer;
		}
		// Check if package defined as a workspace member
		if let Some(members) = config
			.get("workspace")
			.and_then(|i| i.as_table())
			.and_then(|t| t.get("members"))
			.and_then(|m| m.as_array())
			.map(|a| a.iter().filter_map(|v| v.as_str()))
		{
			// Check manifest of each member
			for member in members {
				let member_path = path.join(member);
				for entry in glob(member_path.to_string_lossy().as_ref())
					.expect("expected valid glob for workspace member")
					.filter_map(Result::ok)
				{
					let manifest_path = entry.join("Cargo.toml");
					if manifest_path.exists() {
						let contents = std::fs::read_to_string(&manifest_path)?;
						let config = contents
							.parse::<DocumentMut>()
							.map_err(|err| Error::TomlError(err.into()))?;
						if matches_package(&config) {
							break 'outer;
						}
					}
				}
			}
		};
		manifest = path.parent();
	}
	Ok(manifest.map(|p| p.join("Cargo.toml")))
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::{
		env::current_dir,
		fs::{create_dir_all, remove_dir, remove_file, File},
		io::Write,
	};
	use tempfile::{tempdir, Builder};

	pub(crate) const VERSION: &str = "stable2409";

	mod zombienet {
		use super::*;
		use pop_common::Status;

		pub(crate) struct Output;
		impl Status for Output {
			fn update(&self, status: &str) {
				println!("{status}")
			}
		}

		#[tokio::test]
		async fn new_with_relay_only_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), Some(VERSION), None, None, None, None)
					.await?;

			let relay_chain = &zombienet.relay_chain.binary;
			assert_eq!(relay_chain.name(), "polkadot");
			assert_eq!(relay_chain.path(), temp_dir.path().join(format!("polkadot-{VERSION}")));
			assert_eq!(relay_chain.version().unwrap(), VERSION);
			assert!(matches!(
				relay_chain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(VERSION.to_string())
			));
			assert!(zombienet.parachains.is_empty());
			assert_eq!(zombienet.relay_chain(), "paseo-local");
			assert!(!zombienet.hrmp_channels());
			Ok(())
		}

		#[tokio::test]
		async fn new_with_relay_chain_spec_generator_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
"#
			)?;
			let version = "v1.3.3";

			let zombienet =
				Zombienet::new(&cache, config.path(), None, Some(version), None, None, None)
					.await?;

			assert_eq!(zombienet.relay_chain.chain, "paseo-local");
			let chain_spec_generator = &zombienet.relay_chain.chain_spec_generator.unwrap();
			assert_eq!(chain_spec_generator.name(), "paseo-chain-spec-generator");
			assert_eq!(
				chain_spec_generator.path(),
				temp_dir.path().join(format!("paseo-chain-spec-generator-{version}"))
			);
			assert_eq!(chain_spec_generator.version().unwrap(), version);
			assert!(matches!(
				chain_spec_generator,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(version.to_string())
			));
			assert!(zombienet.parachains.is_empty());
			Ok(())
		}

		#[tokio::test]
		async fn new_with_default_command_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
default_command = "./bin-stable2409/polkadot"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), Some(VERSION), None, None, None, None)
					.await?;

			let relay_chain = &zombienet.relay_chain.binary;
			assert_eq!(relay_chain.name(), "polkadot");
			assert_eq!(relay_chain.path(), temp_dir.path().join(format!("polkadot-{VERSION}")));
			assert_eq!(relay_chain.version().unwrap(), VERSION);
			assert!(matches!(
				relay_chain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(VERSION.to_string())
			));
			assert!(zombienet.parachains.is_empty());
			Ok(())
		}

		#[tokio::test]
		async fn new_with_node_command_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
validator = true
command = "polkadot"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), Some(VERSION), None, None, None, None)
					.await?;

			let relay_chain = &zombienet.relay_chain.binary;
			assert_eq!(relay_chain.name(), "polkadot");
			assert_eq!(relay_chain.path(), temp_dir.path().join(format!("polkadot-{VERSION}")));
			assert_eq!(relay_chain.version().unwrap(), VERSION);
			assert!(matches!(
				relay_chain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(VERSION.to_string())
			));
			assert!(zombienet.parachains.is_empty());
			Ok(())
		}

		#[tokio::test]
		async fn new_ensures_node_commands_valid() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
validator = true
command = "polkadot"

[[relaychain.nodes]]
name = "bob"
validator = true
command = "polkadot-stable2409"
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await,
				Err(Error::UnsupportedCommand(error))
				if error == "the relay chain command is unsupported: polkadot-stable2409"
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_ensures_node_command_valid() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
default_command = "polkadot"

[[relaychain.nodes]]
name = "alice"
validator = true
command = "polkadot-stable2409"
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await,
				Err(Error::UnsupportedCommand(error))
				if error == "the relay chain command is unsupported: polkadot-stable2409"
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_system_chain_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"
"#
			)?;
			let system_parachain_version = "stable2407";

			let zombienet = Zombienet::new(
				&cache,
				config.path(),
				Some(VERSION),
				None,
				Some(system_parachain_version),
				None,
				None,
			)
			.await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let system_parachain = &zombienet.parachains.get(&1000).unwrap().binary;
			assert_eq!(system_parachain.name(), "polkadot-parachain");
			assert_eq!(
				system_parachain.path(),
				temp_dir.path().join(format!("polkadot-parachain-{system_parachain_version}"))
			);
			assert_eq!(system_parachain.version().unwrap(), system_parachain_version);
			assert!(matches!(
				system_parachain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(system_parachain_version.to_string())
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_system_chain_spec_generator_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, Some(VERSION), None)
					.await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let system_parachain = &zombienet.parachains.get(&1000).unwrap();
			assert_eq!(system_parachain.chain.as_ref().unwrap(), "asset-hub-paseo-local");
			let chain_spec_generator = system_parachain.chain_spec_generator.as_ref().unwrap();
			assert_eq!(chain_spec_generator.name(), "paseo-chain-spec-generator");
			assert_eq!(
				chain_spec_generator.path(),
				temp_dir.path().join(format!("paseo-chain-spec-generator-{VERSION}"))
			);
			assert_eq!(chain_spec_generator.version().unwrap(), VERSION);
			assert!(matches!(
				chain_spec_generator,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(VERSION.to_string())
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_pop_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let pop = &zombienet.parachains.get(&4385).unwrap().binary;
			let version = pop.latest().unwrap();
			assert_eq!(pop.name(), "pop-node");
			assert_eq!(pop.path(), temp_dir.path().join(format!("pop-node-{version}")));
			assert_eq!(pop.version().unwrap(), version);
			assert!(matches!(
				pop,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(version.to_string())
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_pop_version_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
			)?;
			let version = "v1.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path(),
				None,
				None,
				None,
				None,
				Some(&vec![format!("https://github.com/r0gue-io/pop-node#{version}")]),
			)
			.await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let pop = &zombienet.parachains.get(&4385).unwrap().binary;
			assert_eq!(pop.name(), "pop-node");
			assert_eq!(pop.path(), temp_dir.path().join(format!("pop-node-{version}")));
			assert_eq!(pop.version().unwrap(), version);
			assert!(matches!(
				pop,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(version.to_string())
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_local_parachain_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 2000
default_command = "./target/release/parachain-template-node"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let pop = &zombienet.parachains.get(&2000).unwrap().binary;
			assert_eq!(pop.name(), "parachain-template-node");
			assert_eq!(pop.path(), Path::new("./target/release/parachain-template-node"));
			assert_eq!(pop.version(), None);
			assert!(matches!(pop, Binary::Local { .. }));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_local_parachain_without_path_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "rococo-local"

[[parachains]]
id = 1000

[parachains.collator]
name = "collator"
command = "parachain-template-node"

[[parachains]]
id = 2000

[parachains.collator]
name = "collator"
command = "substrate-contracts-node"
"#
			)?;
			// Expecting failure since no custom path is provided and binaries don't exist in the
			// default build directory.
			assert!(matches!(
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await,
				Err(Error::MissingBinary(command))
				if command == "parachain-template-node"
			));
			// Create the binaries in the default build directory.
			let parachain_template = PathBuf::from("target/release/parachain-template-node");
			create_dir_all(parachain_template.parent().unwrap())?;
			File::create(&parachain_template)?;
			// Ensure the the binary is detected in the debug profile too.
			let parachain_contracts_template =
				PathBuf::from("target/debug/substrate-contracts-node");
			create_dir_all(parachain_contracts_template.parent().unwrap())?;
			File::create(&parachain_contracts_template)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			// Remove the binaries created above after Zombienet initialization, as they are no
			// longer needed.
			remove_file(&parachain_template)?;
			remove_file(&parachain_contracts_template)?;
			remove_dir(parachain_template.parent().unwrap())?;
			remove_dir(parachain_contracts_template.parent().unwrap())?;

			assert_eq!(zombienet.parachains.len(), 2);
			let parachain = &zombienet.parachains.get(&1000).unwrap().binary;
			assert_eq!(parachain.name(), "parachain-template-node");
			assert_eq!(parachain.path(), Path::new("./target/release/parachain-template-node"));
			assert_eq!(parachain.version(), None);
			assert!(matches!(parachain, Binary::Local { .. }));
			let contract_parachain = &zombienet.parachains.get(&2000).unwrap().binary;
			assert_eq!(contract_parachain.name(), "substrate-contracts-node");
			assert_eq!(
				contract_parachain.path(),
				Path::new("./target/debug/substrate-contracts-node")
			);
			assert_eq!(contract_parachain.version(), None);
			assert!(matches!(contract_parachain, Binary::Local { .. }));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_collator_command_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 2000

[[parachains.collators]]
name = "collator-01"
command = "./target/release/parachain-template-node"
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let pop = &zombienet.parachains.get(&2000).unwrap().binary;
			assert_eq!(pop.name(), "parachain-template-node");
			assert_eq!(pop.path(), Path::new("./target/release/parachain-template-node"));
			assert_eq!(pop.version(), None);
			assert!(matches!(pop, Binary::Local { .. }));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_moonbeam_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 2000
default_command = "moonbeam"
"#
			)?;
			let version = "v0.38.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path(),
				None,
				None,
				None,
				None,
				Some(&vec![format!("https://github.com/moonbeam-foundation/moonbeam#{version}")]),
			)
			.await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let pop = &zombienet.parachains.get(&2000).unwrap().binary;
			assert_eq!(pop.name(), "moonbeam");
			assert_eq!(pop.path(), temp_dir.path().join(format!("moonbeam-{version}")));
			assert_eq!(pop.version().unwrap(), version);
			assert!(matches!(
				pop,
				Binary::Source { source: Source::GitHub(SourceCodeArchive { reference, .. }), .. }
				if *reference == Some(version.to_string())
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_with_hrmp_channels_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"

[[parachains]]
id = 4385
default_command = "pop-node"

[[hrmp_channels]]
sender = 4385
recipient = 1000
max_capacity = 1000
max_message_size = 8000
"#
			)?;

			let zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;

			assert!(zombienet.hrmp_channels());
			Ok(())
		}

		#[tokio::test]
		async fn new_ensures_parachain_id_exists() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await,
				Err(Error::Config(error))
				if error == "TOML parse error at line 5, column 1\n  |\n5 | [[parachains]]\n  | ^^^^^^^^^^^^^^\nmissing field `id`\n"
			));
			Ok(())
		}

		#[tokio::test]
		async fn new_handles_missing_binary() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 404
default_command = "missing-binary"
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await,
				Err(Error::MissingBinary(command))
				if command == "missing-binary"
			));
			Ok(())
		}

		#[tokio::test]
		async fn binaries_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"

[[parachains]]
id = 2000
default_command = "./target/release/parachain-template-node"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
			)?;

			let mut zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			assert_eq!(zombienet.binaries().count(), 6);
			Ok(())
		}

		#[tokio::test]
		async fn binaries_includes_chain_spec_generators() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"
"#
			)?;

			let mut zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			assert_eq!(zombienet.binaries().count(), 4);
			Ok(())
		}

		#[tokio::test]
		async fn spawn_ensures_relay_chain_binary_exists() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
"#
			)?;

			let mut zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			assert!(matches!(
				zombienet.spawn().await,
				Err(Error::MissingBinary(error))
				if error == "polkadot"
			));
			Ok(())
		}

		#[tokio::test]
		async fn spawn_ensures_relay_chain_version_set() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
"#
			)?;
			File::create(cache.join("polkadot"))?;

			let mut zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			if let Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. } =
				&mut zombienet.relay_chain.binary
			{
				*tag = None
			}
			assert!(matches!(
				zombienet.spawn().await,
				Err(Error::MissingBinary(error))
				if error == "Could not determine version for `polkadot` binary",
			));
			Ok(())
		}

		#[tokio::test]
		async fn spawn_symlinks_workers() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"
"#
			)?;
			File::create(cache.join(format!("polkadot-{VERSION}")))?;
			File::create(cache.join(format!("polkadot-execute-worker-{VERSION}")))?;
			File::create(cache.join(format!("polkadot-prepare-worker-{VERSION}")))?;

			let mut zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			assert!(!cache.join("polkadot-execute-worker").exists());
			assert!(!cache.join("polkadot-prepare-worker").exists());
			let _ = zombienet.spawn().await;
			assert!(cache.join("polkadot-execute-worker").exists());
			assert!(cache.join("polkadot-prepare-worker").exists());
			let _ = zombienet.spawn().await;
			Ok(())
		}

		#[tokio::test]
		async fn spawn_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
validator = true
"#
			)?;

			let mut zombienet =
				Zombienet::new(&cache, config.path(), None, None, None, None, None).await?;
			for b in zombienet.binaries() {
				b.source(true, &Output, true).await?;
			}

			zombienet.spawn().await?;
			Ok(())
		}
	}

	mod network_config {
		use super::*;
		use std::{
			fs::{create_dir_all, File},
			io::Write,
			path::PathBuf,
		};
		use tempfile::{tempdir, Builder};

		#[test]
		fn initialising_from_file_fails_when_missing() {
			assert!(NetworkConfiguration::try_from(PathBuf::new().as_path()).is_err());
		}

		#[test]
		fn initialising_from_file_fails_when_malformed() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(config.as_file(), "[")?;
			assert!(matches!(
				NetworkConfiguration::try_from(config.path()),
				Err(Error::TomlError(..))
			));
			Ok(())
		}

		#[test]
		fn initialising_from_file_fails_when_relaychain_missing() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			assert!(matches!(
				NetworkConfiguration::try_from(config.path()),
				Err(Error::Config(error)) if error == "Relay chain does not exist."
			));
			Ok(())
		}

		#[test]
		fn initializes_relay_from_file() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
				[relaychain]
				chain = "paseo-local"
				default_command = "polkadot"
				[[relaychain.nodes]]
				name = "alice"
			"#
			)?;
			let network_config = NetworkConfiguration::try_from(config.path())?;
			let relay_chain = network_config.0.relaychain();
			assert_eq!("paseo-local", relay_chain.chain().as_str());
			assert_eq!(Some("polkadot"), relay_chain.default_command().map(|c| c.as_str()));
			let nodes = relay_chain.nodes();
			assert_eq!("alice", nodes.get(0).unwrap().name());
			assert!(network_config.0.parachains().is_empty());
			Ok(())
		}

		#[test]
		fn initializes_parachains_from_file() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
				[relaychain]
				chain = "paseo-local"
				[[parachains]]
				id = 2000
				default_command = "node"
			"#
			)?;
			let network_config = NetworkConfiguration::try_from(config.path())?;
			let parachains = network_config.0.parachains();
			let para_2000 = parachains.get(0).unwrap();
			assert_eq!(2000, para_2000.id());
			assert_eq!(Some("node"), para_2000.default_command().map(|c| c.as_str()));
			Ok(())
		}

		#[test]
		fn adapt_works() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
command = "polkadot"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"

[[parachains.collators]]
name = "asset-hub"
command = "polkadot-parachain"

[[parachains]]
id = 2000
default_command = "pop-node"

[[parachains.collators]]
name = "pop"
command = "pop-node"

[[parachains]]
id = 2001
default_command = "./target/release/parachain-template-node"

[[parachains.collators]]
name = "collator-2001"
command = "./target/release/parachain-template-node"

[[parachains]]
id = 2002
default_command = "./target/release/parachain-template-node"

[parachains.collator]
name = "collator-2002"
command = "./target/release/parachain-template-node"
"#
			)?;
			let network_config = NetworkConfiguration::try_from(config.path())?;

			let relay_chain_binary = Builder::new().tempfile()?;
			let relay_chain = relay_chain_binary.path();
			File::create(&relay_chain)?;
			let system_chain_binary = Builder::new().tempfile()?;
			let system_chain = system_chain_binary.path();
			File::create(&system_chain)?;
			let pop_binary = Builder::new().tempfile()?;
			let pop = pop_binary.path();
			File::create(&pop)?;
			let parachain_template_node = Builder::new().tempfile()?;
			let parachain_template = parachain_template_node.path();
			create_dir_all(parachain_template.parent().unwrap())?;
			File::create(&parachain_template)?;

			let adapted = network_config.adapt(
				&RelayChain {
					binary: Binary::Local {
						name: "polkadot".to_string(),
						path: relay_chain.to_path_buf(),
						manifest: None,
					},
					workers: ["polkadot-execute-worker", ""],
					chain: "paseo-local".to_string(),
					chain_spec_generator: None,
				},
				&[
					(
						1000,
						Parachain {
							id: 1000,
							binary: Binary::Local {
								name: "polkadot-parachain".to_string(),
								path: system_chain.to_path_buf(),
								manifest: None,
							},
							chain: None,
							chain_spec_generator: None,
						},
					),
					(
						2000,
						Parachain {
							id: 2000,
							binary: Binary::Local {
								name: "pop-node".to_string(),
								path: pop.to_path_buf(),
								manifest: None,
							},
							chain: None,
							chain_spec_generator: None,
						},
					),
					(
						2001,
						Parachain {
							id: 2001,
							binary: Binary::Local {
								name: "parachain-template-node".to_string(),
								path: parachain_template.to_path_buf(),
								manifest: None,
							},
							chain: None,
							chain_spec_generator: None,
						},
					),
					(
						2002,
						Parachain {
							id: 2002,
							binary: Binary::Local {
								name: "parachain-template-node".to_string(),
								path: parachain_template.to_path_buf(),
								manifest: None,
							},
							chain: None,
							chain_spec_generator: None,
						},
					),
				]
				.into(),
			)?;

			let contents = adapted.dump_to_toml().unwrap();
			println!("{contents}");
			assert_eq!(
				contents,
				format!(
					r#"[settings]
timeout = 1000
node_spawn_timeout = 300

[relaychain]
chain = "paseo-local"
default_command = "{0}"

[[relaychain.nodes]]
name = "alice"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"
add_to_genesis = true
balance = 2000000000000
default_command = "{1}"
cumulus_based = true
evm_based = false

[[parachains.collators]]
name = "asset-hub"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000

[[parachains]]
id = 2000
add_to_genesis = true
balance = 2000000000000
default_command = "{2}"
cumulus_based = true
evm_based = false

[[parachains.collators]]
name = "pop"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000

[[parachains]]
id = 2001
add_to_genesis = true
balance = 2000000000000
default_command = "{3}"
cumulus_based = true
evm_based = false

[[parachains.collators]]
name = "collator-2001"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000

[[parachains]]
id = 2002
add_to_genesis = true
balance = 2000000000000
default_command = "{3}"
cumulus_based = true
evm_based = false

[[parachains.collators]]
name = "collator-2002"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000
"#,
					relay_chain.canonicalize()?.to_str().unwrap(),
					system_chain.canonicalize()?.to_str().unwrap(),
					pop.canonicalize()?.to_str().unwrap(),
					parachain_template.canonicalize()?.to_str().unwrap()
				)
			);
			Ok(())
		}

		#[test]
		fn adapt_with_chain_spec_generator_works() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
command = "polkadot"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"

[[parachains.collators]]
name = "asset-hub"
command = "polkadot-parachain"

"#
			)?;
			let network_config = NetworkConfiguration::try_from(config.path())?;

			let relay_chain_binary = Builder::new().tempfile()?;
			let relay_chain = relay_chain_binary.path();
			File::create(&relay_chain)?;
			let relay_chain_spec_generator = Builder::new().tempfile()?;
			let relay_chain_spec_generator = relay_chain_spec_generator.path();
			File::create(&relay_chain_spec_generator)?;
			let system_chain_binary = Builder::new().tempfile()?;
			let system_chain = system_chain_binary.path();
			File::create(&system_chain)?;
			let system_chain_spec_generator = Builder::new().tempfile()?;
			let system_chain_spec_generator = system_chain_spec_generator.path();
			File::create(&system_chain_spec_generator)?;

			let adapted = network_config.adapt(
				&RelayChain {
					binary: Binary::Local {
						name: "polkadot".to_string(),
						path: relay_chain.to_path_buf(),
						manifest: None,
					},
					workers: ["polkadot-execute-worker", ""],
					chain: "paseo-local".to_string(),
					chain_spec_generator: Some(Binary::Local {
						name: "paseo-chain-spec-generator".to_string(),
						path: relay_chain_spec_generator.to_path_buf(),
						manifest: None,
					}),
				},
				&[(
					1000,
					Parachain {
						id: 1000,
						binary: Binary::Local {
							name: "polkadot-parachain".to_string(),
							path: system_chain.to_path_buf(),
							manifest: None,
						},
						chain: Some("asset-hub-paseo-local".to_string()),
						chain_spec_generator: Some(Binary::Local {
							name: "paseo-chain-spec-generator".to_string(),
							path: system_chain_spec_generator.to_path_buf(),
							manifest: None,
						}),
					},
				)]
				.into(),
			)?;

			let contents = adapted.dump_to_toml().unwrap();
			println!("{contents}");
			assert_eq!(
				contents,
				format!(
					r#"[settings]
timeout = 1000
node_spawn_timeout = 300

[relaychain]
chain = "paseo-local"
default_command = "{0}"
chain_spec_command = "{1} {2}"

[[relaychain.nodes]]
name = "alice"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"
add_to_genesis = true
balance = 2000000000000
default_command = "{3}"
chain_spec_command = "{4} {2}"
cumulus_based = true
evm_based = false

[[parachains.collators]]
name = "asset-hub"
validator = true
invulnerable = true
bootnode = false
balance = 2000000000000
"#,
					relay_chain.canonicalize()?.to_str().unwrap(),
					relay_chain_spec_generator.canonicalize()?.to_str().unwrap(),
					"{{chainName}}",
					system_chain.canonicalize()?.to_str().unwrap(),
					system_chain_spec_generator.canonicalize()?.to_str().unwrap(),
				)
			);
			Ok(())
		}

		#[test]
		fn resolves_path() -> Result<(), Error> {
			let working_dir = tempdir()?;
			let path = working_dir.path().join("./target/release/node");
			assert!(
				matches!(NetworkConfiguration::resolve_path(&path), Err(Error::Config(message))
						if message == format!("the canonical path of {:?} could not be resolved", path)
				)
			);

			create_dir_all(path.parent().unwrap())?;
			File::create(&path)?;
			assert_eq!(
				NetworkConfiguration::resolve_path(&path)?,
				path.canonicalize()?.to_str().unwrap().to_string()
			);
			Ok(())
		}
	}

	mod parachain {
		use super::*;
		use pop_common::sourcing::GitHub::SourceCodeArchive;
		use std::path::PathBuf;

		#[test]
		fn initializes_from_local_binary() -> Result<(), Error> {
			let name = "parachain-template-node";
			let command = PathBuf::from("./target/release").join(&name);
			assert_eq!(
				Parachain::from_local(2000, command.clone(), Some("dev"))?,
				Parachain {
					id: 2000,
					binary: Binary::Local { name: name.to_string(), path: command, manifest: None },
					chain: Some("dev".to_string()),
					chain_spec_generator: None,
				}
			);
			Ok(())
		}

		#[test]
		fn initializes_from_local_package() -> Result<(), Error> {
			let name = "pop-parachains";
			let command = PathBuf::from("./target/release").join(&name);
			assert_eq!(
				Parachain::from_local(2000, command.clone(), Some("dev"))?,
				Parachain {
					id: 2000,
					binary: Binary::Local {
						name: name.to_string(),
						path: command,
						manifest: Some(PathBuf::from("./Cargo.toml"))
					},
					chain: Some("dev".to_string()),
					chain_spec_generator: None,
				}
			);
			Ok(())
		}

		#[test]
		fn initializes_from_git() -> Result<(), Error> {
			let repo = Repository::parse("https://git.com/r0gue-io/pop-node#v1.0")?;
			let cache = tempdir()?;
			assert_eq!(
				Parachain::from_repository(2000, &repo, Some("dev"), cache.path())?,
				Parachain {
					id: 2000,
					binary: Binary::Source {
						name: "pop-node".to_string(),
						source: Git {
							url: repo.url,
							reference: repo.reference,
							manifest: None,
							package: "pop-node".to_string(),
							artifacts: vec!["pop-node".to_string()],
						},
						cache: cache.path().to_path_buf(),
					},
					chain: Some("dev".to_string()),
					chain_spec_generator: None,
				}
			);
			Ok(())
		}

		#[test]
		fn initializes_from_github() -> Result<(), Error> {
			let repo = Repository::parse("https://github.com/r0gue-io/pop-node#v1.0")?;
			let cache = tempdir()?;
			assert_eq!(
				Parachain::from_repository(2000, &repo, Some("dev"), cache.path())?,
				Parachain {
					id: 2000,
					binary: Binary::Source {
						name: "pop-node".to_string(),
						source: Source::GitHub(SourceCodeArchive {
							owner: "r0gue-io".to_string(),
							repository: "pop-node".to_string(),
							reference: Some("v1.0".to_string()),
							manifest: None,
							package: "pop-node".to_string(),
							artifacts: vec!["pop-node".to_string()],
						}),
						cache: cache.path().to_path_buf(),
					},
					chain: Some("dev".to_string()),
					chain_spec_generator: None,
				},
			);
			Ok(())
		}
	}

	#[test]
	fn resolve_manifest_works() -> Result<()> {
		let current_dir = current_dir()?;
		// Crate
		assert_eq!(
			current_dir.join("Cargo.toml"),
			resolve_manifest("pop-parachains", &current_dir)?.unwrap()
		);
		// Workspace
		assert_eq!(
			current_dir.join("../../Cargo.toml").canonicalize()?,
			resolve_manifest("pop-cli", &current_dir)?.unwrap()
		);
		Ok(())
	}
}
