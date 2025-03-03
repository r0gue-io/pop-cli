// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::{BuildSpecCommand, CodePathBuf, StatePathBuf},
	call::chain::{prompt_for_param, Call},
	cli::traits::*,
	common::{
		chain::{configure, Chain},
		wallet::submit_extrinsic,
	},
	style::style,
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_proxy_extrinsic, find_dispatchable_by_name, Action, Payload, Reserved,
};
use std::path::{Path, PathBuf};
use url::Url;

type Proxy = Option<String>;

const DEFAULT_URL: &str = "wss://paseo.rpc.amforc.com/";
const HELP_HEADER: &str = "Chain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpCommand {
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
	pub(crate) relay_chain_url: Option<Url>,
	/// Proxied address. Your account must be registered as a proxy which can act on behalf of this
	/// account.  You can specify the MultiAddress type explicitly, e.g.,
	/// `Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)`. If no type is provided, it
	/// defaults to `Id()`.
	#[arg(long = "proxy")]
	pub(crate) proxied_address: Option<String>,
}

impl UpCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Deploy a rollup")?;
		let config = match self.prepare_for_registration(cli).await {
			Ok(chain) => chain,
			Err(e) => {
				cli.outro_cancel(format!("{}", e))?;
				return Ok(());
			},
		};
		match config.register(cli).await {
			Ok(_) => cli.success(format!(
				"Deployment successfully {}",
				style(format!(
					"https://polkadot.js.org/apps/?rpc={}#/parachains",
					config.chain.url
				))
				.dim()
			))?,
			Err(e) => cli.outro_cancel(format!("{}", e))?,
		}
		Ok(())
	}

	// Prepares the chain for registration by setting up its configuration.
	async fn prepare_for_registration(self, cli: &mut impl Cli) -> Result<Registration> {
		let chain =
			configure("Enter the relay chain node URL", DEFAULT_URL, &self.relay_chain_url, cli)
				.await?;
		let proxy = self.resolve_proxied_address(&chain, cli)?;
		let id = self.resolve_id(&chain, &proxy, cli).await?;
		let (genesis_code, genesis_state) = self.resolve_genesis_files(id, cli).await?;
		Ok(Registration { id, genesis_state, genesis_code, chain, proxy })
	}

	// Retrieves the proxied address, prompting the user if none is specified.
	fn resolve_proxied_address(&self, chain: &Chain, cli: &mut impl Cli) -> Result<Proxy> {
		let proxy = find_dispatchable_by_name(&chain.pallets, "Proxy", "proxy")?;
		if let Some(addr) = &self.proxied_address {
			let valid_multi_address: Vec<String> =
				proxy.params[0].sub_params.iter().map(|p| p.name.to_string()).collect();
			return Ok(
				if valid_multi_address.iter().any(|t| addr.starts_with(&format!("{}(", t))) {
					Some(addr.to_string())
				} else {
					Some(format!("Id({})", addr))
				},
			);
		}
		if cli.confirm("Would you like to use a proxy for registration? This is considered a best practice.").interact()? {
			cli.info("Enter the account that the proxy will make a call on behalf of.")?;
			let address = prompt_for_param(cli, &proxy.params[0])?;
			return Ok(Some(address));
		}
		Ok(None)
	}

	// Resolves the ID, reserving a new one if necessary.
	async fn resolve_id(&self, chain: &Chain, proxy: &Proxy, cli: &mut impl Cli) -> Result<u32> {
		match self.id {
			Some(id) => Ok(id),
			None => {
				cli.info(format!("Reserving an ID. You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.", chain.url))?;
				reserve(chain, proxy, cli).await
			},
		}
	}
	// Resolves the genesis state and code files, generating them if necessary.
	async fn resolve_genesis_files(
		&self,
		id: u32,
		cli: &mut impl Cli,
	) -> Result<(CodePathBuf, StatePathBuf)> {
		match (&self.genesis_code, &self.genesis_state) {
			(Some(code), Some(state)) => Ok((code.clone(), state.clone())),
			_ => {
				cli.info("Generating the chain spec for your project")?;
				generate_spec_files(id, self.path.as_deref(), cli).await
			},
		}
	}
}

// Represents the configuration for rollup registration.
pub(crate) struct Registration {
	id: u32,
	genesis_state: PathBuf,
	genesis_code: PathBuf,
	chain: Chain,
	proxy: Proxy,
}
impl Registration {
	// Registers by submitting an extrinsic.
	async fn register(&self, cli: &mut impl Cli) -> Result<()> {
		cli.info(format!("Registering. You will need to sign a transaction on {} using the `Registrar::register` function.", self.chain.url))?;
		let call_data = self.prepare_register_call_data(cli)?;
		submit_extrinsic(&self.chain.client, &self.chain.url, call_data, cli).await?;
		Ok(())
	}

	// Prepares and returns the encoded call data for registering a chain.
	fn prepare_register_call_data(&self, cli: &mut impl Cli) -> Result<Vec<u8>> {
		let Registration { id, genesis_code, genesis_state, chain, proxy } = self;
		let dispatchable = find_dispatchable_by_name(
			&chain.pallets,
			Action::Register.pallet_name(),
			Action::Register.function_name(),
		)?;
		let state = std::fs::read_to_string(genesis_state)
			.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
		let code = std::fs::read_to_string(genesis_code)
			.map_err(|err| anyhow!("Failed to read genesis code file: {}", err.to_string()))?;
		let mut xt = Call {
			function: dispatchable.clone(),
			args: vec![id.to_string(), state, code],
			use_wallet: true,
			..Default::default()
		}
		.prepare_extrinsic(&chain.client, cli)?;
		if let Some(addr) = proxy {
			xt = construct_proxy_extrinsic(&chain.pallets, addr.to_string(), xt)?;
		}
		Ok(xt.encode_call_data(&chain.client.metadata())?)
	}
}

// Reserves an ID by submitting an extrinsic.
async fn reserve(chain: &Chain, proxy: &Proxy, cli: &mut impl Cli) -> Result<u32> {
	let call_data = prepare_reserve_call_data(chain, proxy, cli)?;
	let events = submit_extrinsic(&chain.client, &chain.url, call_data, cli)
		.await
		.map_err(|e| anyhow::anyhow!("ID reservation failed: {}", e))?;
	let id = events
		.find_first::<Reserved>()?
		.ok_or(anyhow::anyhow!("Unable to parse the event. Specify the ID manually with `--id`."))?
		.para_id;
	cli.success(format!("Successfully reserved ID: {}", id))?;
	Ok(id)
}

// Prepares and returns the encoded call data for reserving an ID.
fn prepare_reserve_call_data(chain: &Chain, proxy: &Proxy, cli: &mut impl Cli) -> Result<Vec<u8>> {
	let dispatchable = find_dispatchable_by_name(
		&chain.pallets,
		Action::Reserve.pallet_name(),
		Action::Reserve.function_name(),
	)?;
	let mut xt = Call { function: dispatchable.clone(), use_wallet: true, ..Default::default() }
		.prepare_extrinsic(&chain.client, cli)?;
	if let Some(addr) = proxy {
		xt = construct_proxy_extrinsic(&chain.pallets, addr.to_string(), xt)?;
	}
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
	use crate::cli::MockCli;
	use pop_parachains::decode_call_data;
	use std::fs;
	use tempfile::tempdir;
	use url::Url;

	const POLKADOT_NETWORK_URL: &str = "wss://polkadot-rpc.publicnode.com";
	const POP_NETWORK_TESTNET_URL: &str = "wss://rpc1.paseo.popnetwork.xyz";

	#[tokio::test]
	async fn prepare_for_registration_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_input("Enter the relay chain node URL", POLKADOT_NETWORK_URL.into());
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		let chain_config = UpCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			proxied_address: Some(
				"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string(),
			),
			..Default::default()
		}
		.prepare_for_registration(&mut cli)
		.await?;

		assert_eq!(chain_config.id, 2000);
		assert_eq!(chain_config.genesis_code, genesis_code);
		assert_eq!(chain_config.genesis_state, genesis_state);
		assert_eq!(chain_config.chain.url, Url::parse(POLKADOT_NETWORK_URL)?);
		assert_eq!(
			chain_config.proxy,
			Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string())
		);
		cli.verify()
	}

	#[tokio::test]
	async fn resolve_proxied_address_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_confirm("Would you like to use a proxy for registration? This is considered a best practice.", true)
			.expect_info("Enter the account that the proxy will make a call on behalf of.")
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
		let chain = configure(
			"Enter the relay chain node URL",
			DEFAULT_URL,
			&Some(Url::parse(POLKADOT_NETWORK_URL)?),
			&mut cli,
		)
		.await?;
		let proxied_address = UpCommand::default().resolve_proxied_address(&chain, &mut cli)?;
		assert_eq!(
			proxied_address,
			Some("Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string())
		);
		cli.verify()?;

		cli = MockCli::new().expect_confirm(
			"Would you like to use a proxy for registration? This is considered a best practice.",
			false,
		);
		let proxied_address = UpCommand::default().resolve_proxied_address(&chain, &mut cli)?;
		assert_eq!(proxied_address, None);
		cli.verify()?;

		cli = MockCli::new();
		let proxied_address = UpCommand {
			proxied_address: Some("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".to_string()),
			..Default::default()
		}
		.resolve_proxied_address(&chain, &mut cli)?;
		assert_eq!(
			proxied_address,
			Some("Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string())
		);
		cli.verify()
	}

	#[tokio::test]
	async fn prepare_reserve_call_data_works() -> Result<()> {
		let mut cli = MockCli::new();
		let chain = configure(
			"Enter the relay chain node URL",
			DEFAULT_URL,
			&Some(Url::parse(POLKADOT_NETWORK_URL)?),
			&mut cli,
		)
		.await?;
		let call_data = prepare_reserve_call_data(&chain, &None, &mut cli)?;
		// Encoded call data for a reserve extrinsic.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4605
		let encoded_reserve_extrinsic: &str = "0x4605";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);

		// Ensure `prepare_reserve_call_data` works with a proxy.
		let call_data = prepare_reserve_call_data(
			&chain,
			&Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string()),
			&mut cli,
		)?;
		// Encoded call data for a proxy extrinsic with reserve as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004605
		let encoded_reserve_extrinsic: &str =
			"0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004605";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);
		Ok(())
	}

	#[tokio::test]
	async fn reserve_id_fails_wrong_chain() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Deploy a rollup")
			.expect_info(format!("Reserving an ID. You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.", Url::parse(POP_NETWORK_TESTNET_URL)?.as_str()))
			.expect_outro_cancel("Failed to find the pallet Registrar");
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		UpCommand {
			id: None,
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			relay_chain_url: Some(Url::parse(POP_NETWORK_TESTNET_URL)?),
			path: None,
			proxied_address: None,
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn register_fails_wrong_chain() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Deploy a rollup")
			.expect_info(format!("Registering. You will need to sign a transaction on {} using the `Registrar::register` function.", Url::parse(POP_NETWORK_TESTNET_URL)?.as_str()))
			.expect_outro_cancel("Failed to find the pallet Registrar");
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		UpCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			relay_chain_url: Some(Url::parse(POP_NETWORK_TESTNET_URL)?),
			path: None,
			proxied_address: None,
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn prepare_register_call_data_works() -> Result<()> {
		let mut cli = MockCli::new();
		let chain = configure(
			"Enter the relay chain node URL",
			DEFAULT_URL,
			&Some(Url::parse(POLKADOT_NETWORK_URL)?),
			&mut cli,
		)
		.await?;
		// Create a temporary files to act as genesis_state and genesis_code files.
		let temp_dir = tempdir()?;
		let genesis_state_path = temp_dir.path().join("genesis_state");
		let genesis_code_path = temp_dir.path().join("genesis_code.wasm");
		let mut up_chain = Registration {
			id: 2000,
			genesis_state: genesis_state_path.clone(),
			genesis_code: genesis_code_path.clone(),
			chain,
			proxy: None,
		};

		// Expect failure when the genesis state file cannot be read.
		assert!(matches!(
			up_chain.prepare_register_call_data(&mut cli),
			Err(message) if message.to_string().contains("Failed to read genesis state file")
		));
		std::fs::write(&genesis_state_path, "0x1234")?;

		// Expect failure when the genesis code file cannot be read.
		assert!(matches!(
			up_chain.prepare_register_call_data(&mut cli),
			Err(message) if message.to_string().contains("Failed to read genesis code file")
		));
		std::fs::write(&genesis_code_path, "0x1234")?;

		// Encoded call data for a register extrinsic with the above values.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4600d0070000081234081234
		let encoded_register_extrinsic: &str = "0x4600d0070000081234081234";
		assert_eq!(
			up_chain.prepare_register_call_data(&mut cli)?,
			decode_call_data(encoded_register_extrinsic)?
		);

		// Ensure `prepare_register_call_data` works with a proxy.
		up_chain.proxy = Some("Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string());
		let call_data = up_chain.prepare_register_call_data(&mut cli)?;
		// Encoded call data for a proxy extrinsic with register as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234
		let encoded_reserve_extrinsic: &str = "0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);
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
}
