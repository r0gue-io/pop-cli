// SPDX-License-Identifier: GPL-3.0

use crate::{
	git::{Git, GitHub},
	Result,
};
use anyhow::anyhow;
use duct::cmd;
use indexmap::IndexMap;
use log::{debug, info};
use std::{
	env::current_dir,
	fs::{copy, metadata, remove_dir_all, write, File},
	io::Write,
	os::unix::fs::PermissionsExt,
	path::{Path, PathBuf},
};
use symlink::{remove_symlink_file, symlink_file};
use tempfile::{Builder, NamedTempFile};
use toml_edit::{value, Document, Formatted, Item, Table, Value};
use url::Url;
use zombienet_sdk::{Network, NetworkConfig, NetworkConfigExt};
use zombienet_support::fs::local::LocalFileSystem;

const POLKADOT_SDK: &str = "https://github.com/paritytech/polkadot-sdk";

pub struct Zombienet {
	/// The cache location, used for caching binaries.
	cache: PathBuf,
	/// The config to be used to launch a network.
	network_config: (PathBuf, Document),
	/// The binary required to launch the relay chain.
	relay_chain: Binary,
	/// The binaries required to launch parachains.
	parachains: IndexMap<u32, Binary>,
}

impl Zombienet {
	pub async fn new(
		cache: PathBuf,
		network_config: &str,
		relay_chain_version: Option<&String>,
		system_parachain_version: Option<&String>,
		parachains: Option<&Vec<String>>,
	) -> Result<Self> {
		// Parse network config
		let network_config_path = PathBuf::from(network_config);
		let config = std::fs::read_to_string(&network_config_path)?.parse::<Document>()?;
		// Determine binaries
		let relay_chain_binary = Self::relay_chain(relay_chain_version, &config, &cache).await?;
		let mut parachain_binaries = IndexMap::new();
		if let Some(tables) = config.get("parachains").and_then(|p| p.as_array_of_tables()) {
			for table in tables.iter() {
				let id = table
					.get("id")
					.and_then(|i| i.as_integer())
					.ok_or(anyhow!("expected `parachain` to have `id`"))? as u32;
				let default_command = table
					.get("default_command")
					.cloned()
					.or_else(|| {
						// Check if any collators define command
						if let Some(collators) =
							table.get("collators").and_then(|p| p.as_array_of_tables())
						{
							for collator in collators.iter() {
								if let Some(command) =
									collator.get("command").and_then(|i| i.as_str())
								{
									return Some(Item::Value(Value::String(Formatted::new(
										command.into(),
									))));
								}
							}
						}

						// Otherwise default to polkadot-parachain
						Some(Item::Value(Value::String(Formatted::new(
							"polkadot-parachain".into(),
						))))
					})
					.expect("missing default_command set above");
				let Some(Value::String(command)) = default_command.as_value() else {
					continue;
				};

				let command = command.value().to_lowercase();
				if command == "polkadot-parachain" {
					parachain_binaries.insert(
						id,
						Self::system_parachain(
							system_parachain_version.unwrap_or(&relay_chain_binary.version),
							&cache,
						)?,
					);
				} else if let Some(parachains) = parachains {
					for parachain in parachains {
						let url = Url::parse(parachain)?;
						let name = GitHub::name(&url)?;
						if command == name {
							parachain_binaries.insert(id, Self::parachain(url, &cache)?);
						}
					}
				}
			}
		}

		Ok(Self {
			cache,
			network_config: (network_config_path, config),
			relay_chain: relay_chain_binary,
			parachains: parachain_binaries,
		})
	}

	pub fn missing_binaries(&self) -> Vec<&Binary> {
		let mut missing = Vec::new();
		if !self.relay_chain.path.exists() {
			missing.push(&self.relay_chain);
		}
		for binary in self.parachains.values().filter(|b| !b.path.exists()) {
			missing.push(binary);
		}
		missing
	}

	pub async fn spawn(&mut self) -> Result<Network<LocalFileSystem>> {
		// Symlink polkadot-related binaries
		for file in ["polkadot-execute-worker", "polkadot-prepare-worker"] {
			let dest = self.cache.join(file);
			if dest.exists() {
				remove_symlink_file(&dest)?;
			}
			symlink_file(self.cache.join(format!("{file}-{}", self.relay_chain.version)), dest)?;
		}

		// Load from config and spawn network
		let config = self.configure()?;
		let path = config.path().to_str().expect("temp config file should have a path").into();
		info!("spawning network...");
		let network_config = NetworkConfig::load_from_toml(path)?;
		Ok(network_config.spawn_native().await?)
	}

	// Adapts provided config file to one that is compatible with current zombienet-sdk requirements
	fn configure(&mut self) -> Result<NamedTempFile> {
		let (network_config_path, network_config) = &mut self.network_config;

		// Add zombienet-sdk specific settings if missing
		let Item::Table(settings) =
			network_config.entry("settings").or_insert(Item::Table(Table::new()))
		else {
			return Err(anyhow!("expected `settings` to be table"));
		};
		settings
			.entry("timeout")
			.or_insert(Item::Value(Value::Integer(Formatted::new(1_000))));
		settings
			.entry("node_spawn_timeout")
			.or_insert(Item::Value(Value::Integer(Formatted::new(300))));

		// Update relay chain config
		let relay_path = self
			.relay_chain
			.path
			.to_str()
			.ok_or(anyhow!("the relay chain path is invalid"))?;
		let Item::Table(relay_chain) =
			network_config.entry("relaychain").or_insert(Item::Table(Table::new()))
		else {
			return Err(anyhow!("expected `relaychain` to be table"));
		};
		*relay_chain.entry("default_command").or_insert(value(relay_path)) = value(relay_path);

		// Update parachain config
		if let Some(tables) =
			network_config.get_mut("parachains").and_then(|p| p.as_array_of_tables_mut())
		{
			for table in tables.iter_mut() {
				let id = table
					.get("id")
					.and_then(|i| i.as_integer())
					.ok_or(anyhow!("expected `parachain` to have `id`"))? as u32;

				// Resolve default_command to binary
				{
					// Check if provided via args, therefore cached
					if let Some(para) = self.parachains.get(&id) {
						let para_path =
							para.path.to_str().ok_or(anyhow!("the parachain path is invalid"))?;
						table.insert("default_command", value(para_path));
					} else if let Some(default_command) = table.get_mut("default_command") {
						// Otherwise assume local binary, fix path accordingly
						let command_path = default_command
							.as_str()
							.ok_or(anyhow!("expected `default_command` value to be a string"))?;
						let path = Self::resolve_path(network_config_path, command_path)?;
						*default_command = value(path.to_str().ok_or(anyhow!(
							"the parachain binary was not found: {0}",
							command_path
						))?);
					}
				}

				// Resolve individual collator command to binary
				if let Some(collators) =
					table.get_mut("collators").and_then(|p| p.as_array_of_tables_mut())
				{
					for collator in collators.iter_mut() {
						if let Some(command) = collator.get_mut("command") {
							// Check if provided via args, therefore cached
							if let Some(para) = self.parachains.get(&id) {
								let para_path = para
									.path
									.to_str()
									.ok_or(anyhow!("the parachain path is invalid"))?;
								*command = value(para_path);
							} else {
								let command_path = command
									.as_str()
									.ok_or(anyhow!("expected `command` value to be a string"))?;
								let path = Self::resolve_path(network_config_path, command_path)?;
								*command = value(path.to_str().ok_or(anyhow!(
									"the parachain binary was not found: {0}",
									command_path
								))?);
							}
						}
					}
				}
			}
		}

		// Write adapted zombienet config to temp file
		let network_config_file = Builder::new()
			.suffix(".toml")
			.tempfile()
			.expect("network config could not be created with .toml extension");
		let path = network_config_file
			.path()
			.to_str()
			.expect("temp config file should have a path");
		write(path, network_config.to_string())?;
		Ok(network_config_file)
	}

	fn resolve_path(network_config_path: &mut PathBuf, command_path: &str) -> Result<PathBuf> {
		network_config_path
			.join(command_path)
			.canonicalize()
			.or_else(|_| current_dir().unwrap().join(command_path).canonicalize())
			.map_err(|_| {
				anyhow!(
					"unable to find canonical local path to specified command: `{}` are you missing an argument?",
					command_path
				)
			})
	}

	async fn relay_chain(
		version: Option<&String>,
		network_config: &Document,
		cache: &PathBuf,
	) -> Result<Binary> {
		const BINARY: &str = "polkadot";
		let relay_command = network_config
			.get("relaychain")
			.ok_or(anyhow!("expected `relaychain`"))?
			.get("default_command");
		if let Some(Value::String(command)) = relay_command.and_then(|c| c.as_value()) {
			if !command.value().to_lowercase().contains(BINARY) {
				return Err(anyhow!(
					"the relay chain command is unsupported: {0}",
					command.to_string()
				));
			}
		}
		let version = match version {
			Some(v) => v.to_string(),
			None => Self::latest_polkadot_release().await?,
		};
		let versioned_name = format!("{BINARY}-{version}");
		let path = cache.join(&versioned_name);
		let mut sources = Vec::new();
		if !path.exists() {
			const BINARIES: [&str; 3] =
				[BINARY, "polkadot-execute-worker", "polkadot-prepare-worker"];
			let repo = Url::parse(POLKADOT_SDK).expect("repository url valid");
			if cfg!(target_os = "macos") {
				sources.push(Source::Git {
					url: repo.into(),
					branch: Some(format!("release-polkadot-{version}")),
					package: BINARY.into(),
					binaries: BINARIES.iter().map(|b| b.to_string()).collect(),
					version: Some(version.clone()),
				});
			} else {
				for b in BINARIES {
					sources.push(Source::Url {
						name: b.to_string(),
						version: version.clone(),
						url: GitHub::release(&repo, &format!("polkadot-{version}"), b),
					})
				}
			};
		}

		Ok(Binary { name: versioned_name, version, path, sources })
	}

	fn system_parachain(version: &String, cache: &PathBuf) -> Result<Binary> {
		const BINARY: &str = "polkadot-parachain";
		let versioned_name = format!("{BINARY}-{version}");
		let path = cache.join(&versioned_name);
		let mut sources = Vec::new();
		if !path.exists() {
			let repo = Url::parse(POLKADOT_SDK).expect("repository url valid");
			if cfg!(target_os = "macos") {
				sources.push(Source::Git {
					url: repo.into(),
					branch: Some(format!("release-polkadot-{version}")),
					package: "polkadot-parachain-bin".into(),
					binaries: vec![BINARY.into()],
					version: Some(version.into()),
				})
			} else {
				sources.push(Source::Url {
					name: BINARY.into(),
					version: version.into(),
					url: GitHub::release(&repo, &format!("polkadot-{version}"), BINARY),
				})
			};
		}
		Ok(Binary { name: versioned_name, version: version.into(), path, sources })
	}

	fn parachain(repo: Url, cache: &PathBuf) -> Result<Binary> {
		let binary = repo.query();
		let branch = repo.fragment().map(|f| f.to_string());
		let mut url = repo.clone();
		url.set_query(None);
		url.set_fragment(None);
		let binary = match binary {
			Some(b) => b,
			None => GitHub::name(&url)?,
		}
		.to_string();

		let path = cache.join(&binary);
		let mut sources = Vec::new();
		if !path.exists() {
			sources.push(Source::Git {
				url: repo.clone(),
				branch: branch.clone(),
				package: binary.clone(),
				binaries: vec![binary.clone()],
				version: branch,
			})
		}
		Ok(Binary { name: binary, version: "".into(), path, sources })
	}

	async fn latest_polkadot_release() -> Result<String> {
		debug!("relay chain version not specified - determining latest polkadot release...");
		let repo = Url::parse(POLKADOT_SDK).expect("valid polkadot-sdk repository url");
		let release_tag = GitHub::get_latest_release(&repo).await?;
		Ok(release_tag
			.strip_prefix("polkadot-")
			.map_or_else(|| release_tag.clone(), |v| v.to_string()))
	}
}

pub struct Binary {
	pub name: String,
	version: String,
	path: PathBuf,
	sources: Vec<Source>,
}

impl Binary {
	pub async fn source(&self, cache: &PathBuf) -> Result<()> {
		for source in &self.sources {
			source.process(cache).await?;
		}
		Ok(())
	}
}

/// The source of a binary.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Source {
	/// The source is a URL.
	Url {
		/// The name of the binary.
		name: String,
		/// The version of the binary.
		version: String,
		/// The url to download the binary.
		url: String,
	},
	/// The source is a git repository.
	Git {
		/// The url of the repository.
		url: Url,
		branch: Option<String>,
		package: String,
		binaries: Vec<String>,
		version: Option<String>,
	},
}

impl Source {
	async fn build_binaries<'b>(
		path: &Path,
		package: &str,
		names: impl Iterator<Item = (&'b String, PathBuf)>,
	) -> Result<()> {
		info!("building {package}");
		// Build binaries and then copy to cache and target
		cmd(
			"cargo",
			vec![
				"build",
				"--release",
				"-p",
				package,
				//     "--quiet"
			],
		)
		.dir(path)
		.run()?;
		for (name, dest) in names {
			copy(path.join(format!("target/release/{name}")), dest)?;
		}
		Ok(())
	}

	async fn download(url: &str, cache: &PathBuf) -> Result<()> {
		// Download to cache
		let response = reqwest::get(url).await?;
		let mut file = File::create(&cache)?;
		file.write_all(&response.bytes().await?)?;
		// Make executable
		let mut perms = metadata(cache)?.permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(cache, perms)?;
		Ok(())
	}

	pub async fn process(&self, cache: &Path) -> Result<Option<Vec<PathBuf>>> {
		// Download or clone and build from source
		match self {
			Source::Url { name, version, url } => {
				// Check if source already exist within cache
				let versioned_name = Self::versioned_name(name, Some(version));
				if cache.join(&versioned_name).exists() {
					return Ok(None);
				}

				// Download required version of binaries
				info!("downloading {name} {version} from {url}...");
				Self::download(&url, &cache.join(&versioned_name)).await?;
				Ok(None)
			},
			Source::Git { url, branch, package, binaries, version } => {
				// Check if all binaries already exist within cache
				let versioned_names: Vec<_> = binaries
					.iter()
					.map(|n| (n, Self::versioned_name(n, version.as_deref())))
					.collect();
				if versioned_names.iter().all(|(_, n)| cache.join(&n).exists()) {
					return Ok(None);
				}

				let repository_name = GitHub::name(url)?;
				info!("cloning {repository_name} repository...");
				// Clone repository into working directory
				let working_dir = cache.join(".src").join(repository_name);
				let working_dir = Path::new(&working_dir);
				if let Err(e) = Git::clone(url, working_dir, branch.as_deref()) {
					if working_dir.exists() {
						Self::remove(working_dir)?;
					}
					return Err(e);
				}
				// Build binaries and finally remove working directory
				Self::build_binaries(
					working_dir,
					package,
					versioned_names
						.iter()
						.map(|(binary, versioned)| (*binary, cache.join(versioned))),
				)
				.await?;
				Self::remove(working_dir)?;
				Ok(None)
			},
		}
	}

	fn remove(path: &Path) -> Result<()> {
		remove_dir_all(path)?;
		if let Some(source) = path.parent() {
			if source.exists() && source.read_dir().map(|mut i| i.next().is_none()).unwrap_or(false)
			{
				remove_dir_all(source)?;
			}
		}
		Ok(())
	}

	/// A versioned name of a binary.
	pub fn versioned_name(name: &str, version: Option<&str>) -> String {
		match version {
			Some(version) => format!("{name}-{version}"),
			None => name.to_string(),
		}
	}
}
