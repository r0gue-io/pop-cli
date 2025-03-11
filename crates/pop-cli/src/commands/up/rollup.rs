// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::{BuildSpecCommand, ChainType, CodePathBuf, GenesisArtifacts, StatePathBuf},
	call::chain::Call,
	cli::traits::*,
	common::{
		chain::{configure, Chain},
		wallet::submit_extrinsic,
	},
	deployment_api::{DeployRequest, DeploymentApi},
	style::style,
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_common::{parse_account, templates::Template};
use pop_parachains::{
	construct_proxy_extrinsic, find_dispatchable_by_name, Action, DeploymentProvider, Parachain,
	Payload, Reserved, SupportedChains,
};
use std::{
	env,
	path::{Path, PathBuf},
};
use strum::VariantArray;
use url::Url;

type Proxy = Option<String>;

const DEFAULT_URL: &str = "wss://paseo.rpc.amforc.com/";
const HELP_HEADER: &str = "Chain deployment options";
const PLACEHOLDER_ADDRESS: &str = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
const POP_API_KEY: &str = "POP_API_KEY";

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
	/// account.
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
				cli.outro_cancel(e.to_string())?;
				return Ok(());
			},
		};
		match config.registration.register(cli).await {
			Ok(_) => cli.success(format!(
				"Registration successfully {}",
				style(format!(
					"https://polkadot.js.org/apps/?rpc={}#/parachains",
					config.registration.chain.url
				))
				.dim()
			))?,
			Err(e) => cli.outro_cancel(e.to_string())?,
		}
		if let Some(api) = config.api {
			let Some(collator_file_id) = config.collator_file_id else {
				cli.outro_cancel("No collator_file_id was found.")?;
				return Ok(());
			};
			let mut request = DeployRequest::new(
				collator_file_id,
				config.registration.genesis_artifacts,
				config.registration.proxy,
			)?;
			if request.runtime_template.is_none() {
				let template_name = prompt_template_used(cli)?;
				request.runtime_template = template_name.map(|name| name.to_string());
			}
			cli.info(format!("Starting deployment with {}", api.provider.name()))?;
			match api.deploy(config.registration.id, request).await {
				Ok(result) => cli.success(format!(
					"Deployment successfully {}",
					style(format!("{}", result.message)).dim()
				))?,
				Err(e) => cli.outro_cancel(format!("{}", e))?,
			}
		}

		Ok(())
	}

	// Prepares the chain for registration by setting up its configuration.
	async fn prepare_for_registration(self, cli: &mut impl Cli) -> Result<Deployment> {
		let mut api = None;
		let mut relay_chain_url = self.relay_chain_url.clone();
		if relay_chain_url.is_none() {
			// TODO: Needs refactoring once we don't manage supporting Local deployments.
			let provider = prompt_provider(cli)?;
			let relay_chain = prompt_supported_chain(cli)?;
			relay_chain_url = relay_chain
				.and_then(|chain| chain.get_rpc_url())
				.and_then(|url| Url::parse(&url).ok());

			if let Some(provider) = provider {
				let api_key = prompt_api_key(cli)?;
				// TODO: As above. Local is only testing
				api = Some(DeploymentApi::new(
					provider,
					api_key,
					relay_chain
						.as_ref()
						.map(ToString::to_string)
						.unwrap_or_else(|| "Local".to_string()),
				)?);
			}
		}
		let chain =
			configure("Enter the relay chain node URL", DEFAULT_URL, &relay_chain_url, cli).await?;
		let proxy = self.resolve_proxied_address(cli)?;
		let id = self.resolve_id(&chain, &proxy, cli).await?;
		let (genesis_artifacts, collator_file_id) =
			self.resolve_genesis_files(&api, id, cli).await?;
		Ok(Deployment {
			api,
			collator_file_id,
			registration: Registration { id, genesis_artifacts, chain, proxy },
		})
	}

	// Retrieves the proxied address, prompting the user if none is specified.
	fn resolve_proxied_address(&self, cli: &mut impl Cli) -> Result<Proxy> {
		if let Some(addr) = &self.proxied_address {
			return Ok(parse_account(addr).map(|valid_addr| Some(format!("Id({valid_addr})")))?);
		}
		if cli.confirm("Would you like to use a pure proxy for registration? This is considered a best practice.").interact()? {
			return Ok(Some(prompt_for_proxy_address(cli)?));
		}
		Ok(None)
	}

	// Resolves the ID, reserving a new one if necessary.
	async fn resolve_id(&self, chain: &Chain, proxy: &Proxy, cli: &mut impl Cli) -> Result<u32> {
		match self.id {
			Some(id) => Ok(id),
			None => {
				cli.info(format!("You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.", chain.url))?;
				reserve(chain, proxy, cli).await
			},
		}
	}
	// Resolves the genesis state and code files, generating them if necessary.
	async fn resolve_genesis_files(
		&self,
		api: &Option<DeploymentApi>,
		id: u32,
		cli: &mut impl Cli,
	) -> Result<(GenesisArtifacts, Option<String>)> {
		if let (Some(code), Some(state), _) = (&self.genesis_code, &self.genesis_state, &api) {
			return Ok((
				GenesisArtifacts {
					genesis_code_file: Some(code.clone()),
					genesis_state_file: Some(state.clone()),
					..Default::default()
				},
				None,
			)); // Ignore the collator file ID and the chain spec files.
		}
		cli.info("Generating the chain spec for your project")?;
		generate_spec_files(api, id, self.path.as_deref(), cli).await
	}
}

// Represents the configuration for deployment.
struct Deployment {
	api: Option<DeploymentApi>,
	collator_file_id: Option<String>,
	registration: Registration,
}

// Represents the configuration for rollup registration.
struct Registration {
	id: u32,
	genesis_artifacts: GenesisArtifacts,
	chain: Chain,
	proxy: Proxy,
}
impl Registration {
	// Registers by submitting an extrinsic.
	async fn register(&self, cli: &mut impl Cli) -> Result<()> {
		cli.info(format!("You will need to sign a transaction to register on {}, using the `Registrar::register` function.", self.chain.url))?;
		let call_data = self.prepare_register_call_data(cli)?;
		submit_extrinsic(&self.chain.client, &self.chain.url, call_data, cli)
			.await
			.map_err(|e| anyhow::anyhow!("Registration failed: {}", e))?;
		Ok(())
	}

	// Prepares and returns the encoded call data for registering a chain.
	fn prepare_register_call_data(&self, cli: &mut impl Cli) -> Result<Vec<u8>> {
		let Registration { id, genesis_artifacts, chain, proxy, .. } = self;
		let dispatchable = find_dispatchable_by_name(
			&chain.pallets,
			Action::Register.pallet_name(),
			Action::Register.function_name(),
		)?;
		let state = std::fs::read_to_string(
			genesis_artifacts
				.genesis_state_file
				.as_ref()
				.ok_or_else(|| anyhow::anyhow!("Failed to generate the genesis state file."))?,
		)
		.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
		let code = std::fs::read_to_string(
			genesis_artifacts
				.genesis_code_file
				.as_ref()
				.ok_or_else(|| anyhow::anyhow!("Failed to generate the genesis code."))?,
		)
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
	api: &Option<DeploymentApi>,
	id: u32,
	path: Option<&Path>,
	cli: &mut impl Cli,
) -> anyhow::Result<(GenesisArtifacts, Option<String>)> {
	// Changes the working directory if a path is provided to ensure the build spec process runs in
	// the correct context.
	if let Some(path) = path {
		std::env::set_current_dir(path)?;
	}
	let mut build_spec = BuildSpecCommand {
		id: Some(id),
		genesis_code: true,
		genesis_state: true,
		chain_type: Some(ChainType::Live),
		..Default::default()
	}
	.configure_build_spec(cli)
	.await?;

	let mut genesis_artifacts = build_spec.clone().build(cli)?;
	let mut collator_file_id = None;
	if let Some(api) = api {
		cli.info("Fetching collator keys...")?;
		let keys = api.get_collator_keys(id).await?;
		cli.info("Rebuilding chain spec with updated collator keys...")?;
		build_spec
			.update_chain_spec_with_keys(keys.collator_keys, &genesis_artifacts.chain_spec)?;
		build_spec.skip_plain_chain_spec = true;
		genesis_artifacts = build_spec.build(cli)?;
		collator_file_id = Some(keys.collator_file_id);
	}
	Ok((genesis_artifacts, collator_file_id))
}

// Prompt the user to input an address and return it formatted as `Id(address)`
fn prompt_for_proxy_address(cli: &mut impl Cli) -> Result<String> {
	cli.info(format!(
		"Don't have a pure proxy yet? \n{}",
		style("Create one: `pop call chain` and fund it").dim()
	))?;
	let address = cli
	.input("Enter your pure proxy account or the account that the proxy will make a call on behalf of")
		.placeholder(&format!("e.g {}", PLACEHOLDER_ADDRESS))
		.validate(|input: &String| match parse_account(input) {
			Ok(_) => Ok(()),
			Err(_) => Err("Invalid address."),
		})
		.interact()?;
	Ok(format!("Id({address})"))
}

// Prompts the user for what action they want to do.
fn prompt_provider(cli: &mut impl Cli) -> Result<Option<DeploymentProvider>> {
	let mut predefined_action = cli.select("Select your deployment method:");
	for action in DeploymentProvider::VARIANTS {
		predefined_action =
			predefined_action.item(Some(action.clone()), action.name(), action.description());
	}
	predefined_action = predefined_action.item(
		None,
		"Only Register in Relay Chain",
		"Register the parachain in the relay chain without deploying",
	);
	Ok(predefined_action.interact()?)
}

// Prompts the user for what action they want to do.
fn prompt_supported_chain(cli: &mut impl Cli) -> Result<Option<SupportedChains>> {
	let mut chain_selected =
		cli.select("Select a Relay Chain\n\nChoose from the supported relay chains:");
	for chain in SupportedChains::VARIANTS {
		chain_selected = chain_selected.item(Some(chain.clone()), chain.to_string(), "");
	}
	// TODO: Remove after local testing.
	//if provider.is_none() {
	chain_selected =
		chain_selected.item(None, "CUSTOM", "You will be asked to enter the URL manually.");
	//}
	Ok(chain_selected.interact()?)
}

// Prompts for an API key and stores it securely.
fn prompt_api_key(cli: &mut impl Cli) -> Result<String> {
	if let Ok(api_key) = env::var(POP_API_KEY) {
		cli.info(format!("Using API key from environment variable ({POP_API_KEY})."))?;
		return Ok(api_key);
	}
	cli.warning(format!("No API key found. You can set the `{POP_API_KEY}` environment variable or enter it manually."))?;
	let api_key = cli.password("Enter your API key:").interact()?;
	if cli
		.confirm("Do you want to set this API key as an environment variable for this session?")
		.interact()?
	{
		env::set_var("POP_API_KEY", &api_key);
	}
	Ok(api_key)
}

// Prompts the user to select the template used.
fn prompt_template_used(cli: &mut impl Cli) -> Result<Option<&str>> {
	cli.warning("We could not automatically detect which template was used to build your rollup.")?;
	let mut template = cli.select("Select the template used:");
	for supported_template in
		Parachain::VARIANTS.iter().filter(|variant| variant.deployment_name().is_some())
	{
		template = template.item(
			supported_template.deployment_name(),
			supported_template.name(),
			supported_template.description().trim(),
		);
	}
	template = template.item(None, "None of the above", "Proceed without a predefined template.");
	Ok(template.interact()?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_parachains::decode_call_data;
	use std::fs;
	use tempfile::tempdir;
	use url::Url;

	const MOCK_PROXIED_ADDRESS: &str = "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty";
	const MOCK_PROXY_ADDRESS_ID: &str = "Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)";
	const POLKADOT_NETWORK_URL: &str = "wss://polkadot-rpc.publicnode.com";
	const POP_NETWORK_TESTNET_URL: &str = "wss://rpc1.paseo.popnetwork.xyz";

	#[tokio::test]
	async fn prepare_for_registration_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_select(
				"Select your deployment method:",
				Some(false),
				true,
				Some(
					DeploymentProvider::VARIANTS
						.into_iter()
						.map(|action| (action.name().to_string(), action.description().to_string()))
						.chain(std::iter::once((
							"Only Register in Relay Chain".to_string(),
							"Register the parachain in the relay chain without deploying"
								.to_string(),
						)))
						.collect::<Vec<_>>(),
				),
				DeploymentProvider::VARIANTS.len(), // Only Register in Relay Chain
			)
			.expect_select(
				"Select a Relay Chain\n\nChoose from the supported relay chains:",
				Some(false),
				true,
				Some(
					SupportedChains::VARIANTS
						.into_iter()
						.map(|chain| (chain.to_string(), "".to_string()))
						.chain(std::iter::once((
							"CUSTOM".to_string(),
							"You will be asked to enter the URL manually.".to_string(),
						)))
						.collect::<Vec<_>>(),
				),
				SupportedChains::VARIANTS.len(),
				None,
			)
			.expect_input("Enter the relay chain node URL", POLKADOT_NETWORK_URL.into());
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

		assert_eq!(chain_config.registration.id, 2000);
		assert_eq!(
			chain_config.registration.genesis_artifacts.genesis_code_file,
			Some(genesis_code)
		);
		assert_eq!(
			chain_config.registration.genesis_artifacts.genesis_state_file,
			Some(genesis_state)
		);
		assert_eq!(chain_config.registration.chain.url, Url::parse(POLKADOT_NETWORK_URL)?);
		assert_eq!(
			chain_config.registration.proxy,
			Some(format!("Id({})", MOCK_PROXIED_ADDRESS.to_string()))
		);
		assert!(chain_config.api.is_none());
		assert!(chain_config.collator_file_id.is_none());
		cli.verify()
	}

	#[test]
	fn resolve_proxied_address_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_confirm("Would you like to use a pure proxy for registration? This is considered a best practice.", true)
			.expect_info(format!(
				"Don't have a pure proxy yet? \n{}",
				style("Create one: `pop call chain` and fund it").dim()
			))
			.expect_input(
				"Enter your pure proxy account or the account that the proxy will make a call on behalf of",
				MOCK_PROXIED_ADDRESS.into(),
			);
		let proxied_address = UpCommand::default().resolve_proxied_address(&mut cli)?;
		assert_eq!(proxied_address, Some(format!("Id({})", MOCK_PROXIED_ADDRESS)));
		cli.verify()?;

		cli = MockCli::new().expect_confirm(
			"Would you like to use a pure proxy for registration? This is considered a best practice.",
			false,
		);
		let proxied_address = UpCommand::default().resolve_proxied_address(&mut cli)?;
		assert_eq!(proxied_address, None);
		cli.verify()?;

		cli = MockCli::new();
		let proxied_address = UpCommand {
			proxied_address: Some(MOCK_PROXIED_ADDRESS.to_string()),
			..Default::default()
		}
		.resolve_proxied_address(&mut cli)?;
		assert_eq!(proxied_address, Some(format!("Id({})", MOCK_PROXIED_ADDRESS)));
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
		let call_data =
			prepare_reserve_call_data(&chain, &Some(MOCK_PROXY_ADDRESS_ID.to_string()), &mut cli)?;
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
			.expect_info(format!("You will need to sign a transaction to reserve an ID on {} using the `Registrar::reserve` function.", Url::parse(POP_NETWORK_TESTNET_URL)?.as_str()))
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
			.expect_info(format!("You will need to sign a transaction to register on {}, using the `Registrar::register` function.", Url::parse(POP_NETWORK_TESTNET_URL)?.as_str()))
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
			genesis_artifacts: GenesisArtifacts {
				genesis_state_file: Some(genesis_state_path.clone()),
				genesis_code_file: Some(genesis_code_path.clone()),
				..Default::default()
			},
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
		up_chain.proxy = Some(MOCK_PROXY_ADDRESS_ID.to_string());
		let call_data = up_chain.prepare_register_call_data(&mut cli)?;
		// Encoded call data for a proxy extrinsic with register as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.public.curie.radiumblock.co%2Fws#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234
		let encoded_reserve_extrinsic: &str = "0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de004600d0070000081234081234";
		assert_eq!(call_data, decode_call_data(encoded_reserve_extrinsic)?);
		Ok(())
	}

	#[test]
	fn prompt_api_key_works() -> Result<()> {
		// A backup of the existing env variable to restore it at the end of the test.
		let original_api_key = env::var(POP_API_KEY).ok();

		env::remove_var(POP_API_KEY); // Remove the environment variable for the test
		let mut cli = MockCli::new()
			.expect_warning(format!("No API key found. You can set the `{POP_API_KEY}` environment variable or enter it manually."))
			.expect_password("Enter your API key:", "test_api_key".into())
			.expect_confirm("Do you want to set this API key as an environment variable for this session?", true);

		let api_key = prompt_api_key(&mut cli)?;
		assert_eq!(api_key, "test_api_key");
		cli.verify()?;

		// Test when API KEY exist in the env variable.
		cli = MockCli::new()
			.expect_info(format!("Using API key from environment variable ({POP_API_KEY})."));

		let api_key = prompt_api_key(&mut cli)?;
		assert_eq!(api_key, "test_api_key");
		cli.verify()?;

		// Restore the original `POP_API_KEY` if it existed before the test, otherwise, remove it to
		// ensure a clean environment.
		if let Some(original) = original_api_key {
			env::set_var("POP_API_KEY", original);
		} else {
			env::remove_var("POP_API_KEY");
		}
		Ok(())
	}

	#[tokio::test]
	async fn prompt_template_used_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_warning(
				"We could not automatically detect which template was used to build your rollup.",
			)
			.expect_select(
				"Select the template used:",
				Some(false),
				true,
				Some(
					Parachain::VARIANTS
						.iter()
						.filter(|variant| variant.deployment_name().is_some())
						.map(|template| {
							(template.name().to_string(), template.description().trim().to_string())
						})
						.chain(std::iter::once((
							"None of the above".to_string(),
							"Proceed without a predefined template.".to_string(),
						)))
						.collect::<Vec<_>>(),
				),
				Parachain::Standard as usize,
				None,
			);
		assert_eq!(prompt_template_used(&mut cli)?, Some("POP_STANDARD"));
		cli.verify()
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
