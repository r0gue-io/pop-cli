// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::{BuildSpecCommand, CodePathBuf, StatePathBuf},
	call::chain::Call,
	cli::traits::*,
	common::{
		chain::{configure_chain, Chain},
		wallet::submit_extrinsic_with_wallet,
	},
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{extract_para_id_from_event, find_dispatchable_by_name, Action, Payload};
use std::path::{Path, PathBuf};
use url::Url;

const DEFAULT_URL: &str = "wss://paseo.rpc.amforc.com/";
const HELP_HEADER: &str = "Chain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpChainCommand {
	/// Path to the project.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
	/// ID to use. If not specified, a new ID will be reserved.
	#[arg(short, long)]
	pub(crate) id: Option<u32>,
	/// Path to the genesis state file. If not specified, it will be generated.
	#[arg(short = 'G', long = "genesis-state")]
	pub(crate) genesis_state: Option<StatePathBuf>,
	/// Path to the genesis code file.  If not specified, it will be generated.
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: Option<CodePathBuf>,
	/// Websocket endpoint of the relay chain.
	#[arg(long)]
	pub(crate) relay_url: Option<Url>,
}

impl UpChainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Deploy a chain")?;
		let chain_config = match self.prepare_chain_for_registration(cli).await {
			Ok(chain) => chain,
			Err(e) => {
				cli.outro_cancel(format!("{}", e))?;
				return Ok(());
			},
		};
		match chain_config.register_parachain(cli).await {
			Ok(_) => cli.success("Chain deployed successfully")?,
			Err(e) => cli.outro_cancel(format!("{}", e))?,
		}
		Ok(())
	}

	// Prepares the chain for registration by setting up its configuration.
	async fn prepare_chain_for_registration(self, cli: &mut impl Cli) -> Result<UpChain> {
		let chain = configure_chain(
			"Enter the relay chain node URL to deploy your parachain",
			DEFAULT_URL,
			&self.relay_url,
			cli,
		)
		.await?;
		let para_id = self.resolve_parachain_id(&chain, cli).await?;
		let (genesis_code, genesis_state) = self.resolve_genesis_files(para_id, cli).await?;
		Ok(UpChain { id: para_id, genesis_state, genesis_code, chain })
	}

	// Resolves the parachain ID, reserving a new one if necessary.
	async fn resolve_parachain_id(&self, chain: &Chain, cli: &mut impl Cli) -> Result<u32> {
		match self.id {
			Some(id) => Ok(id),
			None => {
				cli.info("Reserving a parachain ID")?;
				reserve_para_id(chain, cli).await
			},
		}
	}
	// Resolves the genesis state and code files, generating them if necessary.
	async fn resolve_genesis_files(
		&self,
		para_id: u32,
		cli: &mut impl Cli,
	) -> Result<(CodePathBuf, StatePathBuf)> {
		match (&self.genesis_code, &self.genesis_state) {
			(Some(code), Some(state)) => Ok((code.clone(), state.clone())),
			_ => {
				cli.info("Generating the chain spec for your parachain")?;
				generate_spec_files(para_id, self.path.as_deref(), cli).await
			},
		}
	}
}

// Represents the configuration for deploying a chain.
pub(crate) struct UpChain {
	id: u32,
	genesis_state: PathBuf,
	genesis_code: PathBuf,
	chain: Chain,
}
impl UpChain {
	// Registers by submitting an extrinsic.
	async fn register_parachain(&self, cli: &mut impl Cli) -> Result<()> {
		cli.info("Registering a parachain ID")?;
		let call_data = self.prepare_register_parachain_call_data(cli)?;
		submit_extrinsic_with_wallet(&self.chain.client, &self.chain.url, call_data, cli).await?;
		Ok(())
	}

	// Prepares and returns the encoded call data for registering a parachain.
	fn prepare_register_parachain_call_data(&self, cli: &mut impl Cli) -> Result<Vec<u8>> {
		let UpChain { id, genesis_code, genesis_state, chain } = self;
		let dispatchable = find_dispatchable_by_name(
			&chain.pallets,
			Action::Register.pallet_name(),
			Action::Register.function_name(),
		)?;
		let state = std::fs::read_to_string(genesis_state)
			.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
		let code = std::fs::read_to_string(genesis_code)
			.map_err(|err| anyhow!("Failed to read genesis code file: {}", err.to_string()))?;
		let xt = Call {
			function: dispatchable.clone(),
			args: vec![id.to_string(), state, code],
			use_wallet: true,
			..Default::default()
		}
		.prepare_extrinsic(&chain.client, cli)?;
		Ok(xt.encode_call_data(&chain.client.metadata())?)
	}
}

// Reserves an ID by submitting an extrinsic.
async fn reserve_para_id(chain: &Chain, cli: &mut impl Cli) -> Result<u32> {
	let call_data = prepare_reserve_parachain_call_data(chain, cli)?;
	let events = submit_extrinsic_with_wallet(&chain.client, &chain.url, call_data, cli)
		.await
		.map_err(|e| anyhow::anyhow!("ID reservation failed: {}", e))?;
	let para_id = extract_para_id_from_event(&events).map_err(|_| {
		anyhow::anyhow!("Unable to parse the event. Specify the ID manually with `--id`.")
	})?;
	cli.success(format!("Successfully reserved ID: {}", para_id))?;
	Ok(para_id)
}

// Prepares and returns the encoded call data for reserving an ID.
fn prepare_reserve_parachain_call_data(chain: &Chain, cli: &mut impl Cli) -> Result<Vec<u8>> {
	let dispatchable = find_dispatchable_by_name(
		&chain.pallets,
		Action::Reserve.pallet_name(),
		Action::Reserve.function_name(),
	)?;
	let xt = Call { function: dispatchable.clone(), use_wallet: true, ..Default::default() }
		.prepare_extrinsic(&chain.client, cli)?;
	Ok(xt.encode_call_data(&chain.client.metadata())?)
}

// Generates chain spec files for the project.
async fn generate_spec_files(
	id: u32,
	path: Option<&Path>,
	cli: &mut impl Cli,
) -> anyhow::Result<(CodePathBuf, StatePathBuf)> {
	// Changes the working directory if a path is provided to ensure the build spec process runs in
	// the correct context.
	if let Some(path) = path {
		std::env::set_current_dir(path)?;
	}
	let build_spec = BuildSpecCommand {
		id: Some(id),
		genesis_code: true,
		genesis_state: true,
		..Default::default()
	}
	.configure_build_spec(cli)
	.await?;

	let (genesis_code_file, genesis_state_file) = build_spec.build(cli)?;
	Ok((
		genesis_code_file.ok_or_else(|| anyhow::anyhow!("Failed to generate the genesis code."))?,
		genesis_state_file
			.ok_or_else(|| anyhow::anyhow!("Failed to generate the genesis state file."))?,
	))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		build::spec::{ChainType, RelayChain},
		cli::MockCli,
	};
	use duct::cmd;
	use pop_common::Profile;
	use pop_parachains::decode_call_data;
	use std::{env, fs};
	use strum::{EnumMessage, VariantArray};
	use tempfile::tempdir;
	use url::Url;

	const POLKADOT_NETWORK_URL: &str = "wss://polkadot-rpc.publicnode.com";
	const POP_NETWORK_TESTNET_URL: &str = "wss://rpc1.paseo.popnetwork.xyz";

	#[tokio::test]
	async fn prepare_chain_for_registration_works() -> Result<()> {
		let mut cli = MockCli::new().expect_input(
			"Enter the relay chain node URL to deploy your parachain",
			POLKADOT_NETWORK_URL.into(),
		);
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		let chain_config = UpChainCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			..Default::default()
		}
		.prepare_chain_for_registration(&mut cli)
		.await?;

		assert_eq!(chain_config.id, 2000);
		assert_eq!(chain_config.genesis_code, genesis_code);
		assert_eq!(chain_config.genesis_state, genesis_state);
		assert_eq!(chain_config.chain.url, Url::parse(POLKADOT_NETWORK_URL)?);
		cli.verify()
	}

	#[tokio::test]
	async fn prepare_reserve_parachain_call_data_works() -> Result<()> {
		let mut cli = MockCli::new();
		let chain = configure_chain(
			"Enter the relay chain node URL to deploy your parachain",
			DEFAULT_URL,
			&Some(Url::parse(POLKADOT_NETWORK_URL)?),
			&mut cli,
		)
		.await?;
		let call_data = prepare_reserve_parachain_call_data(&chain, &mut cli)?;
		// Encoded call data for a reserve extrinsic.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4605
		let encoded_reserve_extrinsic: &str = "0x4605";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);
		Ok(())
	}

	#[tokio::test]
	async fn reserve_parachain_id_fails_wrong_chain() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Deploy a chain")
			.expect_info("Reserving a parachain ID")
			.expect_outro_cancel("Failed to find the pallet Registrar");
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		UpChainCommand {
			id: None,
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			relay_url: Some(Url::parse(POP_NETWORK_TESTNET_URL)?),
			path: None,
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn resolve_genesis_files_fails_wrong_path() -> Result<()> {
		// Mock a project path without node.
		let name = "hello_world";
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let project_path = path.join(name);
		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		let original_dir = std::env::current_dir()?;

		let mut cli = MockCli::new()
			.expect_intro("Deploy a chain")
			.expect_info("Generating the chain spec for your parachain")
			.expect_input("Provide the chain specification to use (e.g. dev, local, custom or a path to an existing file)", "dev".to_string())
			.expect_input(
				"Name or path for the plain chain spec file:", "output_file".to_string())
			.expect_input(
				"Enter the protocol ID that will identify your network:", "protocol_id".to_string())
			.expect_select(
				"Choose the chain type: ",
				Some(false),
				true,
				Some(get_messages(ChainType::VARIANTS)),
				ChainType::Development as usize,
			).expect_select(
				"Choose the relay your chain will be connecting to: ",
				Some(false),
				true,
				Some(get_messages(RelayChain::VARIANTS)),
				RelayChain::PaseoLocal as usize,
			).expect_select(
				"Choose the build profile of the binary that should be used: ",
				Some(false),
				true,
				Some(get_messages(Profile::VARIANTS)),
				Profile::Release as usize,
		).expect_outro_cancel(format!("Failed to get manifest path: {}/node/Cargo.toml", fs::canonicalize(&project_path)?.display().to_string()));

		UpChainCommand {
			id: Some(2000),
			genesis_state: None,
			genesis_code: None,
			relay_url: Some(Url::parse(POP_NETWORK_TESTNET_URL)?),
			path: Some(project_path.clone()),
		}
		.execute(&mut cli)
		.await?;

		assert_eq!(fs::canonicalize(env::current_dir()?)?, fs::canonicalize(project_path)?);
		// Reset working directory back to original
		std::env::set_current_dir(original_dir)?;
		cli.verify()
	}

	#[tokio::test]
	async fn register_parachain_fails_wrong_chain() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Deploy a chain")
			.expect_info("Registering a parachain ID")
			.expect_outro_cancel("Failed to find the pallet Registrar");
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		UpChainCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			relay_url: Some(Url::parse(POP_NETWORK_TESTNET_URL)?),
			path: None,
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn prepare_register_parachain_call_data_works() -> Result<()> {
		let mut cli = MockCli::new();
		let chain = configure_chain(
			"Enter the relay chain node URL to deploy your parachain",
			DEFAULT_URL,
			&Some(Url::parse(POLKADOT_NETWORK_URL)?),
			&mut cli,
		)
		.await?;
		// Create a temporary files to act as genesis_state and genesis_code files.
		let temp_dir = tempdir()?;
		let genesis_state_path = temp_dir.path().join("genesis_state");
		let genesis_code_path = temp_dir.path().join("genesis_code.wasm");
		let up_chain = UpChain {
			id: 2000,
			genesis_state: genesis_state_path.clone(),
			genesis_code: genesis_code_path.clone(),
			chain,
		};

		// Expect failure when the genesis state file cannot be read.
		assert!(matches!(
			up_chain.prepare_register_parachain_call_data(&mut cli),
			Err(message) if message.to_string().contains("Failed to read genesis state file")
		));
		std::fs::write(&genesis_state_path, "0x1234")?;

		// Expect failure when the genesis code file cannot be read.
		assert!(matches!(
			up_chain.prepare_register_parachain_call_data(&mut cli),
			Err(message) if message.to_string().contains("Failed to read genesis code file")
		));
		std::fs::write(&genesis_code_path, "0x1234")?;

		// Encoded call data for a register extrinsic with the above values.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4600d0070000081234081234
		let encoded_register_extrinsic: &str = "0x4600d0070000081234081234";
		assert_eq!(
			up_chain.prepare_register_parachain_call_data(&mut cli)?,
			decode_call_data(encoded_register_extrinsic)?
		);
		Ok(())
	}

	// Creates temporary files to act as `genesis_state` and `genesis_code` files.
	fn create_temp_genesis_files() -> Result<(PathBuf, PathBuf)> {
		let temp_dir = tempdir()?; // Create a temporary directory
		let genesis_state_path = temp_dir.path().join("genesis_state");
		let genesis_code_path = temp_dir.path().join("genesis_code.wasm");

		fs::write(&genesis_state_path, "0x1234")?;
		fs::write(&genesis_code_path, "0x1234")?;

		Ok((genesis_state_path, genesis_code_path))
	}

	// Generic helper function to convert enum variants into (message, detailed message) tuples.
	fn get_messages<T: EnumMessage + AsRef<str>>(variants: &[T]) -> Vec<(String, String)> {
		variants
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
