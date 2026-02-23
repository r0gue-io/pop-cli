// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::{BuildSpecCommand, ChainType, CodePathBuf, GenesisArtifacts, StatePathBuf},
	call::chain::Call,
	cli::traits::*,
	common::{
		chain::{Chain, configure},
		urls,
		wallet::submit_extrinsic,
	},
	style::style,
};
use anyhow::Result;
use clap::Args;
use pop_chains::{Action, Payload, Reserved, construct_proxy_extrinsic, find_callable_by_name};
use pop_common::Profile;
use serde::Serialize;
use std::{
	env,
	path::{Path, PathBuf},
};
use url::Url;

type Proxy = Option<String>;

const HELP_HEADER: &str = "Chain registration options";

#[derive(Args, Clone, Default, Serialize)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpCommand {
	/// Path to the project.
	#[serde(skip_serializing)]
	#[clap(skip)]
	pub(crate) path: PathBuf,
	/// ID to use. If not specified, a new ID will be reserved.
	#[arg(short, long)]
	pub(crate) id: Option<u32>,
	/// Path to the chain spec file. If provided, it will be used to generate genesis artifacts.
	#[serde(skip_serializing)]
	#[arg(long = "chain-spec")]
	pub(crate) chain_spec: Option<PathBuf>,
	/// Path to the genesis state file. If not specified, it will be generated.
	#[serde(skip_serializing)]
	#[arg(short = 'G', long = "genesis-state")]
	pub(crate) genesis_state: Option<StatePathBuf>,
	/// Path to the genesis code file.  If not specified, it will be generated.
	#[serde(skip_serializing)]
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: Option<CodePathBuf>,
	/// Websocket endpoint of the relay chain.
	#[arg(long)]
	pub(crate) relay_chain_url: Option<Url>,
	/// Proxied address. Your account must be registered as a proxy which can act on behalf of this
	/// account.
	#[arg(long = "proxy")]
	pub(crate) proxied_address: Option<String>,
	/// Build profile [default: release].
	#[clap(long, value_enum)]
	pub(crate) profile: Option<Profile>,
}

impl UpCommand {
	/// Executes the command.
	pub(crate) async fn execute(&mut self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Register a chain")?;
		let config = match self.prepare_for_registration(cli).await {
			Ok(chain) => chain,
			Err(e) => {
				cli.outro_cancel(e.to_string())?;
				return Ok(());
			},
		};
		match config.register(cli).await {
			Ok(_) => cli.success(format!(
				"Registration successful {}",
				style(format!(
					"https://polkadot.js.org/apps/?rpc={}#/parachains",
					config.chain.url
				))
				.dim()
			))?,
			Err(e) => {
				cli.outro_cancel(format!("{}", e))?;
				return Ok(());
			},
		}
		cli.info(self.display())?;
		Ok(())
	}

	// Prepares the chain for registration by setting up its configuration.
	async fn prepare_for_registration(&self, cli: &mut impl Cli) -> Result<Registration> {
		let chain = configure(
			"Select a chain (type to filter)",
			"Enter the relay chain node URL",
			urls::LOCAL,
			&self.relay_chain_url,
			|node| node.is_relay,
			cli,
		)
		.await?;
		let proxy = self.resolve_proxied_address();
		let id = self.resolve_id(&chain, &proxy, cli).await?;
		let genesis_artifacts = self.resolve_genesis_files(id, cli).await?;
		Ok(Registration { id, genesis_artifacts, chain, proxy })
	}

	// Retrieves the proxied address if specified via CLI flag.
	fn resolve_proxied_address(&self) -> Proxy {
		self.proxied_address.as_ref().map(|addr| format!("Id({addr})"))
	}

	// Resolves the ID, reserving a new one if necessary.
	async fn resolve_id(&self, chain: &Chain, proxy: &Proxy, cli: &mut impl Cli) -> Result<u32> {
		match self.id {
			Some(id) => Ok(id),
			None => {
				cli.info(format!(
					"You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.",
					chain.url
				))?;
				reserve(chain, proxy, cli).await
			},
		}
	}
	// Resolves the genesis state and code files, generating them if necessary.
	async fn resolve_genesis_files(&self, id: u32, cli: &mut impl Cli) -> Result<GenesisArtifacts> {
		// If both genesis code & state exist, there's no need to generate the chain spec.
		if self.genesis_code.is_some() && self.genesis_state.is_some() {
			return Ok(GenesisArtifacts {
				genesis_code_file: self.genesis_code.clone(),
				genesis_state_file: self.genesis_state.clone(),
				..Default::default()
			});
		}
		cli.info("Generating the chain spec for your project")?;
		generate_spec_files(self.chain_spec.as_deref(), id, &self.path, self.profile, cli).await
	}

	fn display(&self) -> String {
		let mut full_message = "pop up rollup".to_string();
		if let Some(id) = self.id {
			full_message.push_str(&format!(" --id {}", id));
		}
		if let Some(chain_spec) = &self.chain_spec {
			full_message.push_str(&format!(" --chain-spec {}", chain_spec.display()));
		}
		if let Some(genesis_state) = &self.genesis_state {
			full_message.push_str(&format!(" --genesis-state {}", genesis_state.display()));
		}
		if let Some(genesis_code) = &self.genesis_code {
			full_message.push_str(&format!(" --genesis-code {}", genesis_code.display()));
		}
		if let Some(url) = &self.relay_chain_url {
			full_message.push_str(&format!(" --relay-chain-url {}", url));
		}
		if let Some(proxy) = &self.proxied_address {
			full_message.push_str(&format!(" --proxy {}", proxy));
		}
		if let Some(profile) = self.profile {
			full_message.push_str(&format!(" --profile {}", profile));
		}
		full_message
	}
}

// Represents the configuration for chain registration.
struct Registration {
	id: u32,
	genesis_artifacts: GenesisArtifacts,
	chain: Chain,
	proxy: Proxy,
}
impl Registration {
	// Registers by submitting an extrinsic.
	async fn register(&self, cli: &mut impl Cli) -> Result<()> {
		cli.info(format!(
			"You will need to sign a transaction to register on {}, using the `Registrar::register` function.",
			self.chain.url
		))?;
		let call_data = self.prepare_register_call_data(cli)?;
		submit_extrinsic(&self.chain.client, &self.chain.url, call_data, cli)
			.await
			.map_err(|e| anyhow::anyhow!("Registration failed: {}", e))?;
		Ok(())
	}

	// Prepares and returns the encoded call data for registering a chain.
	fn prepare_register_call_data(&self, cli: &mut impl Cli) -> Result<Vec<u8>> {
		let Registration { id, genesis_artifacts, chain, proxy, .. } = self;
		let dispatchable = find_callable_by_name(
			&chain.pallets,
			Action::Register.pallet_name(),
			Action::Register.function_name(),
		)?;
		let state = genesis_artifacts.read_genesis_state()?;
		let code = genesis_artifacts.read_genesis_code()?;
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
	let dispatchable = find_callable_by_name(
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
	chain_spec: Option<&Path>,
	id: u32,
	path: &Path,
	profile: Option<Profile>,
	cli: &mut impl Cli,
) -> Result<GenesisArtifacts> {
	let chain_spec_path = chain_spec
		.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()).display().to_string());
	// Changes the working directory if a path is provided to ensure the build spec process runs in
	// the correct context.
	if !path.as_os_str().is_empty() {
		env::set_current_dir(path)?;
	}

	let build_spec = BuildSpecCommand {
		para_id: Some(id),
		genesis_code: Some(true),
		genesis_state: Some(true),
		chain_type: Some(ChainType::Live),
		chain: chain_spec_path,
		profile: profile.or(Some(Profile::Release)),
		..Default::default()
	}
	.configure_build_spec(cli)
	.await?;

	build_spec.build(cli, false).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::MockCli, common::urls};
	use pop_chains::decode_call_data;
	use pop_common::test_env::shared_substrate_ws_url;
	use std::fs;
	use tempfile::tempdir;
	use url::Url;

	const MOCK_PROXIED_ADDRESS: &str = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
	const MOCK_PROXY_ADDRESS_ID: &str = "Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)";

	#[test]
	fn test_up_command_display() {
		let cmd = UpCommand {
			path: PathBuf::from("./my-chain"),
			id: Some(2000),
			chain_spec: Some(PathBuf::from("chain-spec.json")),
			genesis_state: Some(StatePathBuf::from("genesis-state")),
			genesis_code: Some(CodePathBuf::from("genesis-code")),
			relay_chain_url: Some(Url::parse("ws://localhost:9944").unwrap()),
			proxied_address: Some("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".to_string()),
			profile: Some(Profile::Release),
		};
		assert_eq!(
			cmd.display(),
			"pop up rollup --id 2000 --chain-spec chain-spec.json --genesis-state genesis-state --genesis-code genesis-code --relay-chain-url ws://localhost:9944/ --proxy 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty --profile release"
		);

		let cmd = UpCommand { path: PathBuf::from("./"), ..Default::default() };
		assert_eq!(cmd.display(), "pop up rollup");
	}

	#[tokio::test]
	async fn prepare_for_registration_works() -> Result<()> {
		let node_url = shared_substrate_ws_url().await;
		let mut cli = MockCli::new()
			.expect_select(
				"Select a chain (type to filter)".to_string(),
				Some(true),
				true,
				Some(vec![
					("Local".to_string(), "Local node (ws://localhost:9944)".to_string()),
					("Custom".to_string(), "Type the chain URL manually".to_string()),
				]),
				1,
				None,
			)
			.expect_input("Enter the relay chain node URL", node_url.clone());
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		let chain_config = UpCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			proxied_address: Some(MOCK_PROXIED_ADDRESS.to_string()),
			..Default::default()
		}
		.prepare_for_registration(&mut cli)
		.await?;

		assert_eq!(chain_config.id, 2000);
		assert_eq!(chain_config.genesis_artifacts.genesis_code_file, Some(genesis_code));
		assert_eq!(chain_config.genesis_artifacts.genesis_state_file, Some(genesis_state));
		assert_eq!(chain_config.chain.url, Url::parse(&node_url)?);
		assert_eq!(chain_config.proxy, Some(format!("Id({})", MOCK_PROXIED_ADDRESS)));
		cli.verify()
	}

	#[test]
	fn resolve_proxied_address_works() -> Result<()> {
		let proxied_address = UpCommand {
			proxied_address: Some(MOCK_PROXIED_ADDRESS.to_string()),
			..Default::default()
		}
		.resolve_proxied_address();
		assert_eq!(proxied_address, Some(format!("Id({})", MOCK_PROXIED_ADDRESS)));

		let proxied_address = UpCommand::default().resolve_proxied_address();
		assert_eq!(proxied_address, None);
		Ok(())
	}

	#[tokio::test]
	async fn register_fails_wrong_chain() -> Result<()> {
		let node_url = shared_substrate_ws_url().await;
		let mut cli = MockCli::new()
			.expect_intro("Register a chain")
			.expect_info(format!(
				"You will need to sign a transaction to register on {}, using the `Registrar::register` function.",
				Url::parse(&node_url)?.as_str()
			))
			.expect_outro_cancel("Registration failed: Failed to find the pallet: Registrar");
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		UpCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			relay_chain_url: Some(Url::parse(&node_url)?),
			path: PathBuf::from("./"),
			proxied_address: None,
			..Default::default()
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn prepare_register_call_data_works() -> Result<()> {
		let mut cli = MockCli::new();
		let chain = configure(
			"Select a relay chain",
			"Enter the relay chain node URL",
			urls::LOCAL,
			&Some(Url::parse(urls::POLKADOT)?),
			|node| node.is_relay,
			&mut cli,
		)
		.await?;
		// Create a temporary files to act as genesis_state and genesis_code files.
		let temp_dir = tempdir()?;
		let genesis_state_path = temp_dir.path().join("genesis_state");
		fs::write(&genesis_state_path, "0x1234")?;
		let genesis_code_path = temp_dir.path().join("genesis_code.wasm");
		fs::write(&genesis_code_path, "0x1234")?;
		let mut up_chain = Registration {
			id: 2000,
			genesis_artifacts: GenesisArtifacts {
				genesis_state_file: Some(genesis_state_path.clone()),
				genesis_code_file: Some(genesis_code_path.clone()),
				..Default::default()
			},
			chain,
			proxy: None,
		};

		// Encoded call data for a register extrinsic with the above values.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4600d0070000081234081234
		let encoded_register_extrinsic: &str = "0x4600d0070000081234081234";
		assert_eq!(
			up_chain.prepare_register_call_data(&mut cli)?,
			decode_call_data(encoded_register_extrinsic)?
		);

		// Ensure `prepare_register_call_data` works with a proxy.
		up_chain.proxy = Some(MOCK_PROXY_ADDRESS_ID.to_string());
		let call_data = up_chain.prepare_register_call_data(&mut cli)?;
		// Encoded call data for a proxy extrinsic with register as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234
		let encoded_reserve_extrinsic: &str = "0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);
		Ok(())
	}

	#[tokio::test]
	async fn reserve_id_fails_wrong_chain() -> Result<()> {
		let node_url = shared_substrate_ws_url().await;
		let mut cli = MockCli::new()
			.expect_intro("Register a chain")
			.expect_info(format!(
				"You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.",
				Url::parse(&node_url)?.as_str()
			))
			.expect_outro_cancel("Failed to find the pallet: Registrar");
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		UpCommand {
			id: None,
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			relay_chain_url: Some(Url::parse(&node_url)?),
			path: PathBuf::from("./"),
			proxied_address: None,
			..Default::default()
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn prepare_reserve_call_data_works() -> Result<()> {
		let mut cli = MockCli::new();
		let chain = configure(
			"Select a relay chain",
			"Enter the relay chain node URL",
			urls::LOCAL,
			&Some(Url::parse(urls::POLKADOT)?),
			|node| node.is_relay,
			&mut cli,
		)
		.await?;
		let call_data = prepare_reserve_call_data(&chain, &None, &mut cli)?;
		// Encoded call data for a reserve extrinsic.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x4605
		let encoded_reserve_extrinsic: &str = "0x4605";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);

		// Ensure `prepare_reserve_call_data` works with a proxy.
		let call_data =
			prepare_reserve_call_data(&chain, &Some(MOCK_PROXY_ADDRESS_ID.to_string()), &mut cli)?;
		// Encoded call data for a proxy extrinsic with reserve as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004605
		let encoded_reserve_extrinsic: &str =
			"0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004605";
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
