// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	cli::{traits::Cli as _, Cli},
	style::style,
};
use clap::{Args, ValueEnum};
use cliclack::{confirm, input, spinner};
use pop_common::Profile;
use pop_parachains::{
	binary_path, build_parachain, export_wasm_file, generate_genesis_state_file,
	generate_plain_chain_spec, generate_raw_chain_spec, is_supported, ChainSpec,
};
use std::{env::current_dir, fs::create_dir_all, path::PathBuf};
#[cfg(not(test))]
use std::{thread::sleep, time::Duration};
use strum::{EnumMessage, VariantArray};
use strum_macros::{AsRefStr, Display, EnumString};

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
#[derive(Args)]
pub struct BuildSpecCommand {
	/// File name for the resulting spec. If a path is given,
	/// the necessary directories will be created
	#[arg(short = 'o', long = "output")]
	pub(crate) output_file: Option<PathBuf>,
	/// This command builds the node if it has not been built already.
	/// For production, always build in release mode to exclude debug features.
	#[arg(short = 'r', long)]
	pub(crate) release: bool,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long)]
	pub(crate) id: Option<u32>,
	/// Whether to keep localhost as a bootnode.
	#[arg(long)]
	pub(crate) default_bootnode: bool,
	/// Type of the chain.
	#[arg(short = 't', long = "type", value_enum)]
	pub(crate) chain_type: Option<ChainType>,
	/// Specify the chain specification. It can be one of the predefined ones (e.g. dev, local or a
	/// custom one) or the path to an existing chain spec.
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
		// Checks for appchain project in `./`.
		if is_supported(None)? {
			let build_spec = self.complete_build_spec(&mut cli).await?;
			build_spec.build(&mut cli)
		} else {
			cli.intro("Generate your chain spec")?;
			cli.outro_cancel(
				"🚫 Can't build a specification for target. Maybe not a chain project ?",
			)?;
			Ok("spec")
		}
	}

	/// Completes chain specification requirements by prompting for missing inputs, validating
	/// provided values, and preparing a BuildSpec for generating chain spec files.
	async fn complete_build_spec(
		self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<BuildSpec> {
		cli.intro("Generate your chain spec")?;

		let BuildSpecCommand {
			output_file,
			release,
			id,
			default_bootnode,
			chain_type,
			chain,
			relay,
			protocol_id,
			genesis_state,
			genesis_code,
		} = self;

		// Prompt for chain specification.
		let chain = match chain {
			Some(chain) => chain,
			_ => {
				input("Specify the chain specification. It can be one of the predefined ones (e.g. dev, local or a custom one) or the path to an existing chain spec.")
					.placeholder("dev")
					.default_input("dev")
					.interact()?
			},
		};

		// Check if the provided chain specification is a file.
		let maybe_chain_spec_file = PathBuf::from(&chain);
		let output_file = if maybe_chain_spec_file.exists() && maybe_chain_spec_file.is_file() {
			if output_file.is_some() {
				cli.warning("NOTE: If an existing chain spec file is provided it will be used for the output path.")?;
			}
			// Set the provided chain specification file as output file.
			maybe_chain_spec_file
		} else {
			let output_file = match output_file {
				Some(output) => output,
				None => {
					// Prompt for output file if not provided.
					let default_output = format!("./{DEFAULT_SPEC_NAME}");
					input("Name of the plain chain spec file. If a path is given, the necessary directories will be created:")
						.placeholder(&default_output)
						.default_input(&default_output)
						.interact()?
				},
			};
			prepare_output_path(output_file)?
		};
		// If chain specification file already exists, obtain values for defaults when prompting.
		let chain_spec = ChainSpec::from(&output_file).ok();

		// Prompt for para id if not provided.
		let id = match id {
			Some(id) => id,
			None => {
				let default_id = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_parachain_id())
					.unwrap_or(DEFAULT_PARA_ID as u64)
					.to_string();
				input("What parachain ID should be used?")
					.placeholder(&default_id)
					.default_input(&default_id)
					.interact()?
			},
		};

		// Prompt for chain type if not provided.
		let chain_type = match chain_type {
			Some(chain_type) => chain_type,
			None => {
				let mut prompt = cliclack::select("Choose the chain type: ".to_string());
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_chain_type())
					.and_then(|r| ChainType::from_str(r, true).ok());
				if let Some(chain_type) = default.as_ref() {
					prompt = prompt.initial_value(chain_type);
				}
				for (i, chain_type) in ChainType::VARIANTS.iter().enumerate() {
					if default.is_none() && i == 0 {
						prompt = prompt.initial_value(chain_type);
					}
					prompt = prompt.item(
						chain_type,
						chain_type.get_message().unwrap_or(chain_type.as_ref()),
						chain_type.get_detailed_message().unwrap_or_default(),
					);
				}
				prompt.interact()?.clone()
			},
		};

		// Prompt for relay chain if not provided.
		let relay = match relay {
			Some(relay) => relay,
			None => {
				let mut prompt = cliclack::select(
					"Choose the relay chain your chain will be connecting to: ".to_string(),
				);
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_relay_chain())
					.and_then(|r| RelayChain::from_str(r, true).ok());
				if let Some(relay) = default.as_ref() {
					prompt = prompt.initial_value(relay);
				}
				for relay in RelayChain::VARIANTS {
					prompt = prompt.item(
						relay,
						relay.get_message().unwrap_or(relay.as_ref()),
						relay.get_detailed_message().unwrap_or_default(),
					);
				}
				prompt.interact()?.clone()
			},
		};

		// Prompt for default bootnode if not provided and chain type is Local or Live.
		let default_bootnode = if !default_bootnode {
			match chain_type {
				ChainType::Development => true,
				_ => confirm("Would you like to use local host as a bootnode ?".to_string())
					.interact()?,
			}
		} else {
			true
		};

		// Prompt for protocol-id if not provided.
		let protocol_id = match protocol_id {
			Some(protocol_id) => protocol_id,
			None => {
				let default = chain_spec
					.as_ref()
					.and_then(|cs| cs.get_protocol_id())
					.unwrap_or(DEFAULT_PROTOCOL_ID)
					.to_string();
				input("Enter the protocol ID that will identify your network:")
					.placeholder(&default)
					.default_input(&default)
					.interact()?
			},
		};

		// Prompt for genesis state if not provided.
		let genesis_state = if !genesis_state {
			confirm("Should the genesis state file be generated ?".to_string())
				.initial_value(true)
				.interact()?
		} else {
			true
		};

		// Prompt for genesis code if not provided.
		let genesis_code = if !genesis_code {
			confirm("Should the genesis code file be generated ?".to_string())
				.initial_value(true)
				.interact()?
		} else {
			true
		};

		// Only prompt user for build profile if a live spec is being built on debug mode.
		let release = if !release && matches!(chain_type, ChainType::Live) {
			confirm("Using Debug profile to build a Live specification. Should Release be used instead ?")
				.initial_value(true)
				.interact()?
		} else {
			release
		};

		Ok(BuildSpec {
			output_file,
			release,
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
struct BuildSpec {
	output_file: PathBuf,
	release: bool,
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
	fn build(&self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Building your chain spec")?;
		let spinner = spinner();
		spinner.start("Generating chain specification...");
		let mut generated_files = vec![];

		// Ensure binary is built.
		let mode: Profile = self.release.into();
		let binary_path = ensure_binary_exists(cli, &mode)?;
		if !self.release {
			cli.warning(
				"NOTE: this command defaults to DEBUG builds for development chain types. Please use `--release` (or simply `-r` for a release build...)",
			)?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3))
		}

		// Generate chain spec.
		generate_plain_chain_spec(
			&binary_path,
			&self.output_file,
			self.default_bootnode,
			&self.chain,
		)?;
		// Customize spec based on input.
		self.customize()?;
		generated_files.push(format!(
			"Plain text chain specification file generated at: {}",
			self.output_file.display()
		));

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
			generate_raw_chain_spec(&binary_path, &self.output_file, &raw_spec_name)?;
		generated_files.push(format!(
			"Raw chain specification file generated at: {}",
			raw_chain_spec.display()
		));

		// Generate genesis artifacts.
		if self.genesis_code {
			spinner.set_message("Generating genesis code...");
			let wasm_file_name = format!("para-{}.wasm", self.id);
			let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, &wasm_file_name)?;
			generated_files
				.push(format!("WebAssembly runtime file exported at: {}", wasm_file.display()));
		}
		if self.genesis_state {
			spinner.set_message("Generating genesis state...");
			let genesis_file_name = format!("para-{}-genesis-state", self.id);
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
fn prepare_output_path(mut output_path: PathBuf) -> anyhow::Result<PathBuf> {
	if output_path.is_dir() {
		if !output_path.exists() {
			create_dir_all(&output_path)?;
		}
		output_path.push(DEFAULT_SPEC_NAME);
	} else {
		if let Some(parent_dir) = output_path.parent() {
			if !parent_dir.exists() {
				create_dir_all(&output_path)?;
			}
		}
	}
	output_path.set_extension("json");
	Ok(output_path)
}
