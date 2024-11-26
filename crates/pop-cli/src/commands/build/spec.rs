// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	cli::{
		traits::{Cli as _, *},
		Cli,
	},
	style::style,
};
use clap::{Args, ValueEnum};
use cliclack::spinner;
use pop_common::Profile;
use pop_parachains::{
	binary_path, build_parachain, export_wasm_file, generate_genesis_state_file,
	generate_plain_chain_spec, generate_raw_chain_spec, is_supported, ChainSpec,
};
use std::{
	env::current_dir,
	fs::create_dir_all,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, VariantArray};
use strum_macros::{AsRefStr, Display, EnumString};

const DEFAULT_CHAIN: &str = "dev";
const DEFAULT_PARA_ID: u32 = 2000;
const DEFAULT_PROTOCOL_ID: &str = "my-protocol";
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
	#[arg(short = 'o', long = "output")]
	pub(crate) output_file: Option<PathBuf>,
	/// Build profile for the binary to generate the chain specification.
	#[arg(short = 'r', long, value_enum)]
	pub(crate) profile: Option<Profile>,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long)]
	pub(crate) id: Option<u32>,
	/// Whether to keep localhost as a bootnode.
	#[arg(long)]
	pub(crate) default_bootnode: bool,
	/// Type of the chain.
	#[arg(short = 't', long = "type", value_enum)]
	pub(crate) chain_type: Option<ChainType>,
	/// Provide the chain specification to use (e.g. dev, local, custom or a path to an existing
	/// file).
	#[arg(short = 'c', long = "chain")]
	pub(crate) chain: Option<String>,
	/// Relay chain this parachain will connect to.
	#[arg(long, value_enum)]
	pub(crate) relay: Option<RelayChain>,
	/// Protocol-id to use in the specification.
	#[arg(long = "protocol-id")]
	pub(crate) protocol_id: Option<String>,
	/// Whether the genesis state file should be generated.
	#[arg(long = "genesis-state")]
	pub(crate) genesis_state: bool,
	/// Whether the genesis code file should be generated.
	#[arg(long = "genesis-code")]
	pub(crate) genesis_code: bool,
}

impl BuildSpecCommand {
	/// Executes the build spec command.
	pub(crate) async fn execute(self) -> anyhow::Result<&'static str> {
		let mut cli = Cli;
		cli.intro("Generate your chain spec")?;
		// Checks for appchain project in `./`.
		if is_supported(None)? {
			let build_spec = self.configure_build_spec(&mut cli).await?;
			build_spec.build(&mut cli)
		} else {
			cli.outro_cancel(
				"🚫 Can't build a specification for target. Maybe not a chain project ?",
			)?;
			Ok("spec")
		}
	}

	/// Configure chain specification requirements by prompting for missing inputs, validating
	/// provided values, and preparing a BuildSpec to generate file(s).
	///
	/// # Arguments
	/// * `cli` - The cli.
	async fn configure_build_spec(
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

		// Prompt user for build profile.
		let profile = match profile {
			Some(profile) => profile,
			None => {
				let default = Profile::Release;
				if prompt {
					// Prompt for build profile.
					let mut prompt = cliclack::select(
						"Choose the build profile of the binary that should be used: ".to_string(),
					).initial_value(&default);
					for profile in Profile::VARIANTS {
						prompt = prompt.item(
							profile,
							profile.get_message().unwrap_or(profile.as_ref()),
							profile.get_detailed_message().unwrap_or_default(),
						);
					}
					prompt.interact()?.clone()
				} else { default }
			},
		};

		// Prompt for default bootnode if not provided and chain type is Local or Live.
		let default_bootnode = if !default_bootnode {
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
		})
	}
}

// Represents the configuration for building a chain specification.
#[derive(Debug)]
struct BuildSpec {
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
}

impl BuildSpec {
	// Executes the process of generating the chain specification.
	//
	// This function generates plain and raw chain spec files based on the provided configuration,
	// optionally including genesis state and runtime artifacts. If the node binary is missing,
	// it triggers a build process.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Building your chain spec")?;
		let mut generated_files = vec![];

		let BuildSpec {
			ref output_file, ref profile, id, default_bootnode, ref chain, genesis_state, genesis_code,
		.. } = self;

		// Ensure binary is built.
		let binary_path = ensure_binary_exists(cli, &profile)?;
		let spinner = spinner();
		spinner.start("Generating chain specification...");

		// Generate chain spec.
		generate_plain_chain_spec(
			&binary_path,
			&output_file,
			default_bootnode,
			&chain,
		)?;
		// Customize spec based on input.
		self.customize()?;
		generated_files.push(format!(
			"Plain text chain specification file generated at: {}",
			&output_file.display()
		));

		// Generate raw spec.
		spinner.set_message("Generating raw chain specification...");
		let spec_name = &output_file
			.file_name()
			.and_then(|s| s.to_str())
			.unwrap_or(DEFAULT_SPEC_NAME)
			.trim_end_matches(".json");
		let raw_spec_name = format!("{spec_name}-raw.json");
		let raw_chain_spec =
			generate_raw_chain_spec(&binary_path, &output_file, &raw_spec_name)?;
		generated_files.push(format!(
			"Raw chain specification file generated at: {}",
			raw_chain_spec.display()
		));

		// Generate genesis artifacts.
		if genesis_code {
			spinner.set_message("Generating genesis code...");
			let wasm_file_name = format!("para-{}.wasm", id);
			let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, &wasm_file_name)?;
			generated_files
				.push(format!("WebAssembly runtime file exported at: {}", wasm_file.display()));
		}
		if genesis_state {
			spinner.set_message("Generating genesis state...");
			let genesis_file_name = format!("para-{}-genesis-state", id);
			let genesis_state_file =
				generate_genesis_state_file(&binary_path, &raw_chain_spec, &genesis_file_name)?;
			generated_files
				.push(format!("Genesis State file exported at: {}", genesis_state_file.display()));
		}

		spinner.stop("Chain specification built successfully.");
		let generated_files: Vec<_> = generated_files
			.iter()
			.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
			.collect();
		cli.success(format!("Generated files:\n{}", generated_files.join("\n")))?;
		cli.outro(format!(
			"Need help? Learn more at {}\n",
			style("https://learn.onpop.io").magenta().underlined()
		))?;
		Ok("spec")
	}

	// Customize a chain specification.
	fn customize(&self) -> anyhow::Result<()> {
		let mut chain_spec = ChainSpec::from(&self.output_file)?;
		chain_spec.replace_para_id(self.id)?;
		chain_spec.replace_relay_chain(&self.relay.to_string())?;
		chain_spec.replace_chain_type(&self.chain_type.to_string())?;
		chain_spec.replace_protocol_id(&self.protocol_id)?;
		chain_spec.to_file(&self.output_file)?;
		Ok(())
	}
}

// Locate binary, if it doesn't exist trigger build.
fn ensure_binary_exists(
	cli: &mut impl cli::traits::Cli,
	mode: &Profile,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	match binary_path(&mode.target_directory(&cwd), &cwd.join("node")) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Node was not found. The project will be built locally.".to_string())?;
			cli.warning("NOTE: this may take some time...")?;
			build_parachain(&cwd, None, mode, None).map_err(|e| e.into())
		},
	}
}

// Prepare the output path provided.
fn prepare_output_path(output_path: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
	let mut output_path = output_path.as_ref().to_path_buf();
	// Convert to string to check for trailing slash.
	let output_path_str = output_path.to_string_lossy();
	let ends_with_slash = output_path_str.ends_with(std::path::MAIN_SEPARATOR);
	// Check if the path has an extension.
	let has_extension = output_path.extension().is_some();
	// Check if the path ends with '.json'
	let is_json_file = output_path
		.extension()
		.and_then(|ext| ext.to_str())
		.map(|ext| ext.eq_ignore_ascii_case("json"))
		.unwrap_or(false);
	if ends_with_slash {
		// Treat as directory.
		if !output_path.exists() {
			create_dir_all(&output_path)?;
		}
		output_path.push(DEFAULT_SPEC_NAME);
	} else if !has_extension {
		// No extension, treat as file, set extension to '.json'.
		output_path.set_extension("json");
	} else if !is_json_file {
		// Has an extension but not '.json', change extension to '.json'.
		output_path.set_extension("json");
	}
	// Else: Ends with '.json', treat as file (no changes needed).

	// After modifications, create the parent directory if it doesn't exist.
	if let Some(parent_dir) = output_path.parent() {
		if !parent_dir.exists() {
			create_dir_all(parent_dir)?;
		}
	}
	Ok(output_path)
}
