// SPDX-License-Identifier: GPL-3.0

use crate::{cli, cli::traits::Cli as _, cli::Cli, style::style};
use clap::{Args, ValueEnum};
use cliclack::{confirm, input};
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_parachain, export_wasm_file, generate_genesis_state_file, generate_plain_chain_spec,
	generate_raw_chain_spec, replace_chain_type, replace_relay_spec,
};
use std::fs::create_dir_all;
use std::path::PathBuf;
#[cfg(not(test))]
use std::{thread::sleep, time::Duration};
use strum::VariantArray;

const PLAIN_CHAIN_SPEC_FILE_NAME: &str = "plain-parachain-chainspec.json";
const RAW_CHAIN_SPEC_FILE_NAME: &str = "raw-parachain-chainspec.json";

#[derive(Clone, ValueEnum, Default, VariantArray, Eq, PartialEq)]
/// Supported chain types for the resulting chain spec.
pub(crate) enum ChainType {
	// A development chain that runs mainly on one node.
	#[default]
	Development,
	// A local chain that runs locally on multiple nodes for testing purposes.
	Local,
	// A live chain.
	Live,
}

impl ChainType {
	fn into_str(self) -> &'static str {
		match self {
			ChainType::Development => "Development",
			ChainType::Local => "Local",
			ChainType::Live => "Live",
		}
	}
}

#[derive(Clone, ValueEnum, Default, VariantArray, Eq, PartialEq)]
/// Supported relay chains that can be included in the resulting chain spec.
pub(crate) enum RelayChain {
	Kusama,
	KusamaLocal,
	Rococo,
	RococoLocal,
	Paseo,
	#[default]
	PaseoLocal,
	Polkadot,
	PolkadotLocal,
}

impl RelayChain {
	fn into_str(self) -> &'static str {
		match self {
			RelayChain::Kusama => "kusama",
			RelayChain::KusamaLocal => "kusama-local",
			RelayChain::Rococo => "rococo",
			RelayChain::RococoLocal => "rococo-local",
			RelayChain::Paseo => "paseo",
			RelayChain::PaseoLocal => "paseo-local",
			RelayChain::Polkadot => "polkadot",
			RelayChain::PolkadotLocal => "polkadot-local",
		}
	}
}

#[derive(Args)]
pub struct BuildSpecCommand {
	/// Directory path for your project [default: current directory].
	#[arg(long)]
	pub(crate) path: Option<PathBuf>,
	/// For production, always build in release mode to exclude debug features.
	#[clap(short, long, default_value = "true")]
	pub(crate) release: bool,
	/// Parachain ID to be used when generating the chain spec files.
	#[arg(short = 'i', long = "id")]
	pub(crate) id: Option<u32>,
	/// Whether to keep localhost as a bootnode.
	#[clap(long, default_value = "true")]
	pub(crate) default_bootnode: bool,
	/// Type of the chain [default: Development].
	#[arg(short = 't', long = "type", value_enum)]
	pub(crate) chain_type: Option<ChainType>,
	/// Relay chain this parachain will connect to [default: PaseoLocal].
	#[arg(long, value_enum)]
	pub(crate) relay: Option<RelayChain>,
	// Deprecation flag, used to specify whether the deprecation warning is shown.
	#[clap(skip)]
	pub(crate) valid: bool,
}

impl BuildSpecCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<&'static str> {
		// If para id has been provided we can build the spec
		// otherwise, we need to guide the user.
		let _ = match self.id {
			Some(_) => self.build(&mut cli::Cli),
			None => {
				let config = guide_user_to_generate_spec().await?;
				config.build(&mut cli::Cli)
			},
		};
		Ok("spec")
	}

	/// Builds a parachain spec.
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro(format!("Building your chain spec"))?;

		// Either a para id was already provided or user has been guided to provide one.
		let para_id = self.id.unwrap_or(2000);

		// Show warning if specified as deprecated.
		if !self.valid {
			cli.warning("NOTE: this command is deprecated. Please use `pop build spec` (or simply `pop b s`) in future...")?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(3))
		} else {
			// Notify user in case we need to build the parachain project.
			if !self.release {
				cli.warning("NOTE: this command now defaults to DEBUG builds.")?;
				#[cfg(not(test))]
				sleep(Duration::from_secs(3))
			}
		}

		// Locate binary, if it doesn't exist trigger build.
		let project_path = self.path.unwrap_or_else(|| PathBuf::from("./"));
		let mode: Profile = self.release.into();
		cli.info(format!("Locating the project binary..."))?;
		let binary_path = match maybe_node_binary_path(&project_path, &mode) {
			Some(binary_path) => {
				cli.info(format!("Using {} to build the chain spec.", binary_path.display()))?;
				binary_path
			},
			None => {
				cli.info(format!("The binary was not found. The project will be built locally."))?;
				cli.warning("NOTE: this may take some time...")?;
				build_parachain(&project_path, None, &mode, None).unwrap()
			},
		};

		// Create output dir.
		let mut output_path = project_path.clone();
		output_path.push("target/pop");
		create_dir_all(&output_path)?;

		// Generate spec and artifacts
		let mut generated_files =
			vec![format!("Specification and artifacts generated at: {}", &output_path.display())];

		let plain_chain_spec = output_path.join(PLAIN_CHAIN_SPEC_FILE_NAME);
		generate_plain_chain_spec(&binary_path, &plain_chain_spec, para_id, self.default_bootnode)?;
		generated_files.push(format!(
			"Plain text chain specification file generated at: {}",
			plain_chain_spec.display()
		));

		// Customize spec based on input.
		let relay = self.relay.unwrap_or(RelayChain::PaseoLocal).into_str();
		let _ = replace_relay_spec(&plain_chain_spec, relay, "rococo-local");
		let chain_type = self.chain_type.unwrap_or(ChainType::Development).into_str();
		let _ = replace_chain_type(&plain_chain_spec, chain_type, "Local");

		let raw_chain_spec =
			generate_raw_chain_spec(&binary_path, &plain_chain_spec, RAW_CHAIN_SPEC_FILE_NAME)?;
		generated_files.push(format!(
			"Raw chain specification file generated at: {}",
			raw_chain_spec.display()
		));
		let wasm_file_name = format!("para-{}-wasm.wasm", para_id);
		let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, &wasm_file_name)?;
		generated_files.push(format!(
			"WebAssembly runtime file exported at: {}",
			wasm_file.display().to_string()
		));
		let genesis_file_name = format!("para-{}-genesis-state", para_id);
		let genesis_state_file =
			generate_genesis_state_file(&binary_path, &raw_chain_spec, &genesis_file_name)?;
		generated_files.push(format!(
			"Genesis State exported at {} file",
			genesis_state_file.display().to_string()
		));

		console::Term::stderr().clear_last_lines(5)?;
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

/// Guide the user to generate thier chain specification.
async fn guide_user_to_generate_spec() -> anyhow::Result<BuildSpecCommand> {
	Cli.intro("Generate your chain spec")?;

	// Confirm project path
	let target_path: String = input("Where is the project located?")
		.placeholder("./")
		.default_input("./")
		.interact()?;

	// Prompt for chain id.
	let para_id: u32 = input("What parachain ID should the build use?")
		.placeholder("2000")
		.default_input("2000")
		.interact()?;

	// Prompt for relay chain.
	let mut prompt =
		cliclack::select("Choose the relay chain your chain will be connecting to: ".to_string());
	for (i, relay) in RelayChain::VARIANTS.iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(relay);
		}
		prompt = prompt.item(relay, relay.clone().into_str(), "");
	}
	let rc = prompt.interact()?;
	let relay_chain = rc.clone();

	// Prompt for chain type.
	let mut prompt = cliclack::select("Choose the chain type: ".to_string());
	for (i, chain_type) in ChainType::VARIANTS.iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(chain_type);
		}
		prompt = prompt.item(chain_type, chain_type.clone().into_str(), "");
	}
	let ct = prompt.interact()?;
	let chain_type = ct.clone();

	// Prompt for default bootnode
	let default_bootnode =
		confirm(format!("Would you like to use local host as a bootnode ?")).interact()?;

	Ok(BuildSpecCommand {
		path: Some(PathBuf::from(target_path)),
		release: match chain_type {
			ChainType::Development => false,
			_ => true,
		},
		id: Some(para_id),
		default_bootnode,
		chain_type: Some(chain_type),
		relay: Some(relay_chain),
		valid: true,
	})
}

// Checks if the binary exists in the target directory of the given path
fn maybe_node_binary_path(path: &PathBuf, profile: &Profile) -> Option<PathBuf> {
	// Figure out the name of the binary
	let mut node_path = path.clone();
	node_path.push("node");
	let manifest = match from_path(Some(&node_path)) {
		Ok(manifest) => manifest,
		Err(_) => return None,
	};
	let binary_name = manifest.package().name();
	let binary_path_by_profile = profile.target_folder(&path).join(binary_name);
	if binary_path_by_profile.exists() {
		return Some(binary_path_by_profile);
	} else {
		return None;
	}
}
