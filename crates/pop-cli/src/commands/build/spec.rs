// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	cli::{traits::Cli as _, Cli},
	style::style,
};
use clap::{Args, ValueEnum};
use cliclack::{confirm, input};
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

#[derive(Args)]
pub struct BuildSpecCommand {
	/// File name for the resulting spec. If a path is given,
	/// the necessary directories will be created
	/// [default: ./chain-spec.json].
	#[arg(short = 'o', long = "output")]
	pub(crate) output_file: Option<PathBuf>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short = 'r', long, default_value = "true")]
	pub(crate) release: bool,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long = "id")]
	pub(crate) id: Option<u32>,
	/// Whether to keep localhost as a bootnode.
	#[clap(long, default_value = "true")]
	pub(crate) default_bootnode: bool,
	/// Type of the chain [default: development].
	#[arg(short = 't', long = "type", value_enum)]
	pub(crate) chain_type: Option<ChainType>,
	/// Relay chain this parachain will connect to [default: paseo-local].
	#[arg(long, value_enum)]
	pub(crate) relay: Option<RelayChain>,
	/// Protocol-id to use in the specification.
	#[arg(long = "protocol-id")]
	pub(crate) protocol_id: Option<String>,
	/// Whether the genesis state file should be generated [default: true].
	#[clap(long = "genesis-state", default_value = "true")]
	pub(crate) genesis_state: bool,
	/// Whether the genesis code file should be generated [default: true].
	#[clap(long = "genesis-code", default_value = "true")]
	pub(crate) genesis_code: bool,
}

impl BuildSpecCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<&'static str> {
		// Checks for appchain project in `./`.
		if is_supported(None)? {
			// If para id has been provided we can build the spec
			// otherwise, we need to guide the user.
			let _ = match self.id {
				Some(_) => self.build(&mut Cli),
				None => {
					let config = guide_user_to_generate_spec(self).await?;
					config.build(&mut Cli)
				},
			};
			Ok("spec")
		} else {
			Cli.intro("Building your chain spec")?;
			Cli.outro_cancel(
				"🚫 Can't build a specification for target. Maybe not a chain project ?",
			)?;
			Ok("spec")
		}
	}

	/// Builds a parachain spec.
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Building your chain spec")?;

		// Either a para id was already provided or user has been guided to provide one.
		let para_id = self.id.unwrap_or(DEFAULT_PARA_ID);
		// Notify user in case we need to build the parachain project.
		if !self.release {
			cli.warning("NOTE: this command defaults to DEBUG builds for development chain types. Please use `--release` (or simply `-r` for a release build...)")?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3))
		}

		let spinner = cliclack::spinner();
		spinner.start("Generating chain specification...");

		// Create output path if needed
		let mut output_path = self.output_file.unwrap_or_else(|| PathBuf::from("./"));
		let mut plain_chain_spec;
		if output_path.is_dir() {
			if !output_path.exists() {
				// Generate the output path if needed
				create_dir_all(&output_path)?;
			}
			plain_chain_spec = output_path.join(DEFAULT_SPEC_NAME);
		} else {
			plain_chain_spec = output_path.clone();
			output_path.pop();
			if !output_path.exists() {
				// Generate the output path if needed
				create_dir_all(&output_path)?;
			}
		}
		plain_chain_spec.set_extension("json");

		// Locate binary, if it doesn't exist trigger build.
		let mode: Profile = self.release.into();
		let cwd = current_dir().unwrap_or(PathBuf::from("./"));
		let binary_path = match binary_path(&mode.target_directory(&cwd), &cwd.join("node")) {
			Ok(binary_path) => binary_path,
			_ => {
				cli.info("Node was not found. The project will be built locally.".to_string())?;
				cli.warning("NOTE: this may take some time...")?;
				build_parachain(&cwd, None, &mode, None)?
			},
		};

		// Generate plain spec.
		spinner.set_message("Generating plain chain specification...");
		let mut generated_files = vec![];
		generate_plain_chain_spec(&binary_path, &plain_chain_spec, self.default_bootnode)?;
		generated_files.push(format!(
			"Plain text chain specification file generated at: {}",
			plain_chain_spec.display()
		));

		// Customize spec based on input.
		let mut chain_spec = ChainSpec::from(&plain_chain_spec)?;
		chain_spec.replace_para_id(para_id)?;
		let relay = self.relay.unwrap_or(RelayChain::PaseoLocal).to_string();
		chain_spec.replace_relay_chain(&relay)?;
		let chain_type = self.chain_type.unwrap_or(ChainType::Development).to_string();
		chain_spec.replace_chain_type(&chain_type)?;
		if self.protocol_id.is_some() {
			let protocol_id = self.protocol_id.unwrap_or(DEFAULT_PROTOCOL_ID.to_string());
			chain_spec.replace_protocol_id(&protocol_id)?;
		}
		chain_spec.to_file(&plain_chain_spec)?;

		// Generate raw spec.
		spinner.set_message("Generating raw chain specification...");
		let spec_name = plain_chain_spec
			.file_name()
			.and_then(|s| s.to_str())
			.unwrap_or(DEFAULT_SPEC_NAME)
			.trim_end_matches(".json");
		let raw_spec_name = format!("{spec_name}-raw.json");
		let raw_chain_spec =
			generate_raw_chain_spec(&binary_path, &plain_chain_spec, &raw_spec_name)?;
		generated_files.push(format!(
			"Raw chain specification file generated at: {}",
			raw_chain_spec.display()
		));

		// Generate genesis artifacts.
		if self.genesis_code {
			spinner.set_message("Generating genesis code...");
			let wasm_file_name = format!("para-{}.wasm", para_id);
			let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, &wasm_file_name)?;
			generated_files
				.push(format!("WebAssembly runtime file exported at: {}", wasm_file.display()));
		}

		if self.genesis_state {
			spinner.set_message("Generating genesis state...");
			let genesis_file_name = format!("para-{}-genesis-state", para_id);
			let genesis_state_file =
				generate_genesis_state_file(&binary_path, &raw_chain_spec, &genesis_file_name)?;
			generated_files
				.push(format!("Genesis State file exported at: {}", genesis_state_file.display()));
		}

		cli.intro("Building your chain spec".to_string())?;
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
}

/// Guide the user to generate their chain specification.
async fn guide_user_to_generate_spec(args: BuildSpecCommand) -> anyhow::Result<BuildSpecCommand> {
	Cli.intro("Generate your chain spec")?;

	// Confirm output path
	let default_output = format!("./{DEFAULT_SPEC_NAME}");
	let output_file: String = input("Name of the plain chain spec file. If a path is given, the necessary directories will be created:")
		.placeholder(&default_output)
		.default_input(&default_output)
		.interact()?;

	// Check if specified chain spec already exists, allowing us to default values for prompts
	let path = Path::new(&output_file);
	let chain_spec =
		(path.is_file() && path.exists()).then(|| ChainSpec::from(path).ok()).flatten();

	// Prompt for chain id.
	let default = chain_spec
		.as_ref()
		.and_then(|cs| cs.get_parachain_id())
		.unwrap_or(DEFAULT_PARA_ID as u64)
		.to_string();
	let para_id: u32 = input("What parachain ID should be used?")
		.placeholder(&default)
		.default_input(&default)
		.interact()?;

	// Prompt for chain type.
	// If relay is Kusama or Polkadot, then Live type is used and user is not prompted.
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
	let chain_type: ChainType = prompt.interact()?.clone();

	// Prompt for relay chain.
	let mut prompt =
		cliclack::select("Choose the relay chain your chain will be connecting to: ".to_string());
	let default = chain_spec
		.as_ref()
		.and_then(|cs| cs.get_relay_chain())
		.and_then(|r| RelayChain::from_str(r, true).ok());
	if let Some(relay) = default.as_ref() {
		prompt = prompt.initial_value(relay);
	}
	// Prompt relays chains based on the chain type
	match chain_type {
		ChainType::Live =>
			for relay in RelayChain::VARIANTS {
				if !matches!(
					relay,
					RelayChain::Westend |
						RelayChain::Paseo | RelayChain::Kusama |
						RelayChain::Polkadot
				) {
					continue;
				} else {
					prompt = prompt.item(
						relay,
						relay.get_message().unwrap_or(relay.as_ref()),
						relay.get_detailed_message().unwrap_or_default(),
					);
				}
			},
		_ =>
			for relay in RelayChain::VARIANTS {
				if matches!(
					relay,
					RelayChain::Westend |
						RelayChain::Paseo | RelayChain::Kusama |
						RelayChain::Polkadot
				) {
					continue;
				} else {
					prompt = prompt.item(
						relay,
						relay.get_message().unwrap_or(relay.as_ref()),
						relay.get_detailed_message().unwrap_or_default(),
					);
				}
			},
	}

	let relay_chain = prompt.interact()?.clone();

	// Prompt for default bootnode if chain type is Local or Live.
	let default_bootnode = match chain_type {
		ChainType::Development => true,
		_ => confirm("Would you like to use local host as a bootnode ?".to_string()).interact()?,
	};

	// Prompt for protocol-id.
	let default = chain_spec
		.as_ref()
		.and_then(|cs| cs.get_protocol_id())
		.unwrap_or(DEFAULT_PROTOCOL_ID)
		.to_string();
	let protocol_id: String = input("Enter the protocol ID that will identify your network:")
		.placeholder(&default)
		.default_input(&default)
		.interact()?;

	// Prompt for genesis state
	let genesis_state = confirm("Should the genesis state file be generated ?".to_string())
		.initial_value(true)
		.interact()?;

	// Prompt for genesis code
	let genesis_code = confirm("Should the genesis code file be generated ?".to_string())
		.initial_value(true)
		.interact()?;

	// Only check user to check their profile selection if a live spec is being built on debug mode.
	let profile =
		if !args.release && matches!(chain_type, ChainType::Live) {
			confirm("Using Debug profile to build a Live specification. Should Release be used instead ?")
    		.initial_value(true)
    		.interact()?
		} else {
			args.release
		};

	Ok(BuildSpecCommand {
		output_file: Some(PathBuf::from(output_file)),
		release: profile,
		id: Some(para_id),
		default_bootnode,
		chain_type: Some(chain_type),
		relay: Some(relay_chain),
		protocol_id: Some(protocol_id),
		genesis_state,
		genesis_code,
	})
}
