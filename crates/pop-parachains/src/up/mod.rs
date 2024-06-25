// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::git::GitHub};
use glob::glob;
use indexmap::IndexMap;
use sourcing::{GitHub::*, Source, Source::*};
use std::{
	fmt::Debug,
	fs::write,
	iter::once,
	path::{Path, PathBuf},
};
use symlink::{remove_symlink_file, symlink_file};
use tempfile::{Builder, NamedTempFile};
use toml_edit::{value, ArrayOfTables, DocumentMut, Formatted, Item, Table, Value};
use url::Url;
use zombienet_sdk::{Network, NetworkConfig, NetworkConfigExt};
use zombienet_support::fs::local::LocalFileSystem;

mod chain_specs;
mod parachains;
mod relay;
mod sourcing;

/// Configuration to launch a local network.
pub struct Zombienet {
	/// The config to be used to launch a network.
	network_config: NetworkConfiguration,
	/// The configuration required to launch the relay chain.
	relay_chain: RelayChain,
	/// The configuration required to launch parachains.
	parachains: IndexMap<u32, Parachain>,
}

impl Zombienet {
	/// Initializes the configuration for launching a local network.
	///
	/// # Arguments
	/// * `cache` - The location used for caching binaries.
	/// * `network_config` - The configuration file to be used to launch a network.
	/// * `relay_chain_version` - The specific binary version used for the relay chain (`None` will use the latest available version).
	/// * `relay_chain_runtime_version` - The specific runtime version used for the relay chain runtime (`None` will use the latest available version).
	/// * `system_parachain_version` - The specific binary version used for system parachains (`None` will use the latest available version).
	/// * `system_parachain_runtime_version` - The specific runtime version used for system parachains (`None` will use the latest available version).
	/// * `parachains` - The parachain(s) specified.
	pub async fn new(
		cache: &Path,
		network_config: &str,
		relay_chain_version: Option<&str>,
		relay_chain_runtime_version: Option<&str>,
		system_parachain_version: Option<&str>,
		system_parachain_runtime_version: Option<&str>,
		parachains: Option<&Vec<String>>,
	) -> Result<Self, Error> {
		// Parse network config
		let network_config = NetworkConfiguration::from(network_config)?;
		// Determine relay and parachain requirements based on arguments and config
		let relay_chain = Self::relay_chain(
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
		Ok(Self { network_config, relay_chain, parachains })
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
			.filter_map(|b| b)
	}

	/// Determine parachain configuration based on specified version and network configuration.
	///
	/// # Arguments
	/// * `relay_chain` - The configuration required to launch the relay chain.
	/// * `system_parachain_version` - The specific binary version used for system parachains (`None` will use the latest available version).
	/// * `system_parachain_runtime_version` - The specific runtime version used for system parachains (`None` will use the latest available version).
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
		let Some(tables) = network_config.parachains() else {
			return Ok(IndexMap::default());
		};

		let mut paras: IndexMap<u32, Parachain> = IndexMap::new();
		'outer: for table in tables {
			let id = table
				.get("id")
				.and_then(|i| i.as_integer())
				.ok_or_else(|| Error::Config("expected `parachain` to have `id`".into()))?
				as u32;

			let chain = table.get("chain").and_then(|i| i.as_str());

			let command = NetworkConfiguration::default_command(table)
				.cloned()
				.or_else(|| {
					// Check if any collators define command
					if let Some(collators) =
						table.get("collators").and_then(|p| p.as_array_of_tables())
					{
						for collator in collators.iter() {
							if let Some(command) =
								NetworkConfiguration::command(collator).and_then(|i| i.as_str())
							{
								return Some(Item::Value(Value::String(Formatted::new(
									command.into(),
								))));
							}
						}
					}

					// Otherwise default to polkadot-parachain
					Some(Item::Value(Value::String(Formatted::new("polkadot-parachain".into()))))
				})
				.expect("missing default_command set above")
				.as_str()
				.expect("expected parachain command to be a string")
				.to_lowercase();

			// Check if system parachain
			if let Some(parachain) = parachains::system(
				id,
				&command,
				system_parachain_version,
				system_parachain_runtime_version,
				&relay_chain.binary.version().expect("expected relay chain to have version"),
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
					.filter_map(|r| (r.package == command).then(|| r.reference.as_ref()).flatten())
					.nth(0)
					.map(|v| v.as_str())
			});
			if let Some(parachain) = parachains::from(id, &command, version, chain, cache).await? {
				paras.insert(id, parachain);
				continue;
			}

			// Check if parachain binary source specified as an argument
			if let Some(parachains) = parachains.as_ref() {
				for repo in parachains.iter().filter(|r| command == r.package) {
					paras.insert(id, Parachain::from_repository(id, repo, chain, cache)?);
					continue 'outer;
				}
			}

			// Check if command references a local binary
			if ["./", "../", "/"].iter().any(|p| command.starts_with(p)) {
				paras.insert(id, Parachain::from_local(id, command.into(), chain)?);
				continue;
			}

			return Err(Error::MissingBinary(command));
		}
		Ok(paras)
	}

	/// Determines relay chain configuration based on specified version and network configuration.
	///
	/// # Arguments
	/// * `version` - The specific binary version used for the relay chain (`None` will use the latest available version).
	/// * `runtime_version` - The specific runtime version used for the relay chain runtime (`None` will use the latest available version).
	/// * `network_config` - The network configuration to be used to launch a network.
	/// * `cache` - The location used for caching binaries.
	async fn relay_chain(
		version: Option<&str>,
		runtime_version: Option<&str>,
		network_config: &NetworkConfiguration,
		cache: &Path,
	) -> Result<RelayChain, Error> {
		// Attempt to determine relay from configuration
		let relay_chain = network_config.relay_chain()?;
		let chain = relay_chain.get("chain").and_then(|i| i.as_str());
		if let Some(default_command) =
			NetworkConfiguration::default_command(relay_chain).and_then(|c| c.as_str())
		{
			let relay =
				relay::from(default_command, version, runtime_version, chain, cache).await?;
			// Validate any node config is supported
			if let Some(nodes) = NetworkConfiguration::nodes(relay_chain) {
				for node in nodes {
					if let Some(command) =
						NetworkConfiguration::command(node).and_then(|c| c.as_str())
					{
						if command.to_lowercase() != relay.binary.name() {
							return Err(Error::UnsupportedCommand(format!(
								"the relay chain command is unsupported: {command}",
							)));
						}
					}
				}
			}
			return Ok(relay);
		}
		// Attempt to determine from nodes
		if let Some(nodes) = NetworkConfiguration::nodes(relay_chain) {
			let mut relay: Option<RelayChain> = None;
			for node in nodes {
				if let Some(command) = NetworkConfiguration::command(node).and_then(|c| c.as_str())
				{
					match &relay {
						Some(relay) => {
							if command.to_lowercase() != relay.binary.name() {
								return Err(Error::UnsupportedCommand(format!(
									"the relay chain command is unsupported: {command}",
								)));
							}
						},
						None => {
							relay = Some(
								relay::from(command, version, runtime_version, chain, cache)
									.await?,
							);
						},
					}
				}
			}
			if let Some(relay) = relay {
				return Ok(relay);
			}
		}
		// Otherwise use default
		return Ok(relay::default(version, runtime_version, chain, cache).await?);
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
		let config = self.network_config.configure(&self.relay_chain, &self.parachains)?;
		let path = config.path().to_str().expect("temp config file should have a path").into();
		let network_config = NetworkConfig::load_from_toml(path)?;
		Ok(network_config.spawn_native().await?)
	}
}

/// The network configuration.
struct NetworkConfiguration(DocumentMut);

impl NetworkConfiguration {
	/// Initializes the network configuration from the specified file.
	///
	/// # Arguments
	/// * `file` - The network configuration file.
	fn from(file: impl AsRef<Path>) -> Result<Self, Error> {
		let file = file.as_ref();
		if !file.exists() {
			return Err(Error::Config(format!("The {file:?} configuration file was not found")));
		}
		let contents = std::fs::read_to_string(&file)?;
		let config = contents.parse::<DocumentMut>().map_err(|err| Error::TomlError(err.into()))?;
		let network_config = NetworkConfiguration(config);
		network_config.relay_chain()?;
		Ok(network_config)
	}

	/// Returns the `relaychain` configuration.
	fn relay_chain(&self) -> Result<&Table, Error> {
		self.0
			.get("relaychain")
			.and_then(|i| i.as_table())
			.ok_or_else(|| Error::Config("expected `relaychain`".into()))
	}

	/// Returns the `relaychain` configuration.
	fn relay_chain_mut(&mut self) -> Result<&mut Table, Error> {
		self.0
			.get_mut("relaychain")
			.and_then(|i| i.as_table_mut())
			.ok_or_else(|| Error::Config("expected `relaychain`".into()))
	}

	/// Returns the `parachains` configuration.
	fn parachains(&self) -> Option<&ArrayOfTables> {
		self.0.get("parachains").and_then(|p| p.as_array_of_tables())
	}

	/// Returns the `parachains` configuration.
	fn parachains_mut(&mut self) -> Option<&mut ArrayOfTables> {
		self.0.get_mut("parachains").and_then(|p| p.as_array_of_tables_mut())
	}

	/// Returns the `command` configuration.
	fn command(config: &Table) -> Option<&Item> {
		config.get("command")
	}

	/// Returns the `command` configuration.
	fn command_mut(config: &mut Table) -> Option<&mut Item> {
		config.get_mut("command")
	}

	/// Returns the `default_command` configuration.
	fn default_command(config: &Table) -> Option<&Item> {
		config.get("default_command")
	}

	/// Returns the `nodes` configuration.
	fn nodes(relay_chain: &Table) -> Option<&ArrayOfTables> {
		relay_chain.get("nodes").and_then(|i| i.as_array_of_tables())
	}

	/// Returns the `nodes` configuration.
	fn nodes_mut(relay_chain: &mut Table) -> Option<&mut ArrayOfTables> {
		relay_chain.get_mut("nodes").and_then(|i| i.as_array_of_tables_mut())
	}

	/// Adapts user provided configuration file to one with resolved binary paths and which is
	/// compatible with current zombienet-sdk requirements.
	///
	/// # Arguments
	/// * `relay_chain` - The configuration required to launch the relay chain.
	/// * `parachains` - The configuration required to launch the parachain(s).
	fn configure(
		&mut self,
		relay_chain: &RelayChain,
		parachains: &IndexMap<u32, Parachain>,
	) -> Result<NamedTempFile, Error> {
		// Add zombienet-sdk specific settings if missing
		let settings = self
			.0
			.entry("settings")
			.or_insert(Item::Table(Table::new()))
			.as_table_mut()
			.expect("settings created if missing");
		settings
			.entry("timeout")
			.or_insert(Item::Value(Value::Integer(Formatted::new(1_000))));
		settings
			.entry("node_spawn_timeout")
			.or_insert(Item::Value(Value::Integer(Formatted::new(300))));

		// Update relay chain config
		let relay_chain_config = self.relay_chain_mut()?;
		let relay_chain_binary_path = Self::resolve_path(&relay_chain.binary.path())?;
		*relay_chain_config
			.entry("default_command")
			.or_insert(value(&relay_chain_binary_path)) = value(&relay_chain_binary_path);
		if let Some(nodes) = Self::nodes_mut(relay_chain_config) {
			for node in nodes.iter_mut() {
				if let Some(command) = NetworkConfiguration::command_mut(node) {
					*command = value(&relay_chain_binary_path)
				}
			}
		}
		// Configure chain spec generator
		if let Some(path) = relay_chain.chain_spec_generator.as_ref().map(|b| b.path()) {
			let command = format!("{} {}", Self::resolve_path(&path)?, "{{chainName}}");
			*relay_chain_config.entry("chain_spec_command").or_insert(value(&command)) =
				value(&command);
		}

		// Update parachain config
		if let Some(tables) = self.parachains_mut() {
			for table in tables.iter_mut() {
				let id = table
					.get("id")
					.and_then(|i| i.as_integer())
					.ok_or_else(|| Error::Config("expected `parachain` to have `id`".into()))?
					as u32;
				let para =
					parachains.get(&id).expect("expected parachain existence due to preprocessing");

				// Resolve default_command to binary
				let path = Self::resolve_path(&para.binary.path())?;
				table.insert("default_command", value(&path));

				// Configure chain spec generator
				if let Some(path) = para.chain_spec_generator.as_ref().map(|b| b.path()) {
					let command = format!("{} {}", Self::resolve_path(&path)?, "{{chainName}}");
					*table.entry("chain_spec_command").or_insert(value(&command)) = value(&command);
				}

				// Resolve individual collator command to binary
				if let Some(collators) =
					table.get_mut("collators").and_then(|p| p.as_array_of_tables_mut())
				{
					for collator in collators.iter_mut() {
						if let Some(command) = NetworkConfiguration::command_mut(collator) {
							*command = value(&path)
						}
					}
				}
			}
		}

		// Write adapted zombienet config to temp file
		let network_config_file = Builder::new().suffix(".toml").tempfile()?;
		let path = network_config_file
			.path()
			.to_str()
			.ok_or_else(|| Error::Config("temp config file should have a path".into()))?;
		write(path, self.0.to_string())?;
		Ok(network_config_file)
	}

	/// Resolves the canonical path of a command specified within a network configuration file.
	///
	/// # Arguments
	/// * `path` - The path to be resolved.
	fn resolve_path(path: &Path) -> Result<String, Error> {
		Ok(path
			.canonicalize()
			.map_err(|_| {
				Error::Config(format!("the canonical path of {:?} could not be resolved", path))
			})
			.map(|p| p.to_str().map(|p| p.to_string()))?
			.ok_or_else(|| Error::Config("the path is invalid".into()))?)
	}
}

/// The configuration required to launch the relay chain.
struct RelayChain {
	/// The binary used to launch a relay chain node.
	binary: Binary,
	/// The additional workers required by the relay chain node.
	workers: [&'static str; 2],
	/// The name of the chain.
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

	/// Initializes the configuration required to launch a parachain using a binary sourced from the specified repository.
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

/// A binary used to launch a node.
#[derive(Debug, PartialEq)]
pub enum Binary {
	/// A local binary.
	Local {
		/// The name of the binary.
		name: String,
		/// The path of the binary.
		path: PathBuf,
		/// If applicable, the path to a manifest used to build the binary if missing.
		manifest: Option<PathBuf>,
	},
	/// A binary which needs to be sourced.
	Source {
		/// The name of the binary.
		name: String,
		/// The source of the binary.
		#[allow(private_interfaces)]
		source: Source,
		/// The cache to be used to store the binary.
		cache: PathBuf,
	},
}

impl Binary {
	/// Whether the binary exists.
	pub fn exists(&self) -> bool {
		self.path().exists()
	}

	/// If applicable, the latest version available.
	pub fn latest(&self) -> Option<&str> {
		match self {
			Self::Local { .. } => None,
			Self::Source { source, .. } => {
				if let GitHub(ReleaseArchive { latest, .. }) = source {
					latest.as_deref()
				} else {
					None
				}
			},
		}
	}

	/// Whether the binary is defined locally.
	pub fn local(&self) -> bool {
		matches!(self, Self::Local { .. })
	}

	/// The name of the binary.
	pub fn name(&self) -> &str {
		match self {
			Self::Local { name, .. } => name,
			Self::Source { name, .. } => name,
		}
	}

	/// The path of the binary.
	pub fn path(&self) -> PathBuf {
		match self {
			Self::Local { path, .. } => path.to_path_buf(),
			Self::Source { name, source, cache, .. } => {
				// Determine whether a specific version is specified
				let version = match source {
					Git { reference, .. } => reference.as_ref(),
					GitHub(source) => match source {
						ReleaseArchive { tag, .. } => tag.as_ref(),
						SourceCodeArchive { reference, .. } => reference.as_ref(),
					},
					Archive { .. } | Source::Url { .. } => None,
				};
				version.map_or_else(|| cache.join(name), |v| cache.join(format!("{name}-{v}")))
			},
		}
	}

	/// Attempts to resolve a version of a binary based on whether one is specified, an existing version
	/// can be found cached locally, or uses the latest version.
	///
	/// # Arguments
	/// * `name` - The name of the binary.
	/// * `specified` - If available, a version explicitly specified.
	/// * `available` - The available versions, used to check for those cached locally or the latest otherwise.
	/// * `cache` - The location used for caching binaries.
	fn resolve_version(
		name: &str,
		specified: Option<&str>,
		available: &[impl AsRef<str>],
		cache: &Path,
	) -> Option<String> {
		match specified {
			Some(version) => Some(version.to_string()),
			None => available
				.iter()
				.map(|v| v.as_ref())
				// Default to latest version available locally
				.filter_map(|version| {
					let path = cache.join(format!("{name}-{version}"));
					path.exists().then_some(Some(version.to_string()))
				})
				.nth(0)
				.unwrap_or(
					// Default to latest version
					available.get(0).and_then(|version| Some(version.as_ref().to_string())),
				),
		}
	}

	/// Sources the binary.
	///
	/// # Arguments
	/// * `release` - Whether any binaries needing to be built should be done so using the release profile.
	/// * `status` - Used to observe status updates.
	/// * `verbose` - Whether verbose output is required.
	pub async fn source(
		&self,
		release: bool,
		status: &impl Status,
		verbose: bool,
	) -> Result<(), Error> {
		match self {
			Self::Local { name, path, manifest, .. } => match manifest {
				None => {
					return Err(Error::MissingBinary(format!(
						"The {path:?} binary cannot be sourced automatically."
					)))
				},
				Some(manifest) => {
					sourcing::from_local_package(manifest, name, release, status, verbose).await
				},
			},
			Self::Source { source, cache, .. } => {
				source.source(cache, release, status, verbose).await
			},
		}
	}

	/// Whether any locally cached version can be replaced with a newer version.
	pub fn stale(&self) -> bool {
		// Only binaries sourced from GitHub release archives can currently be determined as stale
		let Self::Source { source: GitHub(ReleaseArchive { tag, latest, .. }), .. } = self else {
			return false;
		};
		latest.as_ref().map_or(false, |l| tag.as_ref() != Some(l))
	}

	/// Specifies that the latest available versions are to be used (where possible).
	pub fn use_latest(&mut self) {
		if let Self::Source { source: GitHub(ReleaseArchive { tag, latest, .. }), .. } = self {
			if let Some(latest) = latest {
				*tag = Some(latest.clone())
			}
		};
	}

	/// If applicable, the version of the binary.
	pub fn version(&self) -> Option<&str> {
		match self {
			Self::Local { .. } => None,
			Self::Source { source, .. } => match source {
				Git { reference, .. } => reference.as_ref(),
				GitHub(source) => match source {
					ReleaseArchive { tag, .. } => tag.as_ref(),
					SourceCodeArchive { reference, .. } => reference.as_ref(),
				},
				Archive { .. } | Source::Url { .. } => None,
			},
		}
		.map(|r| r.as_str())
	}
}

/// A descriptor of a remote repository.
#[derive(Debug, PartialEq)]
struct Repository {
	/// The url of the repository.
	url: Url,
	/// If applicable, the branch or tag to be used.
	reference: Option<String>,
	/// The name of a package within the repository. Defaults to the repository name.
	package: String,
}

impl Repository {
	/// Parses a url in the form of https://github.com/org/repository?package#tag into its component parts.
	///
	/// # Arguments
	/// * `url` - The url to be parsed.
	fn parse(url: &str) -> Result<Self, Error> {
		let url = Url::parse(url)?;
		let package = url.query();
		let reference = url.fragment().map(|f| f.to_string());

		let mut url = url.clone();
		url.set_query(None);
		url.set_fragment(None);

		let package = match package {
			Some(b) => b,
			None => GitHub::name(&url)?,
		}
		.to_string();

		Ok(Self { url, reference, package })
	}
}

/// Trait for observing status updates.
pub trait Status {
	/// Update the observer with the provided `status`.
	fn update(&self, status: &str);
}

impl Status for () {
	// no-op: status updates are ignored
	fn update(&self, _: &str) {}
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
			.and_then(|i| i.as_str())
			.map_or(false, |n| n == package)
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

/// Determines the target triple based on the current platform.
fn target() -> Result<&'static str, Error> {
	use std::env::consts::*;

	if OS == "windows" {
		return Err(Error::UnsupportedPlatform { arch: ARCH, os: OS });
	}

	match ARCH {
		"aarch64" => {
			return match OS {
				"macos" => Ok("aarch64-apple-darwin"),
				_ => Ok("aarch64-unknown-linux-gnu"),
			}
		},
		"x86_64" | "x86" => {
			return match OS {
				"macos" => Ok("x86_64-apple-darwin"),
				_ => Ok("x86_64-unknown-linux-gnu"),
			}
		},
		&_ => {},
	}
	Err(Error::UnsupportedPlatform { arch: ARCH, os: OS })
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::env::current_dir;
	use std::{fs::File, io::Write};
	use tempfile::tempdir;

	mod zombienet {
		use super::*;
		use sourcing::tests::Output;

		#[tokio::test]
		async fn new_with_relay_only_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "rococo-local"
"#
			)?;
			let version = "v1.12.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				Some(version),
				None,
				None,
				None,
				None,
			)
			.await?;

			let relay_chain = &zombienet.relay_chain.binary;
			assert_eq!(relay_chain.name(), "polkadot");
			assert_eq!(relay_chain.path(), temp_dir.path().join(format!("polkadot-{version}")));
			assert_eq!(relay_chain.version().unwrap(), version);
			assert!(matches!(
				relay_chain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(version.to_string())
			));
			assert!(zombienet.parachains.is_empty());
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
			let version = "v1.2.7";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				Some(version),
				None,
				None,
				None,
			)
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
chain = "rococo-local"
default_command = "./bin-v1.6.0/polkadot"
"#
			)?;
			let version = "v1.12.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				Some(version),
				None,
				None,
				None,
				None,
			)
			.await?;

			let relay_chain = &zombienet.relay_chain.binary;
			assert_eq!(relay_chain.name(), "polkadot");
			assert_eq!(relay_chain.path(), temp_dir.path().join(format!("polkadot-{version}")));
			assert_eq!(relay_chain.version().unwrap(), version);
			assert!(matches!(
				relay_chain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(version.to_string())
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
chain = "rococo-local"

[[relaychain.nodes]]
name = "alice"
validator = true
command = "polkadot"
"#
			)?;
			let version = "v1.12.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				Some(version),
				None,
				None,
				None,
				None,
			)
			.await?;

			let relay_chain = &zombienet.relay_chain.binary;
			assert_eq!(relay_chain.name(), "polkadot");
			assert_eq!(relay_chain.path(), temp_dir.path().join(format!("polkadot-{version}")));
			assert_eq!(relay_chain.version().unwrap(), version);
			assert!(matches!(
				relay_chain,
				Binary::Source { source: Source::GitHub(ReleaseArchive { tag, .. }), .. }
				if *tag == Some(version.to_string())
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
chain = "rococo-local"

[[relaychain.nodes]]
name = "alice"
validator = true
command = "polkadot"

[[relaychain.nodes]]
name = "bob"
validator = true
command = "polkadot-v1.12.0"
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path().to_str().unwrap(), None, None, None, None, None).await,
				Err(Error::UnsupportedCommand(error))
				if error == "the relay chain command is unsupported: polkadot-v1.12.0"
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
chain = "rococo-local"
default_command = "polkadot"

[[relaychain.nodes]]
name = "alice"
validator = true
command = "polkadot-v1.12.0"
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path().to_str().unwrap(), None, None, None, None, None).await,
				Err(Error::UnsupportedCommand(error))
				if error == "the relay chain command is unsupported: polkadot-v1.12.0"
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
chain = "rococo-local"

[[parachains]]
id = 1000
chain = "asset-hub-rococo-local"
"#
			)?;
			let system_parachain_version = "v1.12.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				Some("v1.11.0"),
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
			let version = "v1.12.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				Some(version),
				None,
			)
			.await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let system_parachain = &zombienet.parachains.get(&1000).unwrap();
			assert_eq!(system_parachain.chain.as_ref().unwrap(), "asset-hub-paseo-local");
			let chain_spec_generator = system_parachain.chain_spec_generator.as_ref().unwrap();
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
chain = "rococo-local"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
			)?;

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;

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
chain = "rococo-local"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
			)?;
			let version = "v1.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
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
chain = "rococo-local"

[[parachains]]
id = 2000
default_command = "./target/release/parachain-template-node"
"#
			)?;

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;

			assert_eq!(zombienet.parachains.len(), 1);
			let pop = &zombienet.parachains.get(&2000).unwrap().binary;
			assert_eq!(pop.name(), "parachain-template-node");
			assert_eq!(pop.path(), Path::new("./target/release/parachain-template-node"));
			assert_eq!(pop.version(), None);
			assert!(matches!(pop, Binary::Local { .. }));
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
chain = "rococo-local"

[[parachains]]
id = 2000

[[parachains.collators]]
name = "collator-01"
command = "./target/release/parachain-template-node"
"#
			)?;

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;

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
chain = "rococo-local"

[[parachains]]
id = 2000
default_command = "moonbeam"
"#
			)?;
			let version = "v0.38.0";

			let zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
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
		async fn new_ensures_parachain_id_exists() -> Result<()> {
			let temp_dir = tempdir()?;
			let cache = PathBuf::from(temp_dir.path());
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "rococo-local"

[[parachains]]
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path().to_str().unwrap(), None, None, None, None, None).await,
				Err(Error::Config(error))
				if error == "expected `parachain` to have `id`"
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
chain = "rococo-local"

[[parachains]]
id = 404
default_command = "missing-binary"
"#
			)?;

			assert!(matches!(
				Zombienet::new(&cache, config.path().to_str().unwrap(), None, None, None, None, None).await,
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
chain = "rococo-local"

[[parachains]]
id = 1000
chain = "asset-hub-rococo-local"

[[parachains]]
id = 2000
default_command = "./target/release/parachain-template-node"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
			)?;

			let mut zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;
			assert_eq!(zombienet.binaries().count(), 4);
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

			let mut zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;
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
chain = "rococo-local"
"#
			)?;

			let mut zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;
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
chain = "rococo-local"
"#
			)?;
			File::create(cache.join("polkadot"))?;

			let mut zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;
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
chain = "rococo-local"
"#
			)?;
			let version = "v1.12.0";
			File::create(cache.join(format!("polkadot-{version}")))?;
			File::create(cache.join(format!("polkadot-execute-worker-{version}")))?;
			File::create(cache.join(format!("polkadot-prepare-worker-{version}")))?;

			let mut zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;
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
chain = "rococo-local"

[[relaychain.nodes]]
name = "alice"
validator = true
"#
			)?;

			let mut zombienet = Zombienet::new(
				&cache,
				config.path().to_str().unwrap(),
				None,
				None,
				None,
				None,
				None,
			)
			.await?;
			for b in zombienet.binaries() {
				b.source(true, &Output, true).await?;
			}

			zombienet.spawn().await?;
			Ok(())
		}
	}

	mod network_config {
		use super::*;
		use std::io::Read;
		use std::{
			fs::{create_dir_all, File},
			io::Write,
			path::PathBuf,
		};
		use tempfile::{tempdir, Builder};

		#[test]
		fn initialising_from_file_fails_when_missing() {
			assert!(NetworkConfiguration::from(PathBuf::new()).is_err());
		}

		#[test]
		fn initialising_from_file_fails_when_malformed() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(config.as_file(), "[")?;
			assert!(matches!(NetworkConfiguration::from(config.path()), Err(Error::TomlError(..))));
			Ok(())
		}

		#[test]
		fn initialising_from_file_fails_when_relaychain_missing() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			assert!(matches!(NetworkConfiguration::from(config.path()), Err(Error::Config(..))));
			Ok(())
		}

		#[test]
		fn initializes_relay_from_file() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
				[relaychain]
				chain = "rococo-local"
				default_command = "polkadot"
				[[relaychain.nodes]]
				name = "alice"
			"#
			)?;
			let network_config = NetworkConfiguration::from(config.path())?;
			let relay_chain = network_config.relay_chain()?;
			assert_eq!("rococo-local", relay_chain["chain"].as_str().unwrap());
			assert_eq!(
				"polkadot",
				NetworkConfiguration::default_command(relay_chain).unwrap().as_str().unwrap()
			);
			let nodes = NetworkConfiguration::nodes(relay_chain).unwrap();
			assert_eq!("alice", nodes.get(0).unwrap()["name"].as_str().unwrap());
			assert!(network_config.parachains().is_none());
			Ok(())
		}

		#[test]
		fn initializes_parachains_from_file() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
				[relaychain]
				chain = "rococo-local"
				[[parachains]]
				id = 2000
				default_command = "node"
			"#
			)?;
			let network_config = NetworkConfiguration::from(config.path())?;
			let parachains = network_config.parachains().unwrap();
			let para_2000 = parachains.get(0).unwrap();
			assert_eq!(2000, para_2000["id"].as_integer().unwrap());
			assert_eq!(
				"node",
				NetworkConfiguration::default_command(para_2000).unwrap().as_str().unwrap()
			);
			Ok(())
		}

		#[test]
		fn configure_works() -> Result<(), Error> {
			let config = Builder::new().suffix(".toml").tempfile()?;
			writeln!(
				config.as_file(),
				r#"
[relaychain]
chain = "rococo-local"

[[relaychain.nodes]]
name = "alice"
command = "polkadot"

[[parachains]]
id = 1000
chain = "asset-hub-rococo-local"

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
name = "collator"
command = "./target/release/parachain-template-node"
"#
			)?;
			let mut network_config = NetworkConfiguration::from(config.path())?;

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

			let mut configured = network_config.configure(
				&RelayChain {
					binary: Binary::Local {
						name: "polkadot".to_string(),
						path: relay_chain.to_path_buf(),
						manifest: None,
					},
					workers: ["polkadot-execute-worker", ""],
					chain: "rococo-local".to_string(),
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
				]
				.into(),
			)?;
			assert_eq!("toml", configured.path().extension().unwrap());

			let mut contents = String::new();
			configured.read_to_string(&mut contents)?;
			println!("{contents}");
			assert_eq!(
				contents,
				format!(
					r#"
[relaychain]
chain = "rococo-local"
default_command = "{0}"

[[relaychain.nodes]]
name = "alice"
command = "{0}"

[[parachains]]
id = 1000
chain = "asset-hub-rococo-local"
default_command = "{1}"

[[parachains.collators]]
name = "asset-hub"
command = "{1}"

[[parachains]]
id = 2000
default_command = "{2}"

[[parachains.collators]]
name = "pop"
command = "{2}"

[[parachains]]
id = 2001
default_command = "{3}"

[[parachains.collators]]
name = "collator"
command = "{3}"

[settings]
timeout = 1000
node_spawn_timeout = 300

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
		fn configure_with_chain_spec_generator_works() -> Result<(), Error> {
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
			let mut network_config = NetworkConfiguration::from(config.path())?;

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

			let mut configured = network_config.configure(
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
			assert_eq!("toml", configured.path().extension().unwrap());

			let mut contents = String::new();
			configured.read_to_string(&mut contents)?;
			println!("{contents}");
			assert_eq!(
				contents,
				format!(
					r#"
[relaychain]
chain = "paseo-local"
default_command = "{0}"
chain_spec_command = "{1} {2}"

[[relaychain.nodes]]
name = "alice"
command = "{0}"

[[parachains]]
id = 1000
chain = "asset-hub-paseo-local"
default_command = "{3}"
chain_spec_command = "{4} {2}"

[[parachains.collators]]
name = "asset-hub"
command = "{3}"

[settings]
timeout = 1000
node_spawn_timeout = 300


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
		use crate::up::sourcing::GitHub::SourceCodeArchive;
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

	mod binary {
		use super::*;
		use duct::cmd;
		use sourcing::tests::Output;
		use std::fs::create_dir_all;

		#[test]
		fn local_binary_works() -> Result<()> {
			let name = "polkadot";
			let temp_dir = tempdir()?;
			let path = temp_dir.path().join(name);
			File::create(&path)?;

			let binary =
				Binary::Local { name: name.to_string(), path: path.clone(), manifest: None };

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(binary.local());
			assert_eq!(binary.name(), name);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), None);
			Ok(())
		}

		#[test]
		fn local_package_works() -> Result<()> {
			let name = "polkadot";
			let temp_dir = tempdir()?;
			let path = temp_dir.path().join("target/release").join(name);
			create_dir_all(&path.parent().unwrap())?;
			File::create(&path)?;
			let manifest = Some(temp_dir.path().join("Cargo.toml"));

			let binary = Binary::Local { name: name.to_string(), path: path.clone(), manifest };

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(binary.local());
			assert_eq!(binary.name(), name);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), None);
			Ok(())
		}

		#[test]
		fn resolve_version_works() -> Result<()> {
			let name = "polkadot";
			let temp_dir = tempdir()?;

			let available = vec!["v1.13.0", "v1.12.0", "v1.11.0"];

			// Specified
			let specified = Some("v1.12.0");
			assert_eq!(
				Binary::resolve_version(name, specified, &available, temp_dir.path()).unwrap(),
				specified.unwrap()
			);
			// Latest
			assert_eq!(
				Binary::resolve_version(name, None, &available, temp_dir.path()).unwrap(),
				available[0]
			);
			// Cached
			File::create(temp_dir.path().join(format!("{name}-{}", available[1])))?;
			assert_eq!(
				Binary::resolve_version(name, None, &available, temp_dir.path()).unwrap(),
				available[1]
			);
			Ok(())
		}

		#[test]
		fn sourced_from_archive_works() -> Result<()> {
			let name = "polkadot";
			let url = "https://github.com/r0gue-io/polkadot/releases/latest/download/polkadot-aarch64-apple-darwin.tar.gz".to_string();
			let contents = vec![
				name.to_string(),
				"polkadot-execute-worker".into(),
				"polkadot-prepare-worker".into(),
			];
			let temp_dir = tempdir()?;
			let path = temp_dir.path().join(name);
			File::create(&path)?;

			let mut binary = Binary::Source {
				name: name.to_string(),
				source: Archive { url: url.to_string(), contents },
				cache: temp_dir.path().to_path_buf(),
			};

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(!binary.local());
			assert_eq!(binary.name(), name);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), None);
			binary.use_latest();
			assert_eq!(binary.version(), None);
			Ok(())
		}

		#[test]
		fn sourced_from_git_works() -> Result<()> {
			let package = "hello_world";
			let url = Url::parse("https://github.com/hpaluch/rust-hello-world")?;
			let temp_dir = tempdir()?;
			for reference in [None, Some("436b7dbffdfaaf7ad90bf44ae8fdcb17eeee65a3".to_string())] {
				let path = temp_dir.path().join(
					reference
						.as_ref()
						.map_or(package.into(), |reference| format!("{package}-{reference}")),
				);
				File::create(&path)?;

				let mut binary = Binary::Source {
					name: package.to_string(),
					source: Git {
						url: url.clone(),
						reference: reference.clone(),
						manifest: None,
						package: package.to_string(),
						artifacts: vec![package.to_string()],
					},
					cache: temp_dir.path().to_path_buf(),
				};

				assert!(binary.exists());
				assert_eq!(binary.latest(), None);
				assert!(!binary.local());
				assert_eq!(binary.name(), package);
				assert_eq!(binary.path(), path);
				assert!(!binary.stale());
				assert_eq!(binary.version(), reference.as_ref().map(|r| r.as_str()));
				binary.use_latest();
				assert_eq!(binary.version(), reference.as_ref().map(|r| r.as_str()));
			}

			Ok(())
		}

		#[test]
		fn sourced_from_github_release_archive_works() -> Result<()> {
			let owner = "r0gue-io";
			let repository = "polkadot";
			let tag_format = "polkadot-{tag}";
			let name = "polkadot";
			let archive = format!("{name}-{}.tar.gz", target()?);
			let contents = ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"];
			let temp_dir = tempdir()?;
			for tag in [None, Some("v1.12.0".to_string())] {
				let path = temp_dir
					.path()
					.join(tag.as_ref().map_or(name.to_string(), |t| format!("{name}-{t}")));
				File::create(&path)?;
				for latest in [None, Some("v2.0.0".to_string())] {
					let mut binary = Binary::Source {
						name: name.to_string(),
						source: GitHub(ReleaseArchive {
							owner: owner.into(),
							repository: repository.into(),
							tag: tag.clone(),
							tag_format: Some(tag_format.to_string()),
							archive: archive.clone(),
							contents: contents.into_iter().map(|b| (b, None)).collect(),
							latest: latest.clone(),
						}),
						cache: temp_dir.path().to_path_buf(),
					};

					assert!(binary.exists());
					assert_eq!(binary.latest(), latest.as_ref().map(|l| l.as_str()));
					assert!(!binary.local());
					assert_eq!(binary.name(), name);
					assert_eq!(binary.path(), path);
					assert_eq!(binary.stale(), latest.is_some());
					assert_eq!(binary.version(), tag.as_ref().map(|t| t.as_str()));
					binary.use_latest();
					if latest.is_some() {
						assert_eq!(binary.version(), latest.as_ref().map(|l| l.as_str()));
					}
				}
			}
			Ok(())
		}

		#[test]
		fn sourced_from_github_source_code_archive_works() -> Result<()> {
			let owner = "paritytech";
			let repository = "polkadot-sdk";
			let package = "polkadot";
			let manifest = "substrate/Cargo.toml";
			let temp_dir = tempdir()?;
			for reference in [None, Some("72dba98250a6267c61772cd55f8caf193141050f".to_string())] {
				let path = temp_dir.path().join(
					reference.as_ref().map_or(package.to_string(), |t| format!("{package}-{t}")),
				);
				File::create(&path)?;
				let mut binary = Binary::Source {
					name: package.to_string(),
					source: GitHub(SourceCodeArchive {
						owner: owner.to_string(),
						repository: repository.to_string(),
						reference: reference.clone(),
						manifest: Some(PathBuf::from(manifest)),
						package: package.to_string(),
						artifacts: vec![package.to_string()],
					}),
					cache: temp_dir.path().to_path_buf(),
				};

				assert!(binary.exists());
				assert_eq!(binary.latest(), None);
				assert!(!binary.local());
				assert_eq!(binary.name(), package);
				assert_eq!(binary.path(), path);
				assert_eq!(binary.stale(), false);
				assert_eq!(binary.version(), reference.as_ref().map(|r| r.as_str()));
				binary.use_latest();
				assert_eq!(binary.version(), reference.as_ref().map(|l| l.as_str()));
			}
			Ok(())
		}

		#[test]
		fn sourced_from_url_works() -> Result<()> {
			let name = "polkadot";
			let url =
				"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc";
			let temp_dir = tempdir()?;
			let path = temp_dir.path().join(name);
			File::create(&path)?;

			let mut binary = Binary::Source {
				name: name.to_string(),
				source: Source::Url { url: url.to_string(), name: name.to_string() },
				cache: temp_dir.path().to_path_buf(),
			};

			assert!(binary.exists());
			assert_eq!(binary.latest(), None);
			assert!(!binary.local());
			assert_eq!(binary.name(), name);
			assert_eq!(binary.path(), path);
			assert!(!binary.stale());
			assert_eq!(binary.version(), None);
			binary.use_latest();
			assert_eq!(binary.version(), None);
			Ok(())
		}

		#[tokio::test]
		async fn sourcing_from_local_binary_not_supported() -> Result<()> {
			let name = "polkadot".to_string();
			let temp_dir = tempdir()?;
			let path = temp_dir.path().join(&name);
			assert!(matches!(
				Binary::Local { name, path: path.clone(), manifest: None }.source(true, &Output, true).await,
				Err(Error::MissingBinary(error)) if error == format!("The {path:?} binary cannot be sourced automatically.")
			));
			Ok(())
		}

		#[tokio::test]
		async fn sourcing_from_local_package_works() -> Result<()> {
			let temp_dir = tempdir()?;
			let name = "hello_world";
			cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
			let path = temp_dir.path().join(name);
			let manifest = Some(path.join("Cargo.toml"));
			let path = path.join("target/release").join(name);
			Binary::Local { name: name.to_string(), path: path.clone(), manifest }
				.source(true, &Output, true)
				.await?;
			assert!(path.exists());
			Ok(())
		}

		#[tokio::test]
		async fn sourcing_from_url_works() -> Result<()> {
			let name = "polkadot";
			let url =
				"https://github.com/paritytech/polkadot-sdk/releases/latest/download/polkadot.asc";
			let temp_dir = tempdir()?;
			let path = temp_dir.path().join(name);

			Binary::Source {
				name: name.to_string(),
				source: Source::Url { url: url.to_string(), name: name.to_string() },
				cache: temp_dir.path().to_path_buf(),
			}
			.source(true, &Output, true)
			.await?;
			assert!(path.exists());
			Ok(())
		}
	}

	mod repository {
		use super::{Error, Repository};
		use url::Url;

		#[test]
		fn parsing_full_url_works() {
			assert_eq!(
				Repository::parse("https://github.com/org/repository?package#tag").unwrap(),
				Repository {
					url: Url::parse("https://github.com/org/repository").unwrap(),
					reference: Some("tag".into()),
					package: "package".into(),
				}
			);
		}

		#[test]
		fn parsing_simple_url_works() {
			let url = "https://github.com/org/repository";
			assert_eq!(
				Repository::parse(url).unwrap(),
				Repository {
					url: Url::parse(url).unwrap(),
					reference: None,
					package: "repository".into(),
				}
			);
		}

		#[test]
		fn parsing_invalid_url_returns_error() {
			assert!(matches!(
				Repository::parse("github.com/org/repository"),
				Err(Error::ParseError(..))
			));
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

	#[test]
	fn target_works() -> Result<()> {
		use std::{process::Command, str};
		let output = Command::new("rustc").arg("-vV").output()?;
		let output = str::from_utf8(&output.stdout)?;
		let target = output
			.lines()
			.find(|l| l.starts_with("host: "))
			.map(|l| &l[6..])
			.unwrap()
			.to_string();
		assert_eq!(super::target()?, target);
		Ok(())
	}
}
