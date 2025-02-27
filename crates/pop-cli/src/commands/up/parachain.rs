// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::BuildSpecCommand,
	call::chain::prompt_for_param,
	cli::traits::*,
	common::{
		chain::{configure_chain, Chain},
		wallet::submit_extrinsic_with_wallet,
	},
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, construct_proxy_extrinsic, extract_para_id_from_event,
	find_dispatchable_by_name, Action, Payload,
};
use std::path::{Path, PathBuf};
use url::Url;

const DEFAULT_URL: &str = "wss://paseo.rpc.amforc.com/";
const HELP_HEADER: &str = "Chain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpChainCommand {
	/// Path to the chain directory.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
	/// Parachain ID to use. If not specified, a new ID will be reserved.
	#[arg(short, long)]
	pub(crate) id: Option<u32>,
	/// Path to the genesis state file. If not specified, it will be generated.
	#[arg(short = 'G', long = "genesis-state")]
	pub(crate) genesis_state: Option<PathBuf>,
	/// Path to the genesis code file.  If not specified, it will be generated.
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: Option<PathBuf>,
	/// Websocket endpoint of the relay chain.
	#[arg(long)]
	pub(crate) relay_url: Option<Url>,
	/// Proxy address for registration. Specify the address type, e.g.
	/// `Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)`.
	#[arg(long = "proxy")]
	pub(crate) proxy_address: Option<String>,
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
		chain_config.display();
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

		let proxy = self.resolve_proxy_address(&chain, cli)?;
		let para_id = self.resolve_parachain_id(&chain, &proxy, cli).await?;
		let (genesis_state, genesis_code) = self.resolve_genesis_files(para_id, cli).await?;
		Ok(UpChain { id: para_id, genesis_state, genesis_code, chain, proxy })
	}

	// Retrieves the proxy address, prompting the user if none is specified.
	fn resolve_proxy_address(&self, chain: &Chain, cli: &mut impl Cli) -> Result<ProxyConfig> {
		if let Some(addr) = &self.proxy_address {
			return Ok(ProxyConfig::Address(addr.clone()));
		}
		if cli.confirm("Would you like to use a proxy for registration? This is considered a best practice.").interact()? {
			let proxy = find_dispatchable_by_name(&chain.pallets, "Proxy", "proxy")?;
			let address = prompt_for_param(cli, &proxy.params[0])?;
			Ok(ProxyConfig::Address(address))
		} else {
			Ok(ProxyConfig::None)
		}
	}

	// Resolves the parachain ID, reserving a new one if necessary.
	async fn resolve_parachain_id(
		&self,
		chain: &Chain,
		proxy: &ProxyConfig,
		cli: &mut impl Cli,
	) -> Result<u32> {
		match self.id {
			Some(id) => Ok(id),
			None => {
				cli.info("Reserving a parachain ID")?;
				reserve_para_id(chain, proxy, cli).await
			},
		}
	}
	// Resolves the genesis state and code files, generating them if necessary.
	async fn resolve_genesis_files(
		&self,
		para_id: u32,
		cli: &mut impl Cli,
	) -> Result<(PathBuf, PathBuf)> {
		match (&self.genesis_state, &self.genesis_code) {
			(Some(state), Some(code)) => Ok((state.clone(), code.clone())),
			_ => {
				cli.info("Generating the chain spec for your parachain")?;
				generate_spec_files(para_id, self.path.as_deref(), cli).await
			},
		}
	}
}

#[derive(Clone, Debug, PartialEq)]
enum ProxyConfig {
	None,
	Address(String),
}

// Represents the configuration for deploying a chain.
pub(crate) struct UpChain {
	id: u32,
	genesis_state: PathBuf,
	genesis_code: PathBuf,
	chain: Chain,
	proxy: ProxyConfig,
}
impl UpChain {
	// Registers a parachain by submitting an extrinsic.
	async fn register_parachain(&self, cli: &mut impl Cli) -> Result<()> {
		cli.info("Registering a parachain ID")?;
		let call_data = self.prepare_register_parachain_call_data()?;
		submit_extrinsic_with_wallet(&self.chain.client, &self.chain.url, call_data, cli).await?;
		Ok(())
	}

	// Prepares and returns the encoded call data for registering a parachain.
	fn prepare_register_parachain_call_data(&self) -> Result<Vec<u8>> {
		let UpChain { id, genesis_code, genesis_state, chain, proxy } = self;
		let ex = find_dispatchable_by_name(
			&chain.pallets,
			Action::Register.pallet_name(),
			Action::Register.function_name(),
		)?;
		let state = std::fs::read_to_string(genesis_state)
			.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
		let code = std::fs::read_to_string(genesis_code)
			.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
		let mut xt = construct_extrinsic(ex, vec![id.to_string(), state, code])?;
		if let ProxyConfig::Address(addr) = proxy {
			xt = construct_proxy_extrinsic(&chain.pallets, addr.to_string(), xt)?;
		}
		Ok(xt.encode_call_data(&chain.client.metadata())?)
	}

	fn display(&self) -> String {
		let mut full_message = "pop up".to_string();
		full_message.push_str(&format!(" --id {}", self.id));
		full_message.push_str(&format!(" --genesis-state {}", self.genesis_state.display()));
		full_message.push_str(&format!(" --genesis-code {}", self.genesis_code.display()));
		full_message.push_str(&format!(" --relay-url {}", self.chain.url));
		if let ProxyConfig::Address(addr) = &self.proxy {
			full_message.push_str(&format!(" --proxy {}", addr));
		}
		full_message
	}
}

// Reserves a parachain ID by submitting an extrinsic.
async fn reserve_para_id(chain: &Chain, proxy: &ProxyConfig, cli: &mut impl Cli) -> Result<u32> {
	let call_data = prepare_reserve_parachain_call_data(chain, proxy)?;
	let events = submit_extrinsic_with_wallet(&chain.client, &chain.url, call_data, cli)
		.await
		.map_err(|e| anyhow::anyhow!("Parachain ID reservation failed: {}", e))?;
	let para_id = extract_para_id_from_event(&events).map_err(|_| {
		anyhow::anyhow!("Unable to parse the event. Specify the parachain ID manually with `--id`.")
	})?;
	cli.success(format!("Successfully reserved parachain ID: {}", para_id))?;
	Ok(para_id)
}

// Prepares and returns the encoded call data for reserving a parachain ID.
fn prepare_reserve_parachain_call_data(chain: &Chain, proxy: &ProxyConfig) -> Result<Vec<u8>> {
	let dispatchable = find_dispatchable_by_name(
		&chain.pallets,
		Action::Reserve.pallet_name(),
		Action::Reserve.function_name(),
	)?;
	let mut xt = construct_extrinsic(dispatchable, Vec::new())?;
	if let ProxyConfig::Address(addr) = proxy {
		xt = construct_proxy_extrinsic(&chain.pallets, addr.to_string(), xt)?;
	}
	Ok(xt.encode_call_data(&chain.client.metadata())?)
}

// Generates chain spec files for the parachain.
async fn generate_spec_files(
	id: u32,
	path: Option<&Path>,
	cli: &mut impl Cli,
) -> anyhow::Result<(PathBuf, PathBuf)> {
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
	build_spec.generate_genesis_artifacts(cli)
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
			proxy_address: Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string()),
			..Default::default()
		}
		.prepare_chain_for_registration(&mut cli)
		.await?;

		assert_eq!(chain_config.id, 2000);
		assert_eq!(chain_config.genesis_code, genesis_code);
		assert_eq!(chain_config.genesis_state, genesis_state);
		assert_eq!(chain_config.chain.url, Url::parse(POLKADOT_NETWORK_URL)?);
		assert_eq!(
			chain_config.proxy,
			ProxyConfig::Address(
				"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string()
			)
		);
		cli.verify()
	}

	#[tokio::test]
	async fn resolve_proxy_address_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_confirm("Would you like to use a proxy for registration? This is considered a best practice.", true)
			.expect_select(
				"Select the value for the parameter: real",
				Some(true),
				true,
				Some(
					[
						("Id".to_string(), "".to_string()),
						("Index".to_string(), "".to_string()),
						("Raw".to_string(), "".to_string()),
						("Address32".to_string(), "".to_string()),
						("Address20".to_string(), "".to_string()),
					]
					.to_vec(),
				),
				0, // "Id" action
			)
			.expect_input(
				"Enter the value for the parameter: Id",
				"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".into(),
			);
		let chain = configure_chain(
			"Enter the relay chain node URL to deploy your parachain",
			DEFAULT_URL,
			&Some(Url::parse(POLKADOT_NETWORK_URL)?),
			&mut cli,
		)
		.await?;
		let proxy_address = UpChainCommand::default().resolve_proxy_address(&chain, &mut cli)?;
		assert_eq!(
			proxy_address,
			ProxyConfig::Address(
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string()
			)
		);
		cli.verify()?;

		cli = MockCli::new().expect_confirm("Do you want to use a proxy for registration?", false);
		let proxy_address = UpChainCommand::default().resolve_proxy_address(&chain, &mut cli)?;
		assert_eq!(proxy_address, ProxyConfig::None);
		cli.verify()?;

		cli = MockCli::new();
		let proxy_address = UpChainCommand {
			proxy_address: Some("Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string()),
			..Default::default()
		}
		.resolve_proxy_address(&chain, &mut cli)?;
		assert_eq!(
			proxy_address,
			ProxyConfig::Address(
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string()
			)
		);
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
		let call_data = prepare_reserve_parachain_call_data(&chain, &ProxyConfig::None)?;
		// Encoded call data for a reserve extrinsic.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4605
		let encoded_reserve_extrinsic: &str = "0x4605";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);

		// Ensure `prepare_reserve_parachain_call_data` works with a proxy.
		let call_data = prepare_reserve_parachain_call_data(
			&chain,
			&ProxyConfig::Address(
				"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string(),
			),
		)?;
		// Encoded call data for a proxy extrinsic with reserve as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004605
		let encoded_reserve_extrinsic: &str =
			"0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004605";
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
			proxy_address: None,
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
			proxy_address: None,
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
			proxy_address: None,
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
		std::fs::write(&genesis_state_path, "0x1234")?;
		std::fs::write(&genesis_code_path, "0x1234")?;

		let mut up_chain = UpChain {
			id: 2000,
			genesis_state: genesis_state_path,
			genesis_code: genesis_code_path,
			chain,
			proxy: ProxyConfig::None,
		};
		let call_data = up_chain.prepare_register_parachain_call_data()?;
		// Encoded call data for a register extrinsic with the above values.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4600d0070000081234081234
		let encoded_register_extrinsic: &str = "0x4600d0070000081234081234";
		assert_eq!(call_data, decode_call_data(encoded_register_extrinsic)?);

		// Ensure `prepare_register_parachain_call_data` works with a proxy.
		up_chain.proxy = ProxyConfig::Address(
			"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string(),
		);
		let call_data = up_chain.prepare_register_parachain_call_data()?;
		// Encoded call data for a proxy extrinsic with register as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234
		let encoded_reserve_extrinsic: &str = "0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);
		Ok(())
	}

	#[tokio::test]
	async fn display_works() -> Result<()> {
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
			genesis_state: genesis_state_path,
			genesis_code: genesis_code_path,
			chain,
			proxy: ProxyConfig::Address(
				"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string(),
			),
		};
		assert_eq!(
			up_chain.display(),
			format!(
				"pop up --id {} --genesis-state {} --genesis-code {} --relay-url {} --proxy {}",
				up_chain.id,
				up_chain.genesis_state.display(),
				up_chain.genesis_code.display(),
				up_chain.chain.url,
				"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)"
			)
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
