// SPDX-License-Identifier: GPL-3.0

use crate::{cli, cli::traits::Cli as _, cli::Cli, style::style};
use clap::{Args, ValueEnum};
use cliclack::{confirm, input};
use pop_common::{manifest::from_path, Profile};
use pop_parachains::{
	build_parachain, export_wasm_file, generate_genesis_state_file, generate_plain_chain_spec,
	generate_raw_chain_spec, is_supported, replace_chain_type, replace_protocol_id,
	replace_relay_spec,
};
use std::{env::current_dir, fs::create_dir_all, path::PathBuf};
#[cfg(not(test))]
use std::{thread::sleep, time::Duration};
use strum::{EnumMessage, VariantArray};
use strum_macros::{AsRefStr, Display, EnumString};

const DEFAULT_SPEC_NAME: &str = "template-chain-spec.json";

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
	#[strum(serialize = "rococo", message = "Rococo", detailed_message = "Parity's test network.")]
	Rococo,
	#[strum(
		serialize = "rococo-local",
		message = "Rococo Local",
		detailed_message = "Local configuration for Rococo network."
	)]
	RococoLocal,
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
	/// [default: ./template-chain-spec.json].
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
	/// Procotol-id to use in the specification.
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
				Some(_) => self.build(&mut cli::Cli),
				None => {
					let config = guide_user_to_generate_spec().await?;
					config.build(&mut cli::Cli)
				},
			};
			return Ok("spec");
		} else {
			cli::Cli.outro_cancel(
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
		cli.intro(format!("Building your chain spec"))?;

		// Either a para id was already provided or user has been guided to provide one.
		let para_id = self.id.unwrap_or(2000);
		// Notify user in case we need to build the parachain project.
		if !self.release {
			cli.warning("NOTE: this command defaults to DEBUG builds for development chain types. Please use `--release` (or simply `-r` for a release build...")?;
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
		let binary_path = match maybe_node_binary_path(&mode) {
			Some(binary_path) => binary_path,
			None => {
				cli.info(format!("No node was not found. The project will be built locally."))?;
				cli.warning("NOTE: this may take some time...")?;
				build_parachain(&output_path, None, &mode, None)?
			},
		};

		println!("{}", &binary_path.display());

		// Generate plain spec.
		spinner.set_message("Generating plain chain specification...");
		let mut generated_files =
			vec![format!("Specification and artifacts generated at: {}", &output_path.display())];
		generate_plain_chain_spec(&binary_path, &plain_chain_spec, para_id, self.default_bootnode)?;
		generated_files.push(format!(
			"Plain text chain specification file generated at: {}",
			plain_chain_spec.display()
		));

		// Customize spec based on input.
		let relay = self.relay.unwrap_or(RelayChain::PaseoLocal).to_string();
		replace_relay_spec(&plain_chain_spec, &relay, "rococo-local")?;
		let chain_type = self.chain_type.unwrap_or(ChainType::Development).to_string();
		replace_chain_type(&plain_chain_spec, &chain_type, "Local")?;
		if self.protocol_id.is_some() {
			let protocol_id = self.protocol_id.unwrap_or("template-local".to_string());
			replace_protocol_id(&plain_chain_spec, &protocol_id, "template-local")?;
		}

		// Generate raw spec.
		spinner.set_message("Generating raw chain specification...");
		let raw_spec_name = plain_chain_spec
			.file_name()
			.and_then(|s| s.to_str())
			.unwrap_or(DEFAULT_SPEC_NAME);
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
			generated_files.push(format!(
				"WebAssembly runtime file exported at: {}",
				wasm_file.display().to_string()
			));
		}

		if self.genesis_state {
			spinner.set_message("Generating genesis state...");
			let genesis_file_name = format!("para-{}-genesis-state", para_id);
			let genesis_state_file =
				generate_genesis_state_file(&binary_path, &raw_chain_spec, &genesis_file_name)?;
			generated_files.push(format!(
				"Genesis State file exported at: {}",
				genesis_state_file.display().to_string()
			));
		}

		console::Term::stderr().clear_last_lines(1)?;
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
async fn guide_user_to_generate_spec() -> anyhow::Result<BuildSpecCommand> {
	Cli.intro("Generate your chain spec")?;

	// Confirm output path
	let output_file: String = input("Name of the chain spec file. If a path is given, the necessary directories will be created:")
		.placeholder("./template-chain-spec.json")
		.default_input("./template-chain-spec.json")
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
		prompt = prompt.item(
			relay,
			relay.get_message().unwrap_or(relay.as_ref()),
			relay.get_detailed_message().unwrap_or_default(),
		);
	}
	let relay_chain = prompt.interact()?.clone();

	// Prompt for chain type.
	// If relay is Kusama or Polkadot, then Live type is used and user is not prompted.
	let chain_type: ChainType;
	if relay_chain == RelayChain::Polkadot || relay_chain == RelayChain::Kusama {
		chain_type = ChainType::Live;
	} else {
		let mut prompt = cliclack::select("Choose the chain type: ".to_string());
		for (i, chain_type) in ChainType::VARIANTS.iter().enumerate() {
			if i == 0 {
				prompt = prompt.initial_value(chain_type);
			}
			prompt = prompt.item(
				chain_type,
				chain_type.get_message().unwrap_or(chain_type.as_ref()),
				chain_type.get_detailed_message().unwrap_or_default(),
			);
		}
		chain_type = prompt.interact()?.clone();
	}

	// Prompt for default bootnode if chian type is Local or Live.
	let default_bootnode = match chain_type {
		ChainType::Development => true,
		_ => confirm(format!("Would you like to use local host as a bootnode ?")).interact()?,
	};

	// Prompt for protocol-id.
	let protocol_id: String =
		input("Choose the protocol-id that will identify your network on the networking layer: ")
			.placeholder("template-local")
			.default_input("template-local")
			.interact()?;

	// Prompt for genesis state
	let genesis_state = confirm(format!("Should the genesis state file be generated ?"))
		.initial_value(true)
		.interact()?;

	// Prompt for genesis code
	let genesis_code = confirm(format!("Should the genesis code file be generated ?"))
		.initial_value(true)
		.interact()?;

	Ok(BuildSpecCommand {
		output_file: Some(PathBuf::from(output_file)),
		release: match chain_type {
			ChainType::Development => false,
			_ => true,
		},
		id: Some(para_id),
		default_bootnode,
		chain_type: Some(chain_type),
		relay: Some(relay_chain),
		protocol_id: Some(protocol_id),
		genesis_state,
		genesis_code,
	})
}

// Checks if the binary exists in the target directory of the given path
fn maybe_node_binary_path(profile: &Profile) -> Option<PathBuf> {
	// Try to figure out the name of the binary.
	// Assumes being run on the project root.
	let mut cwd = current_dir().unwrap_or(PathBuf::from("./"));
	cwd.push("node");
	let manifest = match from_path(Some(&cwd)) {
		Ok(manifest) => manifest,
		Err(_) => return None,
	};
	cwd.pop();
	let binary_path_by_profile = profile.target_folder(&cwd).join(manifest.package().name());
	if binary_path_by_profile.exists() {
		return Some(binary_path_by_profile);
	} else {
		return None;
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use anyhow::Result;
	use assert_cmd::Command;
	use std::{ops::Deref, path::Path};

	#[test]
	fn build_spec_works() -> Result<()> {
		// Generate template parachain in temp directory.
		let chain_dir = generate_parachain();

		// pop build spec --output ./test-spec.json --id 1234"
		Command::cargo_bin("pop")
			.unwrap()
			.current_dir(&chain_dir)
			.args(&["build", "spec", "--output", "./test-spec.json", "--id", "1234"])
			.assert()
			.success();

		// Assert build files have been generated in ./
		assert!(chain_dir.join("./test-spec.json").exists());
		assert!(chain_dir.join("./raw-test-spec.json").exists());
		assert!(chain_dir.join("./para-1234.wasm").exists());
		assert!(chain_dir.join("./para-1234-genesis-state").exists());

		Ok(())
	}

	#[test]
	fn build_spec_creates_non_existing_output_folder() -> Result<()> {
		// Generate template parachain in temp directory.
		let chain_dir = generate_parachain();
		println!("{}", chain_dir.display());

		// pop build spec --output ./new/directory/test-spec.json --id 1234"
		Command::cargo_bin("pop")
			.unwrap()
			.current_dir(&chain_dir)
			.args(&["build", "spec", "--output", "./new/directory/test-spec.json", "--id", "1234"])
			.assert()
			.success();

		// Assert build files have been generated in ./
		assert!(chain_dir.join("./new/directory/test-spec.json").exists());
		assert!(chain_dir.join("./new/directory/raw-test-spec.json").exists());
		assert!(chain_dir.join("./new/directory/para-1234.wasm").exists());
		assert!(chain_dir.join("./new/directory/para-1234-genesis-state").exists());

		Ok(())
	}

	fn generate_parachain() -> PathBuf {
		//let temp = tempfile::tempdir().unwrap();
		//let temp_dir = temp.path();
		let temp_dir = Path::new("./.test");
		let _ = create_dir_all(temp_dir);

		// pop new parachain test_parachain
		Command::cargo_bin("pop")
			.unwrap()
			.current_dir(&temp_dir)
			.args(&[
				"new",
				"parachain",
				"test_parachain",
				"--symbol",
				"POP",
				"--decimals",
				"6",
				"--endowment",
				"1u64 << 60",
			])
			.assert()
			.success();

		let chain_dir = temp_dir.join("test_parachain");
		assert!(chain_dir.exists());

		chain_dir
	}
}
