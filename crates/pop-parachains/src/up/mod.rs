// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use glob::glob;
use indexmap::IndexMap;
pub use pop_common::{
	git::{GitHub, Repository},
	sourcing::{Binary, GitHub::*, Source, Source::*},
};
use std::{
	fmt::Debug,
	fs::write,
	iter::once,
	path::{Path, PathBuf},
};
use symlink::{remove_symlink_file, symlink_file};
use tempfile::{Builder, NamedTempFile};
use toml_edit::{value, ArrayOfTables, DocumentMut, Formatted, Item, Table, Value};
use zombienet_sdk::{Network, NetworkConfig, NetworkConfigExt};
use zombienet_support::fs::local::LocalFileSystem;

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
		Ok(Self { network_config, relay_chain, parachains, hrmp_channels: false })
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
		let config = self.network_config.configure(&self.relay_chain, &self.parachains)?;
		let path = config.path().to_str().expect("temp config file should have a path").into();
		let network_config = NetworkConfig::load_from_toml(path)?;
		self.hrmp_channels = !network_config.hrmp_channels().is_empty();
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

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::env::current_dir;
	use std::{fs::File, io::Write};
	use tempfile::tempdir;

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
