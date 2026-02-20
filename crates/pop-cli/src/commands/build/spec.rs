// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self, Cli,
		traits::{Cli as _, *},
	},
	common::{
		builds::{ChainPath, create_chain_spec_builder, guide_user_to_select_profile},
		omni_node::source_polkadot_omni_node_binary,
		runtime::build_deterministic_runtime,
	},
	output::{BuildCommandError, CliResponse, OutputMode, PromptRequiredError},
	style::style,
};
use clap::{Args, ValueEnum};
use pop_chains::{
	ChainSpec, ChainSpecBuilder, generate_genesis_state_file_with_node, is_supported,
};
use pop_common::{Profile, manifest::from_path};
use serde::Serialize;
use std::{
	fs::create_dir_all,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, VariantArray};
use strum_macros::{AsRefStr, Display, EnumString};

pub(crate) type CodePathBuf = PathBuf;
pub(crate) type StatePathBuf = PathBuf;

const DEFAULT_CHAIN: &str = "dev";
const DEFAULT_PACKAGE: &str = "parachain-template-runtime";
const DEFAULT_PARA_ID: u32 = 2000;
const DEFAULT_PROTOCOL_ID: &str = "my-protocol";
const DEFAULT_RUNTIME_DIR: &str = "./runtime";
const DEFAULT_SPEC_NAME: &str = "chain-spec.json";

/// Structured output for `build spec --json`.
#[derive(Serialize)]
pub(crate) struct BuildSpecOutput {
	chain_spec_path: String,
	genesis_state_path: Option<String>,
	genesis_code_path: Option<String>,
	relay_chain: Option<String>,
	para_id: Option<u32>,
}

#[derive(
	AsRefStr,
	Clone,
	Default,
	Debug,
	Display,
	EnumString,
	EnumMessage,
	ValueEnum,
	VariantArray,
	Eq,
	PartialEq,
	Serialize,
)]
/// Supported chain types for the resulting chain spec.
pub(crate) enum ChainType {
	// A development chain that runs mainly on one node.
	#[default]
	#[strum(
		serialize = "Development",
		message = "Development",
		detailed_message = "Meant for a development chain that runs mainly on one node."
	)]
	Development,
	// A local chain that runs locally on multiple nodes for testing purposes.
	#[strum(
		serialize = "Local",
		message = "Local",
		detailed_message = "Meant for a local chain that runs locally on multiple nodes for testing purposes."
	)]
	Local,
	// A live chain.
	#[strum(serialize = "Live", message = "Live", detailed_message = "Meant for a live chain.")]
	Live,
}

#[derive(
	AsRefStr,
	Clone,
	Copy,
	Default,
	Debug,
	Display,
	EnumString,
	EnumMessage,
	ValueEnum,
	VariantArray,
	Eq,
	PartialEq,
	Serialize,
)]
/// Supported relay chains that can be included in the resulting chain spec.
pub(crate) enum RelayChain {
	#[strum(
		serialize = "paseo",
		message = "Paseo",
		detailed_message = "Polkadot's community testnet."
	)]
	Paseo,
	#[default]
	#[strum(
		serialize = "paseo-local",
		message = "Paseo Local",
		detailed_message = "Local configuration for Paseo network."
	)]
	PaseoLocal,
	#[strum(
		serialize = "westend",
		message = "Westend",
		detailed_message = "Parity's test network for protocol testing."
	)]
	Westend,
	#[strum(
		serialize = "westend-local",
		message = "Westend Local",
		detailed_message = "Local configuration for Westend network."
	)]
	WestendLocal,
	#[strum(
		serialize = "kusama",
		message = "Kusama",
		detailed_message = "Polkadot's canary network."
	)]
	Kusama,
	#[strum(
		serialize = "kusama-local",
		message = "Kusama Local",
		detailed_message = "Local configuration for Kusama network."
	)]
	KusamaLocal,
	#[strum(
		serialize = "polkadot",
		message = "Polkadot",
		detailed_message = "Polkadot live network."
	)]
	Polkadot,
	#[strum(
		serialize = "polkadot-local",
		message = "Polkadot Local",
		detailed_message = "Local configuration for Polkadot network."
	)]
	PolkadotLocal,
}

/// Command for generating a chain specification.
#[derive(Args, Default, Serialize)]
pub struct BuildSpecCommand {
	/// Directory path for your project [default: current directory]
	#[serde(skip_serializing)]
	#[arg(short, long, default_value = "./")]
	pub(crate) path: PathBuf,
	/// File name for the resulting spec. If a path is given,
	/// the necessary directories will be created
	#[serde(skip_serializing)]
	#[arg(short, long = "output")]
	pub(crate) output_file: Option<PathBuf>,
	/// Build profile for the binary to generate the chain specification.
	#[arg(long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long = "para-id")]
	pub(crate) para_id: Option<u32>,
	/// Whether to keep localhost as a bootnode.
	#[arg(short = 'b', long)]
	pub(crate) default_bootnode: Option<bool>,
	/// Type of the chain.
	#[arg(short = 't', long = "type", value_enum)]
	pub(crate) chain_type: Option<ChainType>,
	/// Comma-separated list of features to build the node or runtime with.
	#[arg(long, default_value = "")]
	pub(crate) features: String,
	/// Whether to skip the build step or not. If artifacts are not found, the build will be
	/// performed regardless.
	#[arg(long = "skip-build")]
	pub(crate) skip_build: bool,
	/// Provide the chain specification to use (e.g. dev, local, custom or a path to an existing
	/// file).
	#[arg(short, long)]
	pub(crate) chain: Option<String>,
	/// Generate a relay chain specification
	#[arg(short='R', long="is-relay", conflicts_with_all=["para_id", "relay"])]
	pub(crate) is_relay: bool,
	/// Relay chain this parachain will connect to.
	#[arg(short = 'r', long, value_enum)]
	pub(crate) relay: Option<RelayChain>,
	/// Name to be used in the specification.
	#[arg(short, long)]
	pub(crate) name: Option<String>,
	/// Id to be used in the specification.
	#[arg(long)]
	pub(crate) id: Option<String>,
	/// Protocol-id to use in the specification.
	#[arg(short = 'P', long = "protocol-id")]
	pub(crate) protocol_id: Option<String>,
	/// The chain properties to use in the specification.
	/// For example, "tokenSymbol=UNIT,decimals=12".
	#[arg(long)]
	pub(crate) properties: Option<String>,
	/// Whether the genesis state file should be generated.
	#[arg(short = 'S', long = "genesis-state")]
	pub(crate) genesis_state: Option<bool>,
	/// Whether the genesis code file should be generated.
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: Option<bool>,
	/// Whether to build the runtime deterministically. This requires Docker running.
	#[arg(short, long)]
	pub(crate) deterministic: Option<bool>,
	/// Whether to use a specific tag for a deterministic build
	#[arg(long, requires = "deterministic")]
	pub(crate) tag: Option<String>,
	/// Define the directory path where the runtime is located.
	#[serde(skip_serializing)]
	#[clap(name = "runtime", long)]
	pub runtime_dir: Option<PathBuf>,
	/// Specify the runtime package name. If not specified, it will be automatically determined
	/// based on `runtime`.
	#[clap(long, requires = "deterministic")]
	pub package: Option<String>,
	/// Generate a raw chain specification.
	#[arg(long)]
	pub(crate) raw: bool,
}

impl BuildSpecCommand {
	/// Executes the build spec command.
	pub(crate) async fn execute(&self, output_mode: OutputMode) -> anyhow::Result<()> {
		match output_mode {
			OutputMode::Human => {
				let mut cli = Cli;
				cli.intro("Generate your chain spec")?;
				// Checks for appchain project.
				if is_supported(&self.path) {
					let build_spec = self.configure_build_spec(&mut cli).await?;
					if let Err(e) = build_spec.build(&mut cli, false).await {
						cli.outro_cancel(e.to_string())?;
					}
				} else {
					cli.outro_cancel(
						"🚫 Can't build a specification for target. Maybe not a chain project ?",
					)?;
				}
				cli.info(self.display())?;
				Ok(())
			},
			OutputMode::Json => self.execute_json().await,
		}
	}

	async fn execute_json(&self) -> anyhow::Result<()> {
		if !is_supported(&self.path) {
			return Err(BuildCommandError::new(
				"Can't build a specification for target. Maybe not a chain project?",
			)
			.into());
		}
		let mut cli = crate::cli::JsonCli;
		let build_spec = self.configure_build_spec_json()?;
		let relay_chain = build_spec.relay.map(|relay| relay.as_ref().to_string());
		let para_id = build_spec.para_id;
		let artifacts =
			build_spec.build(&mut cli, true).await.map_err(map_json_build_spec_error)?;
		CliResponse::ok(BuildSpecOutput {
			chain_spec_path: artifacts.chain_spec.display().to_string(),
			genesis_state_path: artifacts
				.genesis_state_file
				.as_ref()
				.map(|path| path.display().to_string()),
			genesis_code_path: artifacts
				.genesis_code_file
				.as_ref()
				.map(|path| path.display().to_string()),
			relay_chain,
			para_id,
		})
		.print_json();
		Ok(())
	}

	fn display(&self) -> String {
		let mut full_message = "pop build spec".to_string();
		full_message.push_str(&format!(" --path {}", self.path.display()));
		if let Some(output) = &self.output_file {
			full_message.push_str(&format!(" --output {}", output.display()));
		}
		if let Some(profile) = self.profile {
			full_message.push_str(&format!(" --profile {}", profile));
		}
		if let Some(para_id) = self.para_id {
			full_message.push_str(&format!(" --para-id {}", para_id));
		}
		if let Some(bootnode) = self.default_bootnode {
			full_message.push_str(&format!(" --default-bootnode {}", bootnode));
		}
		if let Some(chain_type) = &self.chain_type {
			full_message.push_str(&format!(" --type {}", chain_type));
		}
		if !self.features.is_empty() {
			full_message.push_str(&format!(" --features \"{}\"", self.features));
		}
		if self.skip_build {
			full_message.push_str(" --skip-build");
		}
		if let Some(chain) = &self.chain {
			full_message.push_str(&format!(" --chain {}", chain));
		}
		if self.is_relay {
			full_message.push_str(" --is-relay");
		}
		if let Some(relay) = &self.relay {
			full_message.push_str(&format!(" --relay {}", relay));
		}
		if let Some(name) = &self.name {
			full_message.push_str(&format!(" --name \"{}\"", name));
		}
		if let Some(id) = &self.id {
			full_message.push_str(&format!(" --id {}", id));
		}
		if let Some(protocol_id) = &self.protocol_id {
			full_message.push_str(&format!(" --protocol-id {}", protocol_id));
		}
		if let Some(properties) = &self.properties {
			full_message.push_str(&format!(" --properties \"{}\"", properties));
		}
		if let Some(gs) = self.genesis_state {
			full_message.push_str(&format!(" --genesis-state {}", gs));
		}
		if let Some(gc) = self.genesis_code {
			full_message.push_str(&format!(" --genesis-code {}", gc));
		}
		if let Some(det) = self.deterministic {
			full_message.push_str(&format!(" --deterministic {}", det));
		}
		if let Some(tag) = &self.tag {
			full_message.push_str(&format!(" --tag {}", tag));
		}
		if let Some(runtime_dir) = &self.runtime_dir {
			full_message.push_str(&format!(" --runtime-dir {}", runtime_dir.display()));
		}
		if let Some(package) = &self.package {
			full_message.push_str(&format!(" --package {}", package));
		}
		if self.raw {
			full_message.push_str(" --raw");
		}
		full_message
	}

	fn configure_build_spec_json(&self) -> anyhow::Result<BuildSpec> {
		let output_file = self
			.output_file
			.clone()
			.ok_or_else(|| PromptRequiredError("--output is required with --json".to_string()))?;
		let profile = self
			.profile
			.ok_or_else(|| PromptRequiredError("--profile is required with --json".to_string()))?;
		let chain_type = self
			.chain_type
			.clone()
			.ok_or_else(|| PromptRequiredError("--type is required with --json".to_string()))?;
		let chain = self
			.chain
			.clone()
			.ok_or_else(|| PromptRequiredError("--chain is required with --json".to_string()))?;
		let protocol_id = self.protocol_id.clone().ok_or_else(|| {
			PromptRequiredError("--protocol-id is required with --json".to_string())
		})?;
		let default_bootnode = self.default_bootnode.ok_or_else(|| {
			PromptRequiredError("--default-bootnode is required with --json".to_string())
		})?;
		let genesis_state = self.genesis_state.ok_or_else(|| {
			PromptRequiredError("--genesis-state is required with --json".to_string())
		})?;
		let genesis_code = self.genesis_code.ok_or_else(|| {
			PromptRequiredError("--genesis-code is required with --json".to_string())
		})?;
		let deterministic = self.deterministic.ok_or_else(|| {
			PromptRequiredError("--deterministic is required with --json".to_string())
		})?;

		let (para_id, relay) = if self.is_relay {
			(None, None)
		} else {
			let para_id = self.para_id.ok_or_else(|| {
				PromptRequiredError("--para-id is required with --json".to_string())
			})?;
			let relay = self.relay.ok_or_else(|| {
				PromptRequiredError("--relay is required with --json".to_string())
			})?;
			(Some(para_id), Some(relay))
		};

		let runtime_dir = if deterministic {
			Some(self.runtime_dir.clone().ok_or_else(|| {
				PromptRequiredError(
					"--runtime-dir is required with --json when --deterministic is true"
						.to_string(),
				)
			})?)
		} else {
			self.runtime_dir.clone()
		};

		let package = if deterministic {
			self.package.clone().ok_or_else(|| {
				PromptRequiredError(
					"--package is required with --json when --deterministic is true".to_string(),
				)
			})?
		} else {
			DEFAULT_PACKAGE.to_string()
		};

		let features = self
			.features
			.split(',')
			.map(|s| s.trim())
			.filter(|s| !s.is_empty())
			.map(|s| s.to_string())
			.collect();

		Ok(BuildSpec {
			path: self.path.clone(),
			output_file: prepare_output_path(output_file)?,
			profile,
			is_relay: self.is_relay,
			para_id,
			default_bootnode,
			chain_type,
			chain: Some(chain),
			relay,
			protocol_id,
			name: self.name.clone(),
			id: self.id.clone(),
			properties: self.properties.clone(),
			features,
			skip_build: self.skip_build,
			genesis_state,
			genesis_code,
			deterministic,
			tag: self.tag.clone(),
			package,
			runtime_dir,
			use_existing_plain_spec: false,
			raw: self.raw,
		})
	}

	/// Configure chain specification requirements by prompting for missing inputs, validating
	/// provided values, and preparing a BuildSpec to generate file(s).
	///
	/// # Arguments
	/// * `cli` - The cli.
	pub(crate) async fn configure_build_spec(
		&self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<BuildSpec> {
		let BuildSpecCommand {
			path,
			output_file,
			profile,
			para_id,
			default_bootnode,
			chain_type,
			chain,
			relay,
			name,
			id,
			protocol_id,
			properties,
			features,
			skip_build,
			genesis_state,
			genesis_code,
			deterministic,
			package,
			runtime_dir,
			is_relay,
			raw,
			..
		} = self;

		// Features
		let features = features.split(",").map(|s| s.trim().to_string()).collect();

		// Check if the provided chain specification is a file.
		let (output_file, prompt) = if chain.is_some() &&
			PathBuf::from(&chain.clone().unwrap()).is_file()
		{
			if output_file.is_some() {
				cli.warning(
					"NOTE: If an existing chain spec file is provided it will be used for the output path.",
				)?;
			}
			// Prompt whether the user wants to make additional changes to the provided chain spec
			// file.
			let prompt = cli.confirm("An existing chain spec file is provided. Do you want to make additional changes to it?".to_string())
				.initial_value(false)
				.interact()?;
			// Set the provided chain specification file as output file and whether to prompt the
			// user for additional changes to the provided spec.
			(PathBuf::from(&chain.clone().unwrap()), prompt)
		} else {
			let output_file = match output_file {
				Some(output) => output,
				None => {
					// Prompt for output file if not provided.
					let default_output = format!("./{DEFAULT_SPEC_NAME}");
					&PathBuf::from(
						cli.input("Name or path for the plain chain spec file:")
							.placeholder(&default_output)
							.default_input(&default_output)
							.interact()?,
					)
				},
			};
			(prepare_output_path(output_file)?, true)
		};
		// If chain specification file already exists, obtain values for defaults when prompting.
		let chain_spec = ChainSpec::from(&output_file).ok();

		let (para_id, relay) = if *is_relay {
			(None, None)
		} else {
			// Para id.
			let para_id = match para_id {
				Some(id) => *id,
				None => {
					let default = chain_spec
						.as_ref()
						.and_then(|cs| cs.get_chain_id().map(|id| id as u32))
						.unwrap_or(DEFAULT_PARA_ID);
					if prompt {
						// Prompt for para id.
						let default_str = default.to_string();
						cli.input("What parachain ID should be used?")
							.default_input(&default_str)
							.interact()?
							.parse::<u32>()
							.unwrap_or(DEFAULT_PARA_ID)
					} else {
						default
					}
				},
			};

			// Relay.
			let relay = match relay {
				Some(relay) => *relay,
				None => {
					let default = chain_spec
						.as_ref()
						.and_then(|cs| cs.get_relay_chain())
						.and_then(|r| RelayChain::from_str(r, true).ok())
						.unwrap_or_default();
					if prompt {
						// Prompt for relay.
						let mut prompt = cli
							.select(
								"Choose the relay your chain will be connecting to: ".to_string(),
							)
							.initial_value(default);
						for relay in RelayChain::VARIANTS {
							prompt = prompt.item(
								*relay,
								relay.get_message().unwrap_or(relay.as_ref()),
								relay.get_detailed_message().unwrap_or_default(),
							);
						}
						prompt.interact()?
					} else {
						default
					}
				},
			};
			(Some(para_id), Some(relay))
		};

		// Chain type.
		let chain_type = match chain_type {
			Some(chain_type) => chain_type,
			None => &{
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_chain_type())
					.and_then(|r| ChainType::from_str(r, true).ok())
					.unwrap_or_default();
				if prompt {
					// Prompt for chain type.
					let mut prompt =
						cli.select("Choose the chain type: ".to_string()).initial_value(default);
					for chain_type in ChainType::VARIANTS {
						prompt = prompt.item(
							chain_type.clone(),
							chain_type.get_message().unwrap_or(chain_type.as_ref()),
							chain_type.get_detailed_message().unwrap_or_default(),
						);
					}
					prompt.interact()?
				} else {
					default
				}
			},
		};

		// Prompt user for build profile.
		let profile = match profile {
			Some(profile) => profile,
			None => &{
				let default = Profile::Release;
				if prompt { guide_user_to_select_profile(cli)? } else { default }
			},
		};

		// Protocol id.
		let protocol_id = match protocol_id {
			Some(protocol_id) => protocol_id.clone(),
			None => {
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_protocol_id())
					.unwrap_or(DEFAULT_PROTOCOL_ID)
					.to_string();
				if prompt {
					// Prompt for protocol id.
					cli.input("Enter the protocol ID that will identify your network: ")
						.placeholder(&default)
						.default_input(&default)
						.interact()?
				} else {
					default
				}
			},
		};

		// Prompt for default bootnode if not provided and chain type is Local or Live.
		let default_bootnode = prompt &&
			default_bootnode.unwrap_or_else(|| match chain_type {
				ChainType::Development => true,
				_ => cli
					.confirm("Would you like to use local host as a bootnode ?".to_string())
					.initial_value(true)
					.interact()
					.unwrap_or(true),
			});

		// Prompt for genesis state if not provided.
		let genesis_state = genesis_state.unwrap_or_else(|| {
			cli.confirm("Should the genesis state file be generated ?".to_string())
				.initial_value(true)
				.interact()
				.unwrap_or(true)
		});

		// Prompt for genesis code if not provided.
		let genesis_code = genesis_code.unwrap_or_else(|| {
			cli.confirm("Should the genesis code file be generated ?".to_string())
				.initial_value(true)
				.interact()
				.unwrap_or(true)
		});

		// Prompt the user for deterministic build only if the profile is Production.
		let deterministic = prompt && deterministic.unwrap_or_else(|| cli
            .confirm("Would you like to build the runtime deterministically? This requires a containerization solution (Docker/Podman) and is recommended for production builds.")
            .initial_value(*profile == Profile::Production)
            .interact()
            .unwrap_or(false));
		// If deterministic build is selected, use the provided runtime path or prompt the user if
		// missing.
		let runtime_dir = if deterministic {
			Some(runtime_dir.clone().unwrap_or_else(|| {
				cli.input("Enter the directory path where the runtime is located:")
					.placeholder(DEFAULT_RUNTIME_DIR)
					.default_input(DEFAULT_RUNTIME_DIR)
					.interact()
					.map(PathBuf::from)
					.unwrap_or_else(|_| PathBuf::from(DEFAULT_RUNTIME_DIR))
			}))
		} else {
			runtime_dir.clone()
		};

		// If deterministic build is selected, extract package name from runtime path provided
		// above. Prompt the user if unavailable.
		let package = if deterministic {
			package
				.clone()
				.or_else(|| {
					from_path(
						runtime_dir
							.as_ref()
							.expect("Deterministic builds always have a runtime_dir"),
					)
					.ok()
					.and_then(|manifest| manifest.package.map(|pkg| pkg.name))
				})
				.unwrap_or_else(|| {
					cli.input("Enter the runtime package name:")
						.placeholder(DEFAULT_PACKAGE)
						.default_input(DEFAULT_PACKAGE)
						.interact()
						.unwrap_or_else(|_| DEFAULT_PACKAGE.to_string())
				})
		} else {
			DEFAULT_PACKAGE.to_string()
		};

		Ok(BuildSpec {
			path: path.clone(),
			output_file,
			profile: *profile,
			para_id,
			default_bootnode,
			chain_type: chain_type.clone(),
			is_relay: *is_relay,
			chain: chain.clone(),
			relay,
			protocol_id,
			properties: properties.clone(),
			features,
			name: name.clone(),
			id: id.clone(),
			skip_build: *skip_build,
			genesis_state,
			genesis_code,
			deterministic,
			tag: self.tag.clone(),
			package,
			runtime_dir,
			use_existing_plain_spec: !prompt,
			raw: *raw,
		})
	}
}

fn map_json_build_spec_error(err: anyhow::Error) -> anyhow::Error {
	let message = err.to_string();
	if err.downcast_ref::<PromptRequiredError>().is_some() ||
		err.downcast_ref::<BuildCommandError>().is_some()
	{
		return err;
	}
	if message.contains(super::JSON_PROMPT_ERR) {
		return PromptRequiredError(
			"`build spec --json` requires explicit runtime selection when multiple runtimes are available"
				.to_string(),
		)
		.into();
	}
	BuildCommandError::new("Build spec failed").with_details(message).into()
}

/// Represents the generated chain specification artifacts.
#[derive(Debug, Default, Clone)]
pub struct GenesisArtifacts {
	/// Path to the plain text chain specification file.
	pub chain_spec: PathBuf,
	/// Path to the raw chain specification file.
	pub raw_chain_spec: Option<PathBuf>,
	/// Optional path to the genesis state file.
	pub genesis_state_file: Option<CodePathBuf>,
	/// Optional path to the genesis code file.
	pub genesis_code_file: Option<StatePathBuf>,
}
impl GenesisArtifacts {
	/// Reads the genesis state file as a string.
	pub fn read_genesis_state(&self) -> anyhow::Result<String> {
		std::fs::read_to_string(
			self.genesis_state_file
				.as_ref()
				.ok_or_else(|| anyhow::anyhow!("Missing genesis state file path"))?,
		)
		.map_err(|e| anyhow::anyhow!("Failed to read genesis state file: {}", e))
	}

	/// Reads the genesis code file as a string.
	pub fn read_genesis_code(&self) -> anyhow::Result<String> {
		std::fs::read_to_string(
			self.genesis_code_file
				.as_ref()
				.ok_or_else(|| anyhow::anyhow!("Missing genesis code file path"))?,
		)
		.map_err(|e| anyhow::anyhow!("Failed to read genesis code file: {}", e))
	}
}

// Represents the configuration for building a chain specification.
#[derive(Debug, Default, Clone)]
pub(crate) struct BuildSpec {
	path: PathBuf,
	output_file: PathBuf,
	profile: Profile,
	is_relay: bool,
	para_id: Option<u32>,
	default_bootnode: bool,
	chain_type: ChainType,
	chain: Option<String>,
	relay: Option<RelayChain>,
	protocol_id: String,
	name: Option<String>,
	id: Option<String>,
	properties: Option<String>,
	features: Vec<String>,
	skip_build: bool,
	genesis_state: bool,
	genesis_code: bool,
	deterministic: bool,
	tag: Option<String>,
	package: String,
	runtime_dir: Option<PathBuf>,
	use_existing_plain_spec: bool,
	raw: bool,
}

impl BuildSpec {
	// Executes the process of generating the chain specification.
	//
	// This function generates plain and raw chain spec files based on the provided configuration,
	// optionally including genesis state and runtime artifacts. If the node binary is missing,
	// it triggers a build process.
	pub(crate) async fn build(
		self,
		cli: &mut impl cli::traits::Cli,
		redirect_output_to_stderr: bool,
	) -> anyhow::Result<GenesisArtifacts> {
		let mut generated_files = vec![];
		let builder_path = if let Some(runtime_dir) = &self.runtime_dir {
			ChainPath::Exact(runtime_dir.to_path_buf())
		} else {
			ChainPath::Base(self.path.to_path_buf())
		};
		let builder =
			create_chain_spec_builder(builder_path, &self.profile, self.default_bootnode, cli)?;
		let is_runtime_build = matches!(builder, ChainSpecBuilder::Runtime { .. });
		let artifact_exists = builder.artifact_path().is_ok();
		if self.skip_build && builder.artifact_path().is_err() {
			cli.warning("The node or runtime artifacts are missing. Ignoring the --skip-build flag and performing the build")?;
		}
		if !self.skip_build || !artifact_exists {
			builder.build(&self.features, redirect_output_to_stderr)?;
		}

		// Generate chain spec.
		let spinner = cli.spinner();
		if !self.use_existing_plain_spec {
			let chain_or_preset = if let Some(chain) = self.chain.clone() {
				chain
			} else if is_runtime_build {
				cli.info("Fetching runtime presets...")?;
				let preset_names = pop_chains::get_preset_names(&builder.artifact_path()?)?;
				let mut prompt = cli.select("Select the preset");
				for preset_name in preset_names {
					prompt = prompt.item(preset_name.clone(), preset_name, "");
				}
				prompt.interact()?
			} else {
				cli.input("Provide the chain specification to use (e.g. dev, local, custom or a path to an existing file)")
                           .placeholder(DEFAULT_CHAIN)
                           .default_input(DEFAULT_CHAIN)
                           .interact()?
			};
			spinner.start("Generating chain specification...");
			builder.generate_plain_chain_spec(
				&chain_or_preset,
				&self.output_file,
				self.name.as_deref(),
				self.id.as_deref(),
			)?;
			// Customize spec based on input.
			self.customize(&self.output_file)?;
			// Deterministic build.
			if self.deterministic {
				let (runtime_path, code) = build_deterministic_runtime(
					&self.package,
					self.profile,
					self.runtime_dir
						.clone()
						.expect("Deterministic builds always contains runtime_dir"),
					self.tag.clone(),
				)
				.await
				.map_err(|e| anyhow::anyhow!("Failed to build the deterministic runtime: {e}"))?;
				generated_files
					.push(format!("Runtime file generated at: {}", &runtime_path.display()));
				self.update_code(&code)?;
			}

			generated_files.push(format!(
				"Plain text chain specification file generated at: {}",
				self.output_file.display()
			));
		}

		let (raw_chain_spec, genesis_code_file, genesis_state_file) = if self.raw ||
			self.genesis_code ||
			self.genesis_state
		{
			// Generate raw spec.
			spinner.set_message("Generating raw chain specification...");
			let spec_name = self
				.output_file
				.file_name()
				.and_then(|s| s.to_str())
				.unwrap_or(DEFAULT_SPEC_NAME)
				.trim_end_matches(".json");
			let raw_spec_name = format!("{spec_name}-raw.json");
			let raw_chain_spec =
				builder.generate_raw_chain_spec(&self.output_file, &raw_spec_name)?;
			generated_files.push(format!(
				"Raw chain specification file generated at: {}",
				raw_chain_spec.display()
			));

			if is_runtime_build {
				// The runtime version of the raw chain spec does not include certain parameters,
				// like the relay chain, so we have to overwrite them again.
				self.customize(&raw_chain_spec)?;
			}

			// Generate genesis artifacts.
			let genesis_code_file = if self.genesis_code {
				spinner.set_message("Generating genesis code...");
				let wasm_file = builder.export_wasm_file(&raw_chain_spec, "genesis-code.wasm")?;
				generated_files
					.push(format!("WebAssembly runtime file exported at: {}", wasm_file.display()));
				Some(wasm_file)
			} else {
				None
			};
			let genesis_state_file = if self.genesis_state {
				spinner.set_message("Generating genesis state...");
				let binary_path = match builder {
					ChainSpecBuilder::Runtime { .. } =>
						source_polkadot_omni_node_binary(cli, &spinner, &crate::cache()?, true)
							.await?,
					ChainSpecBuilder::Node { .. } => builder.artifact_path()?,
				};
				let genesis_state_file = generate_genesis_state_file_with_node(
					&binary_path,
					&raw_chain_spec,
					"genesis-state",
				)?;
				generated_files.push(format!(
					"Genesis State file exported at: {}",
					genesis_state_file.display()
				));
				Some(genesis_state_file)
			} else {
				None
			};
			(Some(raw_chain_spec), genesis_code_file, genesis_state_file)
		} else {
			(None, None, None)
		};

		spinner.stop("Chain specification built successfully.");
		if !self.use_existing_plain_spec {
			let generated_files: Vec<_> = generated_files
				.iter()
				.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
				.collect();
			cli.success(format!("Generated files:\n{}", generated_files.join("\n")))?;
			cli.outro(format!(
				"Need help? Learn more at {}\n",
				style("https://learn.onpop.io").magenta().underlined()
			))?;
		}
		Ok(GenesisArtifacts {
			chain_spec: self.output_file.clone(),
			raw_chain_spec,
			genesis_code_file,
			genesis_state_file,
		})
	}

	/// Enables the use of an existing plain chain spec, preventing unnecessary regeneration.
	pub fn enable_existing_plain_spec(&mut self) {
		self.use_existing_plain_spec = true;
	}

	/// Injects collator keys into the chain spec and updates the file.
	///
	/// # Arguments
	/// * `collator_keys` - The list of collator keys to insert.
	/// * `chain_spec_path` - The file path of the chain spec to be updated.
	pub fn update_chain_spec_with_keys(
		&mut self,
		collator_keys: Vec<String>,
		chain_spec_path: &Path,
	) -> anyhow::Result<()> {
		let mut chain_spec = ChainSpec::from(chain_spec_path)?;
		chain_spec.replace_collator_keys(collator_keys)?;
		chain_spec.to_file(chain_spec_path)?;

		self.chain = Some(chain_spec_path.display().to_string());
		Ok(())
	}

	// Customize a chain specification.
	fn customize(&self, path: &Path) -> anyhow::Result<()> {
		let mut chain_spec = ChainSpec::from(path)?;
		if !self.is_relay {
			chain_spec.replace_para_id(
				self.para_id.ok_or_else(|| anyhow::anyhow!("Missing para_id for chain spec"))?,
			)?;
			chain_spec.replace_relay_chain(
				self.relay
					.as_ref()
					.ok_or_else(|| anyhow::anyhow!("Missing relay chain for chain spec"))?
					.as_ref(),
			)?;
		}
		chain_spec.replace_chain_type(self.chain_type.as_ref())?;
		chain_spec.replace_protocol_id(&self.protocol_id)?;
		if let Some(properties) = &self.properties {
			chain_spec.replace_properties(properties)?;
		}
		chain_spec.to_file(path)?;
		Ok(())
	}

	// Updates the chain specification with the runtime code.
	fn update_code(&self, bytes: &[u8]) -> anyhow::Result<()> {
		let mut chain_spec = ChainSpec::from(&self.output_file)?;
		chain_spec.update_runtime_code(bytes)?;
		chain_spec.to_file(&self.output_file)?;
		Ok(())
	}
}

// Prepare the output path provided.
fn prepare_output_path(output_path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
	let mut output_path = output_path.as_ref().to_path_buf();
	// Check if the path ends with '.json'
	let is_json_file = output_path
		.extension()
		.and_then(|ext| ext.to_str())
		.map(|ext| ext.eq_ignore_ascii_case("json"))
		.unwrap_or(false);
	let is_dir = output_path.is_dir();

	if is_dir || (!output_path.exists() && !is_json_file) {
		// Treat as directory (existing or to-be-created)
		if !output_path.exists() {
			create_dir_all(&output_path)?;
		}
		output_path.push(DEFAULT_SPEC_NAME);
	} else {
		// Treat as file; ensure parent dir exists
		if let Some(parent_dir) = output_path.parent() &&
			!parent_dir.exists()
		{
			create_dir_all(parent_dir)?;
		}
	}
	Ok(output_path)
}

#[cfg(test)]
mod tests {
	use super::{ChainType::*, RelayChain::*, *};
	use crate::cli::MockCli;
	use serde_json::json;
	use sp_core::bytes::from_hex;
	use std::{
		fs::{self, create_dir_all},
		path::PathBuf,
	};
	use tempfile::{TempDir, tempdir};

	#[test]
	fn test_build_spec_command_display() {
		let cmd = BuildSpecCommand {
			path: PathBuf::from("./my-project"),
			output_file: Some(PathBuf::from("output.json")),
			profile: Some(Profile::Release),
			para_id: Some(2000),
			default_bootnode: Some(true),
			chain_type: Some(ChainType::Development),
			features: "feature1,feature2".to_string(),
			skip_build: true,
			chain: Some("dev".to_string()),
			is_relay: true,
			relay: Some(RelayChain::Paseo),
			name: Some("My Chain".to_string()),
			id: Some("my_chain".to_string()),
			protocol_id: Some("my_protocol".to_string()),
			properties: Some("tokenSymbol=UNIT,decimals=12".to_string()),
			genesis_state: Some(true),
			genesis_code: Some(true),
			deterministic: Some(true),
			tag: Some("v1".to_string()),
			runtime_dir: Some(PathBuf::from("./runtime")),
			package: Some("my-package".to_string()),
			raw: true,
		};
		assert_eq!(
			cmd.display(),
			"pop build spec --path ./my-project --output output.json --profile release --para-id 2000 --default-bootnode true --type Development --features \"feature1,feature2\" --skip-build --chain dev --is-relay --relay paseo --name \"My Chain\" --id my_chain --protocol-id my_protocol --properties \"tokenSymbol=UNIT,decimals=12\" --genesis-state true --genesis-code true --deterministic true --tag v1 --runtime-dir ./runtime --package my-package --raw"
		);

		let cmd = BuildSpecCommand { path: PathBuf::from("./"), ..Default::default() };
		assert_eq!(cmd.display(), "pop build spec --path ./");
	}

	#[tokio::test]
	async fn configure_build_spec_works() -> anyhow::Result<()> {
		let chain_type = Live;
		let default_bootnode = true;
		let genesis_code = true;
		let genesis_state = true;
		let output_file = "artifacts/chain-spec.json";
		let para_id = 4242;
		let name = "POP Chain Spec";
		let id = "pop";
		let protocol_id = "pop";
		let relay = Polkadot;
		let profile = Profile::Production;
		let deterministic = true;
		let tag = Some("1.88.0".to_owned());
		let package = "runtime-name";
		let runtime_dir = PathBuf::from("./new-runtime-dir");
		let path = PathBuf::from("./");
		let properties = "tokenSymbol=UNIT,decimals=12,isEthereum=false";
		let raw = true;

		let mut flags_used = false;
		for (build_spec_cmd, chain) in [
			// No flags used.
			(BuildSpecCommand::default(), None),
			// All flags used. Parachain
			(
				BuildSpecCommand {
					path: path.clone(),
					output_file: Some(PathBuf::from(output_file)),
					profile: Some(profile),
					name: Some(name.to_string()),
					id: Some(id.to_string()),
					is_relay: false,
					para_id: Some(para_id),
					default_bootnode: Some(default_bootnode),
					chain_type: Some(chain_type.clone()),
					features: "".to_string(),
					chain: Some("local".to_string()),
					relay: Some(relay),
					protocol_id: Some(protocol_id.to_string()),
					properties: Some(properties.to_string()),
					skip_build: true,
					genesis_state: Some(genesis_state),
					genesis_code: Some(genesis_code),
					deterministic: Some(deterministic),
					tag: tag.clone(),
					package: Some(package.to_string()),
					runtime_dir: Some(runtime_dir.clone()),
					raw,
				},
				Some("local".to_string()),
			),
			// All flags used. Relay
			(
				BuildSpecCommand {
					path: path.clone(),
					output_file: Some(PathBuf::from(output_file)),
					profile: Some(profile),
					name: Some(name.to_string()),
					id: Some(id.to_string()),
					is_relay: true,
					para_id: None,
					default_bootnode: Some(default_bootnode),
					chain_type: Some(chain_type.clone()),
					features: "".to_string(),
					chain: Some("local".to_string()),
					relay: None,
					protocol_id: Some(protocol_id.to_string()),
					properties: Some(properties.to_string()),
					skip_build: true,
					genesis_state: Some(genesis_state),
					genesis_code: Some(genesis_code),
					deterministic: Some(deterministic),
					tag: tag.clone(),
					package: Some(package.to_string()),
					runtime_dir: Some(runtime_dir.clone()),
					raw,
				},
				Some("local".to_string()),
			),
		] {
			let mut cli = MockCli::new();
			// If no flags are provided.
			if build_spec_cmd.chain.is_none() {
				cli = cli
					.expect_input(
						"Name or path for the plain chain spec file:",
						output_file.to_string(),
					)
					.expect_input("What parachain ID should be used?", para_id.to_string())
					.expect_select(
						"Choose the relay your chain will be connecting to: ",
						Some(false),
						true,
						Some(relays()),
						relay as usize,
						None,
					)
					.expect_select(
						"Choose the chain type: ",
						Some(false),
						true,
						Some(chain_types()),
						chain_type.clone() as usize,
						None,
					)
					.expect_input(
						"Enter the protocol ID that will identify your network: ",
						protocol_id.to_string(),
					)
					.expect_select(
						"Choose the build profile of the binary that should be used: ",
						Some(false),
						true,
						Some(profiles()),
						profile as usize,
						None,
					)
					.expect_confirm(
						"Would you like to use local host as a bootnode ?",
						default_bootnode,
					)
					.expect_confirm("Should the genesis state file be generated ?", genesis_state)
					.expect_confirm("Should the genesis code file be generated ?", genesis_code)
					.expect_confirm("Would you like to build the runtime deterministically? This requires a containerization solution (Docker/Podman) and is recommended for production builds.", deterministic)
					.expect_input(
						"Enter the directory path where the runtime is located:",
						runtime_dir.display().to_string(),
					)
					.expect_input("Enter the runtime package name:", package.to_string());
			} else {
				flags_used = true;
			}
			let build_spec = build_spec_cmd.configure_build_spec(&mut cli).await?;
			assert_eq!(build_spec.chain, chain);
			assert_eq!(build_spec.output_file, PathBuf::from(output_file));
			assert_eq!(build_spec.profile, profile);
			assert_eq!(build_spec.default_bootnode, default_bootnode);
			assert_eq!(build_spec.chain_type, chain_type);
			assert_eq!(build_spec.protocol_id, protocol_id);
			assert_eq!(build_spec.genesis_state, genesis_state);
			assert_eq!(build_spec.genesis_code, genesis_code);
			assert_eq!(build_spec.deterministic, deterministic);
			assert_eq!(build_spec.package, package);
			assert_eq!(build_spec.runtime_dir, Some(runtime_dir.clone()));
			if flags_used {
				assert_eq!(build_spec.name, Some(name.to_string()));
				assert_eq!(build_spec.id, Some(id.to_string()));
				assert_eq!(build_spec.tag, tag);
			} else {
				assert_eq!(build_spec.name, None);
				assert_eq!(build_spec.id, None);
				assert_eq!(build_spec.tag, None);
			}

			if build_spec.is_relay {
				assert_eq!(build_spec.para_id, None);
				assert_eq!(build_spec.relay, None);
			} else {
				assert_eq!(build_spec.para_id, Some(para_id));
				assert_eq!(build_spec.relay, Some(relay));
			}

			cli.verify()?;
		}
		Ok(())
	}

	#[tokio::test]
	async fn configure_build_spec_with_existing_chain_file() -> anyhow::Result<()> {
		let chain_type = Live;
		let default_bootnode = true;
		let genesis_code = true;
		let genesis_state = true;
		let output_file = "artifacts/chain-spec.json";
		let para_id = 4242;
		let name = "POP Chain Spec";
		let id = "pop";
		let protocol_id = "pop";
		let relay = Polkadot;
		let profile = Profile::Production;
		let deterministic = true;
		let package = "runtime-name";
		let runtime_dir = PathBuf::from("./new-runtime-dir");
		let path = PathBuf::from("./");
		let properties = "tokenSymbol=UNIT,decimals=12,isEthereum=false";

		// Create a temporary file to act as the existing chain spec file.
		let temp_dir = tempdir()?;
		let chain_spec_path = temp_dir.path().join("existing-chain-spec.json");
		fs::write(&chain_spec_path, "{}")?; // Write a dummy JSON to the file.

		// Whether to make changes to the provided chain spec file.
		for changes in [true, false] {
			for build_spec_cmd in [
				// No flags used except the provided chain spec file.
				BuildSpecCommand {
					chain: Some(chain_spec_path.to_string_lossy().to_string()),
					..Default::default()
				},
				// All flags used. Parachain
				BuildSpecCommand {
					path: path.clone(),
					output_file: Some(PathBuf::from(output_file)),
					profile: Some(profile),
					is_relay: false,
					para_id: Some(para_id),
					default_bootnode: None,
					chain_type: Some(chain_type.clone()),
					features: "".to_string(),
					name: Some(name.to_string()),
					id: Some(id.to_string()),
					chain: Some(chain_spec_path.to_string_lossy().to_string()),
					relay: Some(relay),
					protocol_id: Some(protocol_id.to_string()),
					properties: Some(properties.to_string()),
					skip_build: true,
					genesis_state: None,
					genesis_code: None,
					deterministic: None,
					tag: None,
					package: Some(package.to_string()),
					runtime_dir: Some(runtime_dir.clone()),
					raw: true,
				},
				// All flags used. Relay
				BuildSpecCommand {
					path: path.clone(),
					output_file: Some(PathBuf::from(output_file)),
					profile: Some(profile),
					is_relay: true,
					para_id: None,
					default_bootnode: None,
					chain_type: Some(chain_type.clone()),
					features: "".to_string(),
					name: Some(name.to_string()),
					id: Some(id.to_string()),
					chain: Some(chain_spec_path.to_string_lossy().to_string()),
					relay: None,
					protocol_id: Some(protocol_id.to_string()),
					properties: Some(properties.to_string()),
					skip_build: true,
					genesis_state: None,
					genesis_code: None,
					deterministic: None,
					tag: None,
					package: Some(package.to_string()),
					runtime_dir: Some(runtime_dir.clone()),
					raw: true,
				},
			] {
				let mut cli = MockCli::new().expect_confirm(
					"An existing chain spec file is provided. Do you want to make additional changes to it?",
					changes,
				);
				// When user wants to make changes to chain spec file via prompts and no flags
				// provided.
				let no_flags_used = build_spec_cmd.runtime_dir.is_none();
				if changes && no_flags_used {
					if build_spec_cmd.para_id.is_none() {
						cli = cli
							.expect_input("What parachain ID should be used?", para_id.to_string());
					}
					if build_spec_cmd.relay.is_none() {
						cli = cli.expect_select(
							"Choose the relay your chain will be connecting to: ",
							Some(false),
							true,
							Some(relays()),
							relay as usize,
							None,
						);
					}
					if build_spec_cmd.chain_type.is_none() {
						cli = cli.expect_select(
							"Choose the chain type: ",
							Some(false),
							true,
							Some(chain_types()),
							chain_type.clone() as usize,
							None,
						);
					}
					if build_spec_cmd.profile.is_none() {
						cli = cli.expect_select(
							"Choose the build profile of the binary that should be used: ",
							Some(false),
							true,
							Some(profiles()),
							profile as usize,
							None,
						);
					}
					if build_spec_cmd.protocol_id.is_none() {
						cli = cli.expect_input(
							"Enter the protocol ID that will identify your network: ",
							protocol_id.to_string(),
						);
					}
					if build_spec_cmd.default_bootnode.is_none() {
						cli = cli.expect_confirm(
							"Would you like to use local host as a bootnode ?",
							default_bootnode,
						);
					}
					if build_spec_cmd.genesis_state.is_none() {
						cli = cli
							.expect_confirm("Should the genesis state file be generated ?", true);
					}
					if build_spec_cmd.genesis_code.is_none() {
						cli =
							cli.expect_confirm("Should the genesis code file be generated ?", true);
					}
					if build_spec_cmd.deterministic.is_none() {
						cli = cli.expect_confirm(
							"Would you like to build the runtime deterministically? This requires a containerization solution (Docker/Podman) and is recommended for production builds.",
							true,
						).expect_input("Enter the directory path where the runtime is located:", runtime_dir.display().to_string())
						.expect_input("Enter the runtime package name:", package.to_string());
					}
				} else if !changes && no_flags_used {
					if build_spec_cmd.genesis_state.is_none() {
						cli = cli
							.expect_confirm("Should the genesis state file be generated ?", true);
					}
					if build_spec_cmd.genesis_code.is_none() {
						cli =
							cli.expect_confirm("Should the genesis code file be generated ?", true);
					}
				}
				let build_spec = build_spec_cmd.configure_build_spec(&mut cli).await?;
				if !changes && no_flags_used {
					assert_eq!(build_spec.para_id, Some(2000));
					assert_eq!(build_spec.chain_type, Development);
					assert_eq!(build_spec.relay, Some(PaseoLocal));
					assert_eq!(build_spec.protocol_id, "my-protocol");
					assert_eq!(build_spec.name, None);
					assert_eq!(build_spec.id, None);
					assert_eq!(build_spec.genesis_state, genesis_state);
					assert_eq!(build_spec.genesis_code, genesis_code);
					assert!(!build_spec.deterministic);
					assert_eq!(build_spec.tag, None);
					assert_eq!(build_spec.package, DEFAULT_PACKAGE);
					assert_eq!(build_spec.runtime_dir, None);
				} else if changes && no_flags_used {
					assert_eq!(build_spec.para_id, Some(para_id));
					assert_eq!(build_spec.profile, profile);
					assert_eq!(build_spec.default_bootnode, default_bootnode);
					assert_eq!(build_spec.chain_type, chain_type);
					assert_eq!(build_spec.name, None);
					assert_eq!(build_spec.id, None);
					assert_eq!(build_spec.relay, Some(relay));
					assert_eq!(build_spec.protocol_id, protocol_id);
					assert_eq!(build_spec.genesis_state, genesis_state);
					assert_eq!(build_spec.genesis_code, genesis_code);
					assert_eq!(build_spec.deterministic, deterministic);
					assert_eq!(build_spec.tag, None);
					assert_eq!(build_spec.package, package);
					assert_eq!(build_spec.runtime_dir, Some(runtime_dir.clone()));
				} else if !no_flags_used {
					assert_eq!(build_spec.profile, profile);
					assert!(!build_spec.default_bootnode);
					assert_eq!(build_spec.chain_type, chain_type);
					assert_eq!(build_spec.name, Some(name.to_string()));
					assert_eq!(build_spec.id, Some(id.to_string()));
					if build_spec.is_relay {
						assert_eq!(build_spec.para_id, None);
						assert_eq!(build_spec.relay, None);
					} else {
						assert_eq!(build_spec.para_id, Some(para_id));
						assert_eq!(build_spec.relay, Some(relay));
					}
					assert_eq!(build_spec.protocol_id, protocol_id);
					assert!(!build_spec.genesis_state);
					assert!(!build_spec.genesis_code);
					assert!(!build_spec.deterministic);
					assert_eq!(build_spec.tag, None);
					assert_eq!(build_spec.package, "parachain-template-runtime");
					assert_eq!(build_spec.runtime_dir, Some(runtime_dir.clone()));
				}
				// Assert that the chain spec file is correctly detected and used.
				assert_eq!(build_spec.chain, Some(chain_spec_path.to_string_lossy().to_string()));
				assert_eq!(build_spec.output_file, chain_spec_path);
				cli.verify()?;
			}
		}
		Ok(())
	}

	#[test]
	fn update_code_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let output_file = temp_dir.path().join("chain_spec.json");
		fs::write(
			&output_file,
			json!({
				"genesis": {
					"runtimeGenesis": {
						"code": "0x00"
					}
				}
			})
			.to_string(),
		)?;
		let build_spec = BuildSpec { output_file: output_file.clone(), ..Default::default() };
		build_spec.update_code(&from_hex("0x1234")?)?;

		let updated_output_file: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(&output_file)?)?;
		assert_eq!(
			updated_output_file,
			json!({
				"genesis": {
					"runtimeGenesis": {
						"code": "0x1234"
					}
				}
			})
		);
		Ok(())
	}

	#[test]
	fn update_chain_spec_with_keys_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let output_file = temp_dir.path().join("chain_spec.json");
		fs::write(
			&output_file,
			json!({
				"genesis": {
					"runtimeGenesis": {
						"patch": {
						"collatorSelection": {
							"invulnerables": [
							  "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
							]
						  },
						  "session": {
							"keys": [
							  [
								"5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
								"5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
								{
								  "aura": "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
								}
							  ],
							]
						}
						}
					}
				}
			})
			.to_string(),
		)?;
		let mut build_spec = BuildSpec { output_file: output_file.clone(), ..Default::default() };
		build_spec.update_chain_spec_with_keys(
			vec!["5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".to_string()],
			&output_file,
		)?;

		let updated_output_file: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(build_spec.chain.clone().unwrap())?)?;

		assert_eq!(build_spec.chain, Some(output_file.display().to_string()));
		assert_eq!(
			updated_output_file,
			json!({
				"genesis": {
					"runtimeGenesis": {
						"patch": {
						"collatorSelection": {
							"invulnerables": [
							  "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
							]
						  },
						  "session": {
							"keys": [
							  [
								"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
								"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
								{
								  "aura": "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty"
								}
							  ],
							]
						},
						}
					}
				}
			})
		);
		Ok(())
	}

	#[test]
	fn prepare_output_path_works() -> anyhow::Result<()> {
		// Create a temporary directory for testing.
		let temp_dir = TempDir::new()?;
		let temp_dir_path = temp_dir.path();

		// No directory path.
		let file = temp_dir_path.join("chain-spec.json");
		let result = prepare_output_path(&file)?;
		// Expected path: chain-spec.json
		assert_eq!(result, file);

		// Existing directory Path.
		for dir in ["existing_dir", "existing_dir/", "existing_dir_json"] {
			let existing_dir = temp_dir_path.join(dir);
			create_dir_all(&existing_dir)?;
			let result = prepare_output_path(&existing_dir)?;
			// Expected path: existing_dir/chain-spec.json
			let expected_path = existing_dir.join(DEFAULT_SPEC_NAME);
			assert_eq!(result, expected_path);
		}

		// Non-existing directory Path.
		for dir in ["non_existing_dir", "non_existing_dir/", "non_existing_dir_json"] {
			let non_existing_dir = temp_dir_path.join(dir);
			let result = prepare_output_path(&non_existing_dir)?;
			// Expected path: non_existing_dir/chain-spec.json
			let expected_path = non_existing_dir.join(DEFAULT_SPEC_NAME);
			assert_eq!(result, expected_path);
			// The directory should now exist.
			assert!(result.parent().unwrap().exists());
		}

		Ok(())
	}

	#[test]
	fn read_genesis_state_works() -> anyhow::Result<()> {
		let mut artifacts = GenesisArtifacts::default();
		let temp_dir = tempdir()?;
		// Expect failure when the genesis state is `None`.
		assert!(matches!(
			artifacts.read_genesis_state(),
			Err(message) if message.to_string().contains("Missing genesis state file path")
		));
		let genesis_state_path = temp_dir.path().join("genesis_state");
		artifacts.genesis_state_file = Some(genesis_state_path.clone());
		// Expect failure when the genesis state file cannot be read.
		assert!(matches!(
			artifacts.read_genesis_state(),
			Err(message) if message.to_string().contains("Failed to read genesis state file")
		));
		// Successfully read the genesis state file.
		fs::write(&genesis_state_path, "0x1234")?;
		assert_eq!(artifacts.read_genesis_state()?, "0x1234");

		Ok(())
	}

	#[test]
	fn read_genesis_code_works() -> anyhow::Result<()> {
		let mut artifacts = GenesisArtifacts::default();
		let temp_dir = tempdir()?;
		// Expect failure when the genesis code is None.
		assert!(matches!(
			artifacts.read_genesis_code(),
			Err(message) if message.to_string().contains("Missing genesis code file path")
		));
		let genesis_code_path = temp_dir.path().join("genesis_code.wasm");
		artifacts.genesis_code_file = Some(genesis_code_path.clone());
		// Expect failure when the genesis code file cannot be read.
		assert!(matches!(
			artifacts.read_genesis_code(),
			Err(message) if message.to_string().contains("Failed to read genesis code file")
		));
		// Successfully read the genesis code file.
		fs::write(&genesis_code_path, "0x1234")?;
		assert_eq!(artifacts.read_genesis_code()?, "0x1234");

		Ok(())
	}

	#[test]
	fn configure_build_spec_json_requires_flags() {
		let err = BuildSpecCommand::default().configure_build_spec_json().unwrap_err();
		assert!(err.downcast_ref::<PromptRequiredError>().is_some());
		assert!(err.to_string().contains("--output is required with --json"));
	}

	#[test]
	fn map_json_build_spec_error_preserves_prompt_required_errors() {
		let mapped = map_json_build_spec_error(
			PromptRequiredError("--runtime-dir is required with --json".to_string()).into(),
		);
		assert!(mapped.downcast_ref::<PromptRequiredError>().is_some());
		assert!(mapped.to_string().contains("--runtime-dir is required with --json"));
	}

	#[test]
	fn map_json_build_spec_error_maps_json_prompt_failures_to_prompt_required() {
		let mapped =
			map_json_build_spec_error(anyhow::anyhow!("prefix: {}", super::super::JSON_PROMPT_ERR));
		assert!(mapped.downcast_ref::<PromptRequiredError>().is_some());
		assert!(
			mapped
				.to_string()
				.contains("`build spec --json` requires explicit runtime selection")
		);
	}

	#[test]
	fn configure_build_spec_json_builds_non_interactive_spec() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let output_file = temp_dir.path().join("chain-spec.json");
		let command = BuildSpecCommand {
			path: PathBuf::from("./"),
			output_file: Some(output_file.clone()),
			profile: Some(Profile::Release),
			para_id: Some(2000),
			default_bootnode: Some(false),
			chain_type: Some(ChainType::Development),
			features: "runtime-benchmarks".to_string(),
			skip_build: true,
			chain: Some("local".to_string()),
			is_relay: false,
			relay: Some(RelayChain::Paseo),
			name: Some("Json Spec".to_string()),
			id: Some("json_spec".to_string()),
			protocol_id: Some("json".to_string()),
			properties: Some("tokenSymbol=UNIT,decimals=12".to_string()),
			genesis_state: Some(true),
			genesis_code: Some(false),
			deterministic: Some(false),
			tag: None,
			runtime_dir: None,
			package: None,
			raw: false,
		};
		let build_spec = command.configure_build_spec_json()?;
		assert_eq!(build_spec.output_file, output_file);
		assert_eq!(build_spec.profile, Profile::Release);
		assert_eq!(build_spec.para_id, Some(2000));
		assert_eq!(build_spec.relay, Some(RelayChain::Paseo));
		assert_eq!(build_spec.protocol_id, "json".to_string());
		assert_eq!(build_spec.features, vec!["runtime-benchmarks".to_string()]);
		assert!(!build_spec.use_existing_plain_spec);
		Ok(())
	}

	fn relays() -> Vec<(String, String)> {
		RelayChain::VARIANTS
			.iter()
			.map(|variant| {
				(
					variant.get_message().unwrap_or(variant.as_ref()).into(),
					variant.get_detailed_message().unwrap_or_default().into(),
				)
			})
			.collect()
	}

	fn chain_types() -> Vec<(String, String)> {
		ChainType::VARIANTS
			.iter()
			.map(|variant| {
				(
					variant.get_message().unwrap_or(variant.as_ref()).into(),
					variant.get_detailed_message().unwrap_or_default().into(),
				)
			})
			.collect()
	}

	fn profiles() -> Vec<(String, String)> {
		Profile::VARIANTS
			.iter()
			.map(|variant| {
				(
					variant.get_message().unwrap_or(variant.as_ref()).into(),
					variant.get_detailed_message().unwrap_or_default().into(),
				)
			})
			.collect()
	}
}
