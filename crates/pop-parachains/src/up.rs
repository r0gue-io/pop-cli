// SPDX-License-Identifier: GPL-3.0
use crate::{
	errors::Error,
	utils::git::{Git, GitHub},
};
use duct::cmd;
use indexmap::IndexMap;
use std::{
	fmt::Debug,
	fs::{copy, create_dir_all, metadata, rename, write, File},
	io::{BufRead, Seek, SeekFrom, Write},
	iter::once,
	os::unix::fs::PermissionsExt,
	path::{Path, PathBuf},
};
use symlink::{remove_symlink_file, symlink_file};
use tempfile::{tempdir, tempfile, Builder, NamedTempFile};
use toml_edit::{value, ArrayOfTables, DocumentMut, Formatted, Item, Table, Value};
use url::Url;
use zombienet_sdk::{Network, NetworkConfig, NetworkConfigExt};
use zombienet_support::fs::local::LocalFileSystem;

const POLKADOT: &str = "https://github.com/r0gue-io/polkadot";
const POLKADOT_DEFAULT_VERSION: &str = "v1.12.0";
const POP: &str = "https://github.com/r0gue-io/pop-node";
const POP_DEFAULT_VERSION: &str = "v0.1.0-alpha2";

/// Configuration to launch a local network.
pub struct Zombienet {
	/// The cache location, used for caching binaries.
	cache: PathBuf,
	/// The config to be used to launch a network.
	network_config: NetworkConfiguration,
	/// The configuration required to launch the relay chain.
	relay_chain: RelayChain,
	/// The configuration required to launch parachains.
	parachains: IndexMap<u32, Parachain>,
}

impl Zombienet {
	/// Initialises the configuration for launching a local network.
	/// # Arguments
	///
	/// * `cache` - location, used for caching binaries
	/// * `network_config` - config file to be used to launch a network.
	/// * `relay_chain_version` - the specific version used for the relay chain (none will fetch the last one).
	/// * `system_parachain_version` - the specific version used for the system chain (none will fetch the last one).
	/// * `parachains` - list of parachains url.
	pub async fn new(
		cache: PathBuf,
		network_config: &str,
		relay_chain_version: Option<&String>,
		system_parachain_version: Option<&String>,
		parachains: Option<&Vec<String>>,
	) -> Result<Self, Error> {
		// Parse network config
		let network_config = NetworkConfiguration::from(network_config)?;
		// Determine relay and parachain requirements based on arguments and config
		let relay_chain = Self::relay_chain(relay_chain_version, &network_config, &cache).await?;
		let parachains = Self::parachains(
			system_parachain_version.unwrap_or(&relay_chain.binary.version),
			parachains,
			&network_config,
			&cache,
		)
		.await?;
		Ok(Self { cache, network_config, relay_chain, parachains })
	}

	/// Determines whether any binaries are missing.
	pub fn missing_binaries(&self) -> Vec<&Binary> {
		let mut missing = Vec::new();
		if !self.relay_chain.binary.path.exists() {
			missing.push(&self.relay_chain.binary);
		}
		for parachain in self.parachains.values().filter(|p| !p.binary.path.exists()) {
			missing.push(&parachain.binary);
		}
		missing
	}

	/// Launches the local network.
	pub async fn spawn(&mut self) -> Result<Network<LocalFileSystem>, Error> {
		// Symlink polkadot workers
		for worker in &self.relay_chain.workers {
			let dest = self.cache.join(&worker.name);
			if dest.exists() {
				remove_symlink_file(&dest)?;
			}
			symlink_file(&worker.path, dest)?;
		}

		// Load from config and spawn network
		let config = self.configure()?;
		let path = config.path().to_str().expect("temp config file should have a path").into();
		let network_config = NetworkConfig::load_from_toml(path)?;
		Ok(network_config.spawn_native().await?)
	}

	// Determine relay chain requirements based on specified version and config
	async fn relay_chain(
		version: Option<&String>,
		network_config: &NetworkConfiguration,
		cache: &PathBuf,
	) -> Result<RelayChain, Error> {
		// Validate config
		let relay_chain = network_config.relay_chain()?;
		if let Some(command) =
			NetworkConfiguration::default_command(relay_chain).and_then(|c| c.as_str())
		{
			if !command.to_lowercase().ends_with(RelayChain::BINARY) {
				return Err(Error::UnsupportedCommand(format!(
					"the relay chain command is unsupported: {command}",
				)));
			}
		}
		if let Some(nodes) = NetworkConfiguration::nodes(relay_chain) {
			for node in nodes {
				if let Some(command) = NetworkConfiguration::command(node).and_then(|c| c.as_str())
				{
					if !command.to_lowercase().ends_with(RelayChain::BINARY) {
						return Err(Error::UnsupportedCommand(format!(
							"the relay chain command is unsupported: {command}",
						)));
					}
				}
			}
		}

		// Default to latest version when none specified
		let version = match version {
			Some(v) => v.to_string(),
			None => Self::latest_polkadot_release().await?,
		};
		Ok(RelayChain::new(version, cache)?)
	}

	// Determine parachain requirements based on specified version and config
	async fn parachains(
		system_parachain_version: &str,
		parachains: Option<&Vec<String>>,
		network_config: &NetworkConfiguration,
		cache: &PathBuf,
	) -> Result<IndexMap<u32, Parachain>, Error> {
		let Some(tables) = network_config.parachains() else {
			return Ok(IndexMap::default());
		};

		let mut paras = IndexMap::new();
		'outer: for table in tables.iter() {
			let id = table
				.get("id")
				.and_then(|i| i.as_integer())
				.ok_or(Error::Config("expected `parachain` to have `id`".into()))? as u32;

			let default_command = NetworkConfiguration::default_command(table)
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
				.expect("missing default_command set above");
			let Some(command) = default_command.as_str() else {
				continue;
			};
			let command = command.to_lowercase();

			// Check if system parachain
			if command == Parachain::SYSTEM_CHAIN_BINARY {
				paras
					.insert(id, Parachain::system_parachain(id, system_parachain_version, &cache)?);
				continue;
			}

			// Check if pop-node
			if command == Parachain::POP_BINARY {
				paras.insert(id, Parachain::pop(id, &Self::latest_pop_release().await?, &cache)?);
				continue;
			}

			// Check if parachain binary source specified as an argument
			if let Some(parachains) = parachains {
				for parachain in parachains {
					let repository = Repository::parse(parachain)?;
					if command == repository.package {
						paras.insert(
							id,
							Parachain::from_git(
								id,
								repository.url,
								repository.reference,
								repository.package,
								&cache,
							)?,
						);
						continue 'outer;
					}
				}
			}

			// Check if command references a local binary using a relative path
			if command.starts_with("./") || command.starts_with("../") {
				paras.insert(id, Parachain::from_local(id, &PathBuf::default(), command.into())?);
				continue;
			}

			return Err(Error::MissingBinary(command));
		}
		Ok(paras)
	}

	async fn latest_polkadot_release() -> Result<String, Error> {
		let repo = GitHub::parse(POLKADOT)?;
		match repo.get_latest_releases().await {
			Ok(releases) => {
				// Fetching latest releases
				for release in releases {
					if !release.prerelease && release.tag_name.starts_with("polkadot-v") {
						return Ok(release
							.tag_name
							.strip_prefix("polkadot-")
							.map_or_else(|| release.tag_name.clone(), |v| v.to_string()));
					}
				}
				// It should never reach this point, but in case we download a default version of polkadot
				Ok(POLKADOT_DEFAULT_VERSION.to_string())
			},
			// If an error with GitHub API return the POLKADOT DEFAULT VERSION
			Err(_) => Ok(POLKADOT_DEFAULT_VERSION.to_string()),
		}
	}

	async fn latest_pop_release() -> Result<String, Error> {
		let repo = GitHub::parse(POP)?;
		match repo.get_latest_releases().await {
			Ok(releases) => {
				// Fetching latest releases
				for release in releases {
					return Ok(release
						.tag_name
						.strip_prefix("polkadot-")
						.map_or_else(|| release.tag_name.clone(), |v| v.to_string()));
				}
				// It should never reach this point, but in case we download a default version of pop
				Ok(POP_DEFAULT_VERSION.to_string())
			},
			// If an error with GitHub API return the default version
			Err(_) => Ok(POP_DEFAULT_VERSION.to_string()),
		}
	}

	fn configure(&mut self) -> Result<NamedTempFile, Error> {
		self.network_config.configure(&self.relay_chain.binary, &self.parachains)
	}
}

/// The network configuration.
struct NetworkConfiguration(DocumentMut);

impl NetworkConfiguration {
	fn from(path: impl AsRef<Path>) -> Result<Self, Error> {
		let contents = std::fs::read_to_string(&path)?;
		let config = contents.parse::<DocumentMut>().map_err(|err| Error::TomlError(err.into()))?;
		let network_config = NetworkConfiguration(config);
		network_config.relay_chain()?;
		Ok(network_config)
	}

	fn relay_chain(&self) -> Result<&Table, Error> {
		self.0
			.get("relaychain")
			.and_then(|i| i.as_table())
			.ok_or(Error::Config("expected `relaychain`".into()))
	}

	fn relay_chain_mut(&mut self) -> Result<&mut Table, Error> {
		self.0
			.get_mut("relaychain")
			.and_then(|i| i.as_table_mut())
			.ok_or(Error::Config("expected `relaychain`".into()))
	}

	fn parachains(&self) -> Option<&ArrayOfTables> {
		self.0.get("parachains").and_then(|p| p.as_array_of_tables())
	}

	fn parachains_mut(&mut self) -> Option<&mut ArrayOfTables> {
		self.0.get_mut("parachains").and_then(|p| p.as_array_of_tables_mut())
	}

	fn command(config: &Table) -> Option<&Item> {
		config.get("command")
	}

	fn command_mut(config: &mut Table) -> Option<&mut Item> {
		config.get_mut("command")
	}

	fn default_command(config: &Table) -> Option<&Item> {
		config.get("default_command")
	}

	fn nodes(relay_chain: &Table) -> Option<&ArrayOfTables> {
		relay_chain.get("nodes").and_then(|i| i.as_array_of_tables())
	}

	fn nodes_mut(relay_chain: &mut Table) -> Option<&mut ArrayOfTables> {
		relay_chain.get_mut("nodes").and_then(|i| i.as_array_of_tables_mut())
	}

	// Adapts user provided config file to one that with resolved binary paths and which is compatible with current zombienet-sdk requirements
	fn configure(
		&mut self,
		relay_chain: &Binary,
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
		let relay_path = Self::resolve_path(&relay_chain.path)?;
		*relay_chain_config.entry("default_command").or_insert(value(&relay_path)) =
			value(&relay_path);
		if let Some(nodes) = Self::nodes_mut(relay_chain_config) {
			for node in nodes.iter_mut() {
				if let Some(command) = NetworkConfiguration::command_mut(node) {
					*command = value(&relay_path)
				}
			}
		}

		// Update parachain config
		if let Some(tables) = self.parachains_mut() {
			for table in tables.iter_mut() {
				let id = table
					.get("id")
					.and_then(|i| i.as_integer())
					.ok_or(Error::Config("expected `parachain` to have `id`".into()))? as u32;
				let para =
					parachains.get(&id).expect("expected parachain existence due to preprocessing");

				// Resolve default_command to binary
				let path = Self::resolve_path(&para.binary.path)?;
				table.insert("default_command", value(&path));

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
			.ok_or(Error::Config("temp config file should have a path".into()))?;
		write(path, self.0.to_string())?;
		Ok(network_config_file)
	}

	fn resolve_path(path: &Path) -> Result<String, Error> {
		Ok(path
			.canonicalize()
			.map_err(|_| {
				Error::Config(format!("the canonical path of {:?} could not be resolved", path))
			})
			.map(|p| p.to_str().map(|p| p.to_string()))?
			.ok_or(Error::Config("the path is invalid".into()))?)
	}
}

/// The configuration required to launch the relay chain.
#[derive(Debug, PartialEq)]
struct RelayChain {
	/// The binary used to launch a relay chain node.
	binary: Binary,
	/// The additional workers required by the relay chain node.
	workers: [Binary; 2],
}

impl RelayChain {
	const BINARY: &'static str = "polkadot";
	const WORKERS: [&'static str; 2] = ["polkadot-execute-worker", "polkadot-prepare-worker"];
	fn new(version: impl Into<String>, cache: &Path) -> Result<Self, Error> {
		let name = Self::BINARY.to_string();
		let version = version.into();
		let path = cache.join(format!("{name}-{version}"));

		let tag = format!("polkadot-{version}");
		let archive = format!("polkadot-{}.tar.gz", target()?);
		let source =
			Source::Archive {
				url: format!("{POLKADOT}/releases/download/{tag}/{archive}"),
				contents: once((name.clone(), path.clone()))
					.chain(Self::WORKERS.iter().map(|worker| {
						(worker.to_string(), cache.join(&format!("{worker}-{version}")))
					}))
					.collect(),
			};

		// Add polkadot workers
		let workers = Self::WORKERS.map(|worker| {
			Binary::new(
				worker,
				&version,
				cache.join(&format!("{worker}-{version}")),
				Source::Artifact,
			)
		});

		Ok(RelayChain { binary: Binary { name, version, path, source }, workers })
	}
}

/// The configuration required to launch a parachain.
#[derive(Debug, PartialEq)]
struct Parachain {
	/// The parachain identifier on the local network.
	id: u32,
	/// The binary used to launch a relay chain node.
	binary: Binary,
}

impl Parachain {
	const SYSTEM_CHAIN_BINARY: &'static str = "polkadot-parachain";
	const POP_BINARY: &'static str = "pop-node";

	fn from_git(
		id: u32,
		repo: Url,
		reference: Option<String>,
		package: String,
		cache: &Path,
	) -> Result<Parachain, Error> {
		let path = cache.join(&package);
		let source = Source::Git {
			url: repo.clone(),
			reference: reference.clone(),
			package: package.clone(),
			artifacts: vec![(package.clone(), path.clone())],
		};
		Ok(Parachain { id, binary: Binary::new(package, String::default(), path, source) })
	}

	fn from_local(id: u32, working_dir: &Path, relative_path: PathBuf) -> Result<Parachain, Error> {
		let name = relative_path
			.file_name()
			.and_then(|f| f.to_str())
			.ok_or(Error::Config(format!("unable to determine file name for {relative_path:?}")))?
			.to_string();
		Ok(Parachain {
			id,
			binary: Binary::new(
				name,
				String::default(),
				working_dir.join(&relative_path),
				Source::Local(relative_path),
			),
		})
	}

	fn pop(id: u32, version: &str, cache: &Path) -> Result<Self, Error> {
		let name = Self::POP_BINARY;
		let path = cache.join(format!("{name}-{version}"));
		let archive = format!("{name}-{}.tar.gz", target()?);
		let source = Source::Archive {
			url: format!("{POP}/releases/download/{version}/{archive}"),
			contents: vec![(name.to_string(), path.clone())],
		};
		Ok(Parachain { id, binary: Binary::new(name, version, path, source) })
	}

	fn system_parachain(id: u32, version: &str, cache: &Path) -> Result<Self, Error> {
		let name = Self::SYSTEM_CHAIN_BINARY;
		let path = cache.join(format!("{name}-{version}"));
		let tag = format!("polkadot-{version}");
		let archive = format!("polkadot-parachain-{}.tar.gz", target()?);
		let source = Source::Archive {
			url: format!("{POLKADOT}/releases/download/{tag}/{archive}"),
			contents: vec![(name.to_string(), path.clone())],
		};
		Ok(Parachain { id, binary: Binary::new(name, version, path, source) })
	}
}

/// A binary used to launch a node.
#[derive(Debug, Default, PartialEq)]
pub struct Binary {
	/// The name of a binary.
	pub name: String,
	/// The version of the binary.
	version: String,
	/// The path to the binary, typically a versioned name within the cache.
	path: PathBuf,
	/// The source of the binary.
	pub source: Source,
}

impl Binary {
	pub fn new(
		name: impl Into<String>,
		version: impl Into<String>,
		path: impl Into<PathBuf>,
		source: Source,
	) -> Self {
		Self { name: name.into(), version: version.into(), path: path.into(), source }
	}

	/// Sources the binary by either downloading from a url or by cloning a git repository and
	/// building locally from the resulting source code.
	///
	/// # Arguments
	///
	/// * `working_dir` - the working directory to be used
	/// * `status` - used to observe status updates
	/// * `verbose` - whether verbose output is required
	pub async fn source(
		&self,
		working_dir: &Path,
		status: impl Status,
		verbose: bool,
	) -> Result<(), Error> {
		// Ensure working directory exists
		create_dir_all(working_dir)?;
		// Download or clone and build from source
		match &self.source {
			Source::Archive { url, contents } => {
				// Download archive
				status.update(&format!("Downloading from {url}..."));
				let response = reqwest::get(url.as_str()).await?.error_for_status()?;
				let mut file = tempfile()?;
				file.write_all(&response.bytes().await?)?;
				file.seek(SeekFrom::Start(0))?;
				// Extract contents
				status.update("Extracting from archive...");
				let tar = flate2::read::GzDecoder::new(file);
				let mut archive = tar::Archive::new(tar);
				let temp_dir = tempdir()?;
				let working_dir = temp_dir.path();
				archive.unpack(working_dir)?;
				for (name, dest) in contents {
					rename(working_dir.join(name), dest)?;
				}
				status.update("Sourcing complete.");
			},
			Source::Git { url, reference, package, artifacts } => {
				// Clone repository into working directory
				let repository_name = GitHub::name(url)?;
				let working_dir = working_dir.join(repository_name);
				status.update(&format!("Cloning {url}..."));
				Git::clone(url, &working_dir, reference.as_deref())?;
				// Build binaries
				self.build(&working_dir, package, &artifacts, status, verbose).await?;
			},
			Source::Url(url) => {
				// Download required version of binaries
				status.update(&format!("Downloading from {url}..."));
				Self::download(&url, &self.path).await?;
			},
			Source::None | Source::Artifact | Source::Local(..) => {},
		}
		Ok(())
	}

	async fn build(
		&self,
		working_dir: &Path,
		package: &str,
		artifacts: &[(String, PathBuf)],
		status: impl Status,
		verbose: bool,
	) -> Result<(), Error> {
		// Build binaries and then copy to cache and target
		let command = cmd("cargo", vec!["build", "--release", "-p", package]).dir(working_dir);
		match verbose {
			false => {
				let reader = command.stderr_to_stdout().reader()?;
				let mut output = std::io::BufReader::new(reader).lines();
				while let Some(line) = output.next() {
					status.update(&line?);
				}
			},
			true => {
				status.update("");
				command.run()?;
			},
		}
		// Copy artifacts required
		for (name, dest) in artifacts {
			copy(working_dir.join(format!("target/release/{name}")), dest)?;
		}
		Ok(())
	}

	async fn download(url: &str, dest: &PathBuf) -> Result<(), Error> {
		// Download to destination path
		let response = reqwest::get(url).await?.error_for_status()?;
		let mut file = File::create(&dest)?;
		file.write_all(&response.bytes().await?)?;
		// Make executable
		let mut perms = metadata(dest)?.permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(dest, perms)?;
		Ok(())
	}

	pub fn version(&self) -> &str {
		&self.version
	}
}

/// The source of a binary.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Source {
	/// No source could be determined.
	#[default]
	None,
	/// An archive for download.
	Archive {
		/// The url of the archive.
		url: String,
		/// The contents within the archive which are required.
		contents: Vec<(String, PathBuf)>,
	},
	/// A build artifact.
	Artifact,
	/// A git repository.
	Git {
		/// The url of the repository.
		url: Url,
		/// If applicable, the branch, tag or commit.
		reference: Option<String>,
		/// The name of the package to be built.
		package: String,
		/// Any additional artifacts which are required.
		artifacts: Vec<(String, PathBuf)>,
	},
	/// A local source.
	Local(PathBuf),
	/// A URL for download.
	Url(String),
}

/// A descriptor of a remote repository.
#[derive(Debug, PartialEq)]
struct Repository {
	/// The (base) url of the repository.
	url: Url,
	/// If applicable, the branch or tag to be used.
	reference: Option<String>,
	/// The name of a package within the repository. Defaults to the repository name.
	package: String,
}

impl Repository {
	/// Parses a url in the form of https://github.com/org/repository?package#tag into its component parts.
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
pub trait Status: Copy {
	/// Update the observer with the provided `status`.
	fn update(&self, status: &str);
}

impl Status for () {
	// no-op: status updates are ignored
	fn update(&self, _: &str) {}
}

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

	const CONFIG_FILE_PATH: &str = "../../tests/zombienet.toml";
	const TESTING_POLKADOT_VERSION: &str = "v1.7.0";
	const POLKADOT_BINARY: &str = "polkadot-v1.7.0";
	const POLKADOT_PARACHAIN_BINARY: &str = "polkadot-parachain-v1.7.0";

	#[tokio::test]
	async fn test_new_zombienet_success() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let zombienet = Zombienet::new(
			cache.clone(),
			CONFIG_FILE_PATH,
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
		)
		.await?;

		// Check has the binary for Polkadot
		let relay_chain = zombienet.relay_chain;
		assert_eq!(relay_chain.binary.name, RelayChain::BINARY);
		assert_eq!(relay_chain.binary.path, temp_dir.path().join(POLKADOT_BINARY));
		assert_eq!(relay_chain.binary.version, TESTING_POLKADOT_VERSION);
		assert!(matches!(relay_chain.binary.source, Source::Archive { .. }));

		// Check has the binary for the System Chain
		assert_eq!(zombienet.parachains.len(), 2);

		let system_chain = &zombienet.parachains[0];
		assert_eq!(system_chain.binary.name, Parachain::SYSTEM_CHAIN_BINARY);
		assert_eq!(system_chain.binary.path, temp_dir.path().join(POLKADOT_PARACHAIN_BINARY));
		assert_eq!(system_chain.binary.version, TESTING_POLKADOT_VERSION);
		assert!(matches!(system_chain.binary.source, Source::Archive { .. }));

		// Check has the binary for Pop
		let parachain = &zombienet.parachains[1];
		assert_eq!(parachain.binary.name, "pop-node");
		assert_eq!(parachain.binary.path, temp_dir.path().join("pop-node"));
		assert_eq!(parachain.binary.version, "");
		assert!(matches!(parachain.binary.source, Source::Git { .. }));

		Ok(())
	}

	#[tokio::test]
	async fn test_new_fails_wrong_config_no_para_id() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let toml_file = generate_wrong_config_no_para_id(&temp_dir)
			.expect("Error generating the testing toml file");
		let toml_file_path =
			toml_file.to_str().expect("Error generating the path of the testing toml file");

		let result_error = Zombienet::new(
			cache.clone(),
			toml_file_path,
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
		)
		.await;

		assert!(result_error.is_err());
		let error_message = result_error.err().unwrap();
		assert_eq!(
			error_message.to_string(),
			"Configuration error: expected `parachain` to have `id`"
		);

		Ok(())
	}

	#[tokio::test]
	async fn test_relay_chain() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let config = NetworkConfiguration::from(CONFIG_FILE_PATH)?;

		let relay_chain =
			Zombienet::relay_chain(Some(&TESTING_POLKADOT_VERSION.to_string()), &config, &cache)
				.await?
				.binary;

		assert_eq!(relay_chain.name, RelayChain::BINARY);
		assert_eq!(relay_chain.path, temp_dir.path().join(POLKADOT_BINARY));
		assert_eq!(relay_chain.version, TESTING_POLKADOT_VERSION);
		assert!(matches!(relay_chain.source, Source::Archive { .. }));

		Ok(())
	}

	#[tokio::test]
	async fn test_relay_chain_no_specifying_version() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let config = NetworkConfiguration::from(CONFIG_FILE_PATH)?;

		// Ideally here we will Mock GitHub struct and its get_latest_release function response
		let relay_chain = Zombienet::relay_chain(None, &config, &cache).await?.binary;

		assert_eq!(relay_chain.name, RelayChain::BINARY);
		assert!(relay_chain.version.starts_with("v"));
		assert!(matches!(relay_chain.source, Source::Archive { .. }));

		Ok(())
	}

	#[tokio::test]
	async fn test_relay_chain_fails_wrong_config() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = generate_wrong_config_no_relay(&temp_dir)?;
		assert!(matches!(
			NetworkConfiguration::from(path),
			Err(Error::Config(message)) if message == "expected `relaychain`"));
		Ok(())
	}

	#[tokio::test]
	async fn test_latest_polkadot_release() -> Result<()> {
		let version = Zombienet::latest_polkadot_release().await?;
		// Result will change all the time to the current version (e.g: v1.9.0), check at least starts with v
		assert!(version.starts_with("v"));
		Ok(())
	}

	#[tokio::test]
	async fn test_system_parachain() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let system_chain =
			Parachain::system_parachain(1000, &TESTING_POLKADOT_VERSION.to_string(), &cache)?
				.binary;

		assert_eq!(system_chain.name, Parachain::SYSTEM_CHAIN_BINARY);
		assert_eq!(system_chain.path, temp_dir.path().join(POLKADOT_PARACHAIN_BINARY));
		assert_eq!(system_chain.version, TESTING_POLKADOT_VERSION);
		assert!(matches!(system_chain.source, Source::Url { .. }));

		Ok(())
	}

	#[tokio::test]
	async fn test_parachain() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let repo = Repository::parse("https://github.com/r0gue-io/pop-node")?;
		let parachain =
			Parachain::from_git(2000, repo.url, repo.reference, repo.package, &cache)?.binary;

		assert_eq!(parachain.name, "pop-node");
		assert_eq!(parachain.path, temp_dir.path().join("pop-node"));
		assert_eq!(parachain.version, "");
		assert!(matches!(parachain.source, Source::Git { .. }));

		Ok(())
	}

	#[tokio::test]
	async fn test_missing_binaries() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let zombienet = Zombienet::new(
			cache.clone(),
			CONFIG_FILE_PATH,
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
		)
		.await?;

		let missing_binaries = zombienet.missing_binaries();
		assert_eq!(missing_binaries.len(), 3);

		Ok(())
	}

	#[tokio::test]
	async fn test_missing_binaries_no_missing() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		// Create "fake" binary files
		let relay_chain_file_path = temp_dir.path().join(POLKADOT_BINARY);
		File::create(relay_chain_file_path)?;
		let system_chain_file_path = temp_dir.path().join(POLKADOT_PARACHAIN_BINARY);
		File::create(system_chain_file_path)?;
		let pop_file_path = temp_dir.path().join("pop-node");
		File::create(pop_file_path)?;

		let zombienet = Zombienet::new(
			cache.clone(),
			CONFIG_FILE_PATH,
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
		)
		.await?;

		let missing_binaries = zombienet.missing_binaries();
		assert_eq!(missing_binaries.len(), 0);

		Ok(())
	}

	#[tokio::test]
	async fn test_configure_zombienet() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let mut zombienet = Zombienet::new(
			cache.clone(),
			CONFIG_FILE_PATH,
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
		)
		.await?;

		File::create(cache.join(format!("{}-{TESTING_POLKADOT_VERSION}", RelayChain::BINARY)))?;
		File::create(
			cache.join(format!("{}-{TESTING_POLKADOT_VERSION}", Parachain::SYSTEM_CHAIN_BINARY)),
		)?;
		File::create(cache.join("pop-node"))?;

		zombienet.configure()?;
		Ok(())
	}

	#[tokio::test]
	async fn test_spawn_error_no_binaries() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = PathBuf::from(temp_dir.path());

		let mut zombienet = Zombienet::new(
			cache.clone(),
			CONFIG_FILE_PATH,
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&TESTING_POLKADOT_VERSION.to_string()),
			Some(&vec!["https://github.com/r0gue-io/pop-node".to_string()]),
		)
		.await?;

		let spawn = zombienet.spawn().await;
		assert!(spawn.is_err());

		Ok(())
	}

	#[tokio::test]
	async fn test_source_url() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let cache = PathBuf::from(temp_dir.path());

		let binary = Binary::new("polkadot", TESTING_POLKADOT_VERSION,
			cache.join(POLKADOT_BINARY), Source::Url(
				"https://github.com/paritytech/polkadot-sdk/releases/download/polkadot-v1.7.0/polkadot"
					.to_string(),
			));
		let working_dir = tempfile::tempdir()?;
		binary.source(&working_dir.path(), (), false).await?;
		assert!(temp_dir.path().join(POLKADOT_BINARY).exists());

		Ok(())
	}

	fn generate_wrong_config_no_para_id(temp_dir: &tempfile::TempDir) -> Result<PathBuf> {
		let file_path = temp_dir.path().join("wrong_config_no_para_id.toml");
		let mut file = File::create(file_path.clone())?;
		writeln!(
			file,
			r#"
				[relaychain]
				chain = "rococo-local"

				[[relaychain.nodes]]
				name = "alice"
				validator = true

				[[parachains]]
				default_command = "pop-node"

				[[parachains.collators]]
				name = "pop"
			"#
		)?;
		Ok(file_path)
	}
	fn generate_wrong_config_no_relay(temp_dir: &tempfile::TempDir) -> Result<PathBuf> {
		let file_path = temp_dir.path().join("wrong_config_no_para_id.toml");
		let mut file = File::create(file_path.clone())?;
		writeln!(
			file,
			r#"
				[[parachains]]
				id = 1000
				chain = "asset-hub-rococo-local"
				
				[[parachains.collators]]
				name = "asset-hub"
				
				[[parachains]]
				id = 4385
				default_command = "pop-node"
				
				[[parachains.collators]]
				name = "pop"
			"#
		)?;
		Ok(file_path)
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

	mod network_config {
		use super::{Binary, Error, NetworkConfiguration, Parachain};
		use std::fs::create_dir_all;
		use std::{
			fs::File,
			io::{Read, Write},
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
		fn initialises_relay_from_file() -> Result<(), Error> {
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
		fn initialises_parachains_from_file() -> Result<(), Error> {
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
				&Binary { path: relay_chain.to_path_buf(), ..Default::default() },
				&[
					(
						1000,
						Parachain {
							id: 1000,
							binary: Binary {
								path: system_chain.to_path_buf(),
								..Default::default()
							},
						},
					),
					(
						2000,
						Parachain {
							id: 2000,
							binary: Binary { path: pop.to_path_buf(), ..Default::default() },
						},
					),
					(
						2001,
						Parachain {
							id: 2001,
							binary: Binary {
								path: parachain_template.to_path_buf(),
								..Default::default()
							},
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

	mod relay_chain {
		use super::{
			Binary, Error, GitHub, RelayChain, Source, POLKADOT, POLKADOT_DEFAULT_VERSION,
		};
		use std::iter::once;
		use tempfile::tempdir;
		use url::Url;

		#[test]
		fn initialises_for_build() -> Result<(), Error> {
			let version = POLKADOT_DEFAULT_VERSION;
			let cache = tempdir()?;
			let binary = RelayChain::BINARY;
			let source = Source::Git {
				url: Url::parse(POLKADOT)?,
				reference: Some(format!("release-polkadot-{version}")),
				package: binary.into(),
				artifacts: once((
					binary.to_string(),
					cache.path().join(format!("{binary}-{version}")),
				))
				.chain(RelayChain::WORKERS.iter().map(|worker| {
					(worker.to_string(), cache.path().join(format!("{worker}-{version}")))
				}))
				.collect(),
			};
			let workers = RelayChain::WORKERS.map(|worker| {
				Binary::new(
					worker,
					POLKADOT_DEFAULT_VERSION,
					cache.path().join(format!("{worker}-{version}")),
					Source::Artifact,
				)
			});

			assert_eq!(
				RelayChain::new(version, cache.path())?,
				RelayChain {
					binary: Binary::new(
						binary.to_string(),
						POLKADOT_DEFAULT_VERSION,
						cache.path().join(format!("{binary}-{version}")),
						source
					),
					workers
				}
			);
			Ok(())
		}

		#[test]
		fn initialises_for_download() -> Result<(), Error> {
			let version = POLKADOT_DEFAULT_VERSION;
			let cache = tempdir()?;
			let binary = RelayChain::BINARY;
			let repo = Url::parse(POLKADOT)?;
			let source =
				Source::Url(GitHub::release(&repo, &format!("polkadot-{version}"), binary));
			let workers = RelayChain::WORKERS.map(|worker| {
				Binary::new(
					worker,
					POLKADOT_DEFAULT_VERSION,
					cache.path().join(format!("{worker}-{version}")),
					Source::Url(GitHub::release(&repo, &format!("polkadot-{version}"), worker)),
				)
			});

			assert_eq!(
				RelayChain::new(version, cache.path())?,
				RelayChain {
					binary: Binary::new(
						binary,
						POLKADOT_DEFAULT_VERSION,
						cache.path().join(format!("{binary}-{version}")),
						source
					),
					workers
				}
			);
			Ok(())
		}
	}

	mod parachain {
		use super::{
			Binary, Error, GitHub, Parachain, Repository, Source, POLKADOT,
			POLKADOT_DEFAULT_VERSION,
		};
		use std::path::PathBuf;
		use tempfile::tempdir;
		use url::Url;

		#[test]
		fn initialises_from_git() -> Result<(), Error> {
			let repo = Repository::parse("https://github.com/r0gue-io/pop-node")?;
			let cache = tempdir()?;
			assert_eq!(
				Parachain::from_git(
					2000,
					repo.url.clone(),
					repo.reference.clone(),
					repo.package.clone(),
					cache.path()
				)?,
				Parachain {
					id: 2000,
					binary: Binary {
						name: "pop-node".into(),
						version: String::default(),
						path: cache.path().join("pop-node"),
						source: Source::Git {
							url: repo.url,
							reference: repo.reference,
							package: repo.package,
							artifacts: vec![(
								"pop-node".to_string(),
								cache.path().join("pop-node")
							)],
						},
					}
				}
			);
			Ok(())
		}

		#[test]
		fn initialises_from_local() -> Result<(), Error> {
			let working_dir = tempdir()?;
			let command = PathBuf::from("./target/release/node");
			assert_eq!(
				Parachain::from_local(2000, &working_dir.path(), command.clone())?,
				Parachain {
					id: 2000,
					binary: Binary {
						name: "node".into(),
						version: String::default(),
						path: working_dir.path().join(&command),
						source: Source::Local(command),
					}
				}
			);
			Ok(())
		}

		#[test]
		fn initialises_system_parachain_for_build() -> Result<(), Error> {
			let version = POLKADOT_DEFAULT_VERSION;
			let cache = tempdir()?;
			let binary = Parachain::SYSTEM_CHAIN_BINARY;
			let repo = Url::parse(POLKADOT)?;
			assert_eq!(
				Parachain::system_parachain(1000, version, cache.path())?,
				Parachain {
					id: 1000,
					binary: Binary {
						name: binary.into(),
						version: version.into(),
						path: cache.path().join(format!("{binary}-{version}")),
						source: Source::Git {
							url: repo,
							reference: Some(format!("release-polkadot-{version}")),
							package: "polkadot-parachain-bin".into(),
							artifacts: vec![(
								binary.into(),
								cache.path().join(format!("{binary}-{version}"))
							)],
						},
					}
				}
			);
			Ok(())
		}

		#[test]
		fn initialises_system_parachain_for_download() -> Result<(), Error> {
			let version = POLKADOT_DEFAULT_VERSION;
			let cache = tempdir()?;
			let binary = Parachain::SYSTEM_CHAIN_BINARY;
			let repo = Url::parse(POLKADOT)?;
			assert_eq!(
				Parachain::system_parachain(1000, version, cache.path())?,
				Parachain {
					id: 1000,
					binary: Binary {
						name: binary.into(),
						version: version.into(),
						path: cache.path().join(format!("{binary}-{version}")),
						source: Source::Url(GitHub::release(
							&repo,
							&format!("polkadot-{version}"),
							binary,
						),),
					}
				}
			);
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
}
