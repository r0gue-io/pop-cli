// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self,
		traits::{Cli as _, *},
		Cli,
	},
	common::{
		builds::{ensure_node_binary_exists, guide_user_to_select_profile},
		runtime::build_deterministic_runtime,
	},
	style::style,
};
use clap::{Args, ValueEnum};
use cliclack::spinner;
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	export_wasm_file, generate_genesis_state_file, generate_plain_chain_spec,
	generate_raw_chain_spec, is_supported, ChainSpec,
};
use std::{
	env::current_dir,
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
	Default,
	Debug,
	Display,
	EnumString,
	EnumMessage,
	ValueEnum,
	VariantArray,
	Eq,
	PartialEq,
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
#[derive(Args, Default)]
pub struct BuildSpecCommand {
	/// File name for the resulting spec. If a path is given,
	/// the necessary directories will be created
	#[arg(short, long = "output")]
	pub(crate) output_file: Option<PathBuf>,
	/// Build profile for the binary to generate the chain specification.
	#[arg(long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short, long)]
	pub(crate) id: Option<u32>,
	/// Whether to keep localhost as a bootnode.
	#[arg(short = 'b', long)]
	pub(crate) default_bootnode: bool,
	/// Type of the chain.
	#[arg(short = 't', long = "type", value_enum)]
	pub(crate) chain_type: Option<ChainType>,
	/// Provide the chain specification to use (e.g. dev, local, custom or a path to an existing
	/// file).
	#[arg(short, long)]
	pub(crate) chain: Option<String>,
	/// Relay chain this parachain will connect to.
	#[arg(short = 'r', long, value_enum)]
	pub(crate) relay: Option<RelayChain>,
	/// Protocol-id to use in the specification.
	#[arg(short = 'P', long = "protocol-id")]
	pub(crate) protocol_id: Option<String>,
	/// Whether the genesis state file should be generated.
	#[arg(short = 'S', long = "genesis-state")]
	pub(crate) genesis_state: bool,
	/// Whether the genesis code file should be generated.
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: bool,
	/// Whether to build the runtime deterministically. This requires a containerization solution
	/// (Docker/Podman).
	#[arg(short, long)]
	pub(crate) deterministic: bool,
	/// Skips the confirmation prompt for deterministic build.
	#[arg(long, conflicts_with = "deterministic")]
	pub(crate) skip_deterministic_build: bool,
	/// Define the directory path where the runtime is located.
	#[clap(name = "runtime", long, requires = "deterministic")]
	pub runtime_dir: Option<PathBuf>,
	/// Specify the runtime package name. If not specified, it will be automatically determined
	/// based on `runtime`.
	#[clap(long, requires = "deterministic")]
	pub package: Option<String>,
}

impl BuildSpecCommand {
	/// Executes the build spec command.
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		let mut cli = Cli;
		cli.intro("Generate your chain spec")?;
		// Checks for appchain project in `./`.
		if is_supported(None)? {
			let build_spec = self.configure_build_spec(&mut cli).await?;
			if let Err(e) = build_spec.build(&mut cli) {
				cli.outro_cancel(e.to_string())?;
			}
		} else {
			cli.outro_cancel(
				"🚫 Can't build a specification for target. Maybe not a chain project ?",
			)?;
		}
		Ok(())
	}

	/// Configure chain specification requirements by prompting for missing inputs, validating
	/// provided values, and preparing a BuildSpec to generate file(s).
	///
	/// # Arguments
	/// * `cli` - The cli.
	pub(crate) async fn configure_build_spec(
		self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<BuildSpec> {
		let BuildSpecCommand {
			output_file,
			profile,
			id,
			default_bootnode,
			chain_type,
			chain,
			relay,
			protocol_id,
			genesis_state,
			genesis_code,
			deterministic,
			skip_deterministic_build,
			package,
			runtime_dir,
		} = self;

		// Chain.
		let chain = match chain {
			Some(chain) => chain,
			_ => {
				// Prompt for chain if not provided.
				cli.input("Provide the chain specification to use (e.g. dev, local, custom or a path to an existing file)")
					.placeholder(DEFAULT_CHAIN)
					.default_input(DEFAULT_CHAIN)
					.interact()?
			},
		};

		// Output file.
		let maybe_chain_spec_file = PathBuf::from(&chain);
		// Check if the provided chain specification is a file.
		let (output_file, prompt) = if maybe_chain_spec_file.exists() &&
			maybe_chain_spec_file.is_file()
		{
			if output_file.is_some() {
				cli.warning("NOTE: If an existing chain spec file is provided it will be used for the output path.")?;
			}
			// Prompt whether the user wants to make additional changes to the provided chain spec
			// file.
			let prompt = cli.confirm("An existing chain spec file is provided. Do you want to make additional changes to it?".to_string())
				.initial_value(false)
				.interact()?;
			// Set the provided chain specification file as output file and whether to prompt the
			// user for additional changes to the provided spec.
			(maybe_chain_spec_file, prompt)
		} else {
			let output_file = match output_file {
				Some(output) => output,
				None => {
					// Prompt for output file if not provided.
					let default_output = format!("./{DEFAULT_SPEC_NAME}");
					PathBuf::from(
						cli.input("Name or path for the plain chain spec file:")
							.placeholder(&default_output)
							.default_input(&default_output)
							.interact()?,
					)
				},
			};
			(prepare_output_path(&output_file)?, true)
		};
		// If chain specification file already exists, obtain values for defaults when prompting.
		let chain_spec = ChainSpec::from(&output_file).ok();

		// Para id.
		let id = match id {
			Some(id) => id,
			None => {
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_parachain_id().map(|id| id as u32))
					.unwrap_or(DEFAULT_PARA_ID);
				if prompt {
					// Prompt for para id.
					let default_str = default.to_string();
					cli.input("What parachain ID should be used?")
						.default_input(&default_str)
						.default_input(&default_str)
						.interact()?
						.parse::<u32>()
						.unwrap_or(DEFAULT_PARA_ID)
				} else {
					default
				}
			},
		};

		// Chain type.
		let chain_type = match chain_type {
			Some(chain_type) => chain_type,
			None => {
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_chain_type())
					.and_then(|r| ChainType::from_str(r, true).ok())
					.unwrap_or_default();
				if prompt {
					// Prompt for chain type.
					let mut prompt =
						cli.select("Choose the chain type: ".to_string()).initial_value(&default);
					for chain_type in ChainType::VARIANTS {
						prompt = prompt.item(
							chain_type,
							chain_type.get_message().unwrap_or(chain_type.as_ref()),
							chain_type.get_detailed_message().unwrap_or_default(),
						);
					}
					prompt.interact()?.clone()
				} else {
					default
				}
			},
		};

		// Relay.
		let relay = match relay {
			Some(relay) => relay,
			None => {
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_relay_chain())
					.and_then(|r| RelayChain::from_str(r, true).ok())
					.unwrap_or_default();
				if prompt {
					// Prompt for relay.
					let mut prompt = cli
						.select("Choose the relay your chain will be connecting to: ".to_string())
						.initial_value(&default);
					for relay in RelayChain::VARIANTS {
						prompt = prompt.item(
							relay,
							relay.get_message().unwrap_or(relay.as_ref()),
							relay.get_detailed_message().unwrap_or_default(),
						);
					}
					prompt.interact()?.clone()
				} else {
					default
				}
			},
		};

		// Prompt user for build profile.
		let profile = match profile {
			Some(profile) => profile,
			None => {
				let default = Profile::Release;
				if prompt {
					guide_user_to_select_profile(cli)?
				} else {
					default
				}
			},
		};

		// Protocol id.
		let protocol_id = match protocol_id {
			Some(protocol_id) => protocol_id,
			None => {
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_protocol_id())
					.unwrap_or(DEFAULT_PROTOCOL_ID)
					.to_string();
				if prompt {
					// Prompt for protocol id.
					cli.input("Enter the protocol ID that will identify your network:")
						.placeholder(&default)
						.default_input(&default)
						.interact()?
				} else {
					default
				}
			},
		};

		// Prompt for default bootnode if not provided and chain type is Local or Live.
		let default_bootnode = if !default_bootnode && prompt {
			match chain_type {
				ChainType::Development => true,
				_ => cli
					.confirm("Would you like to use local host as a bootnode ?".to_string())
					.interact()?,
			}
		} else {
			true
		};

		// Prompt for genesis state if not provided.
		let genesis_state = if !genesis_state {
			cli.confirm("Should the genesis state file be generated ?".to_string())
				.initial_value(true)
				.interact()?
		} else {
			true
		};

		// Prompt for genesis code if not provided.
		let genesis_code = if !genesis_code {
			cli.confirm("Should the genesis code file be generated ?".to_string())
				.initial_value(true)
				.interact()?
		} else {
			true
		};

		// Prompt the user for deterministic build only if the profile is Production.
		let deterministic = if skip_deterministic_build || !prompt {
			false
		} else {
			deterministic || cli
				.confirm("Would you like to build the runtime deterministically? This requires a containerization solution (Docker/Podman) and is recommended for production builds.")
				.initial_value(profile == Profile::Production)
				.interact()?
		};

		// If deterministic build is selected, use the provided runtime path or prompt the user if
		// missing.
		let runtime_dir = if deterministic {
			runtime_dir.unwrap_or_else(|| {
				cli.input("Enter the directory path where the runtime is located:")
					.placeholder(DEFAULT_RUNTIME_DIR)
					.default_input(DEFAULT_RUNTIME_DIR)
					.interact()
					.map(PathBuf::from)
					.unwrap_or_else(|_| PathBuf::from(DEFAULT_RUNTIME_DIR))
			})
		} else {
			DEFAULT_RUNTIME_DIR.into()
		};

		// If deterministic build is selected, extract package name from runtime path provided
		// above. Prompt the user if unavailable.
		let package = if deterministic {
			package
				.or_else(|| {
					from_path(Some(&runtime_dir))
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
			output_file,
			profile,
			id,
			default_bootnode,
			chain_type,
			chain,
			relay,
			protocol_id,
			genesis_state,
			genesis_code,
			deterministic,
			package,
			runtime_dir,
			use_existing_plain_spec: !prompt,
		})
	}
}

/// Represents the generated chain specification artifacts.
#[derive(Debug, Default, Clone)]
pub struct GenesisArtifacts {
	/// Path to the plain text chain specification file.
	pub chain_spec: PathBuf,
	/// Path to the raw chain specification file.
	pub raw_chain_spec: PathBuf,
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
	output_file: PathBuf,
	profile: Profile,
	id: u32,
	default_bootnode: bool,
	chain_type: ChainType,
	chain: String,
	relay: RelayChain,
	protocol_id: String,
	genesis_state: bool,
	genesis_code: bool,
	deterministic: bool,
	package: String,
	runtime_dir: PathBuf,
	use_existing_plain_spec: bool,
}

impl BuildSpec {
	// Executes the process of generating the chain specification.
	//
	// This function generates plain and raw chain spec files based on the provided configuration,
	// optionally including genesis state and runtime artifacts. If the node binary is missing,
	// it triggers a build process.
	pub(crate) fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<GenesisArtifacts> {
		cli.intro("Building your chain spec")?;
		let cwd = current_dir().unwrap_or(PathBuf::from("./"));
		let mut generated_files = vec![];
		let BuildSpec {
			ref output_file,
			ref profile,
			id,
			default_bootnode,
			ref chain,
			genesis_state,
			genesis_code,
			use_existing_plain_spec,
			..
		} = self;
		// Ensure binary is built.
		let binary_path = ensure_node_binary_exists(cli, &cwd, profile, vec![])?;
		let spinner = spinner();
		if !use_existing_plain_spec {
			spinner.start("Generating chain specification...");
			// Generate chain spec.
			generate_plain_chain_spec(&binary_path, output_file, default_bootnode, chain)?;
			// Customize spec based on input.
			self.customize()?;
			// Deterministic build.
			if self.deterministic {
				let (runtime_path, code) = build_deterministic_runtime(
					cli,
					&spinner,
					&self.package,
					self.profile.clone(),
					self.runtime_dir.clone(),
				)
				.map_err(|e| {
					anyhow::anyhow!("Failed to build the deterministic runtime: {}", e.to_string())
				})?;
				generated_files
					.push(format!("Runtime file generated at: {}", &runtime_path.display()));
				self.update_code(&code)?;
			}

			generated_files.push(format!(
				"Plain text chain specification file generated at: {}",
				&output_file.display()
			));
		}

		// Generate raw spec.
		spinner.set_message("Generating raw chain specification...");
		let spec_name = &output_file
			.file_name()
			.and_then(|s| s.to_str())
			.unwrap_or(DEFAULT_SPEC_NAME)
			.trim_end_matches(".json");
		let raw_spec_name = format!("{spec_name}-raw.json");
		let raw_chain_spec = generate_raw_chain_spec(&binary_path, output_file, &raw_spec_name)?;
		generated_files.push(format!(
			"Raw chain specification file generated at: {}",
			raw_chain_spec.display()
		));

		// Generate genesis artifacts.
		let genesis_code_file = if genesis_code {
			spinner.set_message("Generating genesis code...");
			let wasm_file_name = format!("para-{}.wasm", id);
			let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, &wasm_file_name)?;
			generated_files
				.push(format!("WebAssembly runtime file exported at: {}", wasm_file.display()));
			Some(wasm_file)
		} else {
			None
		};
		let genesis_state_file = if genesis_state {
			spinner.set_message("Generating genesis state...");
			let genesis_file_name = format!("para-{}-genesis-state", id);
			let genesis_state_file =
				generate_genesis_state_file(&binary_path, &raw_chain_spec, &genesis_file_name)?;
			generated_files
				.push(format!("Genesis State file exported at: {}", genesis_state_file.display()));
			Some(genesis_state_file)
		} else {
			None
		};

		spinner.stop("Chain specification built successfully.");
		if !use_existing_plain_spec {
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
			chain_spec: output_file.clone(),
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

		self.chain = chain_spec_path.display().to_string();
		Ok(())
	}

	// Customize a chain specification.
	fn customize(&self) -> anyhow::Result<()> {
		let mut chain_spec = ChainSpec::from(&self.output_file)?;
		chain_spec.replace_para_id(self.id)?;
		chain_spec.replace_relay_chain(self.relay.as_ref())?;
		chain_spec.replace_chain_type(self.chain_type.as_ref())?;
		chain_spec.replace_protocol_id(&self.protocol_id)?;
		chain_spec.to_file(&self.output_file)?;
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

	if !is_json_file {
		// Treat as directory.
		if !output_path.exists() {
			create_dir_all(&output_path)?;
		}
		output_path.push(DEFAULT_SPEC_NAME);
	} else {
		// Treat as file.
		if let Some(parent_dir) = output_path.parent() {
			if !parent_dir.exists() {
				create_dir_all(parent_dir)?;
			}
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
	use tempfile::{tempdir, TempDir};

	#[tokio::test]
	async fn configure_build_spec_works() -> anyhow::Result<()> {
		let chain = "local";
		let chain_type = Live;
		let default_bootnode = true;
		let genesis_code = true;
		let genesis_state = true;
		let output_file = "artifacts/chain-spec.json";
		let para_id = 4242;
		let protocol_id = "pop";
		let relay = Polkadot;
		let profile = Profile::Production;
		let deterministic = true;
		let package = "runtime-name";
		let runtime_dir = PathBuf::from("./new-runtime-dir");

		for build_spec_cmd in [
			// No flags used.
			BuildSpecCommand::default(),
			// All flags used.
			BuildSpecCommand {
				output_file: Some(PathBuf::from(output_file)),
				profile: Some(profile.clone()),
				id: Some(para_id),
				default_bootnode,
				chain_type: Some(chain_type.clone()),
				chain: Some(chain.to_string()),
				relay: Some(relay.clone()),
				protocol_id: Some(protocol_id.to_string()),
				genesis_state,
				genesis_code,
				deterministic,
				skip_deterministic_build: false,
				package: Some(package.to_string()),
				runtime_dir: Some(runtime_dir.clone()),
			},
		] {
			let mut cli = MockCli::new();
			// If no flags are provided.
			if build_spec_cmd.chain.is_none() {
				cli = cli
					.expect_input("Provide the chain specification to use (e.g. dev, local, custom or a path to an existing file)", chain.to_string())
					.expect_input(
						"Name or path for the plain chain spec file:", output_file.to_string())
					.expect_input(
						"What parachain ID should be used?", para_id.to_string())
					.expect_input(
						"Enter the protocol ID that will identify your network:", protocol_id.to_string())
					.expect_select(
						"Choose the chain type: ",
						Some(false),
						true,
						Some(chain_types()),
						chain_type.clone() as usize,
						None,
					).expect_select(
					"Choose the relay your chain will be connecting to: ",
					Some(false),
					true,
					Some(relays()),
					relay.clone() as usize,
					None,
				).expect_select(
					"Choose the build profile of the binary that should be used: ",
					Some(false),
					true,
					Some(profiles()),
					profile.clone() as usize,
					None,
				).expect_confirm("Would you like to use local host as a bootnode ?", default_bootnode
				).expect_confirm("Should the genesis state file be generated ?", genesis_state
				).expect_confirm("Should the genesis code file be generated ?", genesis_code)
				.expect_confirm("Would you like to build the runtime deterministically? This requires a containerization solution (Docker/Podman) and is recommended for production builds.", deterministic)
				.expect_input("Enter the directory path where the runtime is located:", runtime_dir.display().to_string())
				.expect_input("Enter the runtime package name:", package.to_string());
			}
			let build_spec = build_spec_cmd.configure_build_spec(&mut cli).await?;
			assert_eq!(build_spec.chain, chain);
			assert_eq!(build_spec.output_file, PathBuf::from(output_file));
			assert_eq!(build_spec.id, para_id);
			assert_eq!(build_spec.profile, profile);
			assert_eq!(build_spec.default_bootnode, default_bootnode);
			assert_eq!(build_spec.chain_type, chain_type);
			assert_eq!(build_spec.relay, relay);
			assert_eq!(build_spec.protocol_id, protocol_id);
			assert_eq!(build_spec.genesis_state, genesis_state);
			assert_eq!(build_spec.genesis_code, genesis_code);
			assert_eq!(build_spec.deterministic, deterministic);
			assert_eq!(build_spec.package, package);
			assert_eq!(build_spec.runtime_dir, runtime_dir);
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
		let protocol_id = "pop";
		let relay = Polkadot;
		let profile = Profile::Production;
		let deterministic = true;
		let package = "runtime-name";
		let runtime_dir = PathBuf::from("./new-runtime-dir");

		// Create a temporary file to act as the existing chain spec file.
		let temp_dir = tempdir()?;
		let chain_spec_path = temp_dir.path().join("existing-chain-spec.json");
		std::fs::write(&chain_spec_path, "{}")?; // Write a dummy JSON to the file.

		// Whether to make changes to the provided chain spec file.
		for changes in [true, false] {
			for build_spec_cmd in [
				// No flags used except the provided chain spec file.
				BuildSpecCommand {
					chain: Some(chain_spec_path.to_string_lossy().to_string()),
					..Default::default()
				},
				// All flags used.
				BuildSpecCommand {
					output_file: Some(PathBuf::from(output_file)),
					profile: Some(profile.clone()),
					id: Some(para_id),
					default_bootnode,
					chain_type: Some(chain_type.clone()),
					chain: Some(chain_spec_path.to_string_lossy().to_string()),
					relay: Some(relay.clone()),
					protocol_id: Some(protocol_id.to_string()),
					genesis_state,
					genesis_code,
					deterministic,
					skip_deterministic_build: false,
					package: Some(package.to_string()),
					runtime_dir: Some(runtime_dir.clone()),
				},
			] {
				let mut cli = MockCli::new().expect_confirm(
					"An existing chain spec file is provided. Do you want to make additional changes to it?",
					changes,
				);
				// When user wants to make changes to chain spec file via prompts and no flags
				// provided.
				let no_flags_used = build_spec_cmd.relay.is_none();
				if changes && no_flags_used {
					if build_spec_cmd.id.is_none() {
						cli = cli
							.expect_input("What parachain ID should be used?", para_id.to_string());
					}
					if build_spec_cmd.protocol_id.is_none() {
						cli = cli.expect_input(
							"Enter the protocol ID that will identify your network:",
							protocol_id.to_string(),
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
					if build_spec_cmd.relay.is_none() {
						cli = cli.expect_select(
							"Choose the relay your chain will be connecting to: ",
							Some(false),
							true,
							Some(relays()),
							relay.clone() as usize,
							None,
						);
					}
					if build_spec_cmd.profile.is_none() {
						cli = cli.expect_select(
							"Choose the build profile of the binary that should be used: ",
							Some(false),
							true,
							Some(profiles()),
							profile.clone() as usize,
							None,
						);
					}
					if !build_spec_cmd.default_bootnode {
						cli = cli.expect_confirm(
							"Would you like to use local host as a bootnode ?",
							default_bootnode,
						);
					}
					if !build_spec_cmd.genesis_state {
						cli = cli.expect_confirm(
							"Should the genesis state file be generated ?",
							genesis_state,
						);
					}
					if !build_spec_cmd.genesis_code {
						cli = cli.expect_confirm(
							"Should the genesis code file be generated ?",
							genesis_code,
						);
					}
					if !build_spec_cmd.deterministic {
						cli = cli.expect_confirm(
							"Would you like to build the runtime deterministically? This requires a containerization solution (Docker/Podman) and is recommended for production builds.",
							deterministic,
						).expect_input("Enter the directory path where the runtime is located:", runtime_dir.display().to_string())
						.expect_input("Enter the runtime package name:", package.to_string());
					}
				} else if !changes && no_flags_used {
					if !build_spec_cmd.genesis_state {
						cli = cli.expect_confirm(
							"Should the genesis state file be generated ?",
							genesis_state,
						);
					}
					if !build_spec_cmd.genesis_code {
						cli = cli.expect_confirm(
							"Should the genesis code file be generated ?",
							genesis_code,
						);
					}
				}
				let build_spec = build_spec_cmd.configure_build_spec(&mut cli).await?;
				if !changes && no_flags_used {
					assert_eq!(build_spec.id, 2000);
					assert_eq!(build_spec.chain_type, Development);
					assert_eq!(build_spec.relay, PaseoLocal);
					assert_eq!(build_spec.protocol_id, "my-protocol");
					assert_eq!(build_spec.genesis_state, genesis_state);
					assert_eq!(build_spec.genesis_code, genesis_code);
					assert_eq!(build_spec.deterministic, false);
					assert_eq!(build_spec.package, DEFAULT_PACKAGE);
					assert_eq!(build_spec.runtime_dir, PathBuf::from(DEFAULT_RUNTIME_DIR));
				} else if changes && no_flags_used {
					assert_eq!(build_spec.id, para_id);
					assert_eq!(build_spec.profile, profile);
					assert_eq!(build_spec.default_bootnode, default_bootnode);
					assert_eq!(build_spec.chain_type, chain_type);
					assert_eq!(build_spec.relay, relay);
					assert_eq!(build_spec.protocol_id, protocol_id);
					assert_eq!(build_spec.genesis_state, genesis_state);
					assert_eq!(build_spec.genesis_code, genesis_code);
					assert_eq!(build_spec.deterministic, deterministic);
					assert_eq!(build_spec.package, package);
					assert_eq!(build_spec.runtime_dir, runtime_dir);
				}
				// Assert that the chain spec file is correctly detected and used.
				assert_eq!(build_spec.chain, chain_spec_path.to_string_lossy());
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
		std::fs::write(
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
		std::fs::write(
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
			serde_json::from_str(&fs::read_to_string(&build_spec.chain)?)?;

		assert_eq!(build_spec.chain, output_file.display().to_string());
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
		std::fs::write(&genesis_state_path, "0x1234")?;
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
		std::fs::write(&genesis_code_path, "0x1234")?;
		assert_eq!(artifacts.read_genesis_code()?, "0x1234");

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
