// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::{
		BuildSpecCommand, ChainType, CodePathBuf, GenesisArtifacts, RelayChain, StatePathBuf,
	},
	call::chain::Call,
	cli::traits::*,
	common::{
		chain::{configure, Chain},
		wallet::submit_extrinsic,
	},
	deployment_api::{DeployRequest, DeployResponse, DeploymentApi},
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
	str::FromStr,
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
	/// Flag to skip the registration step and only attempt deployment.
	#[arg(long, requires = "id")]
	pub(crate) skip_registration: bool,
	/// Path to the chain spec file. If provided, it will be used to generate genesis artifacts.
	#[arg(long = "chain-spec")]
	pub(crate) chain_spec: Option<PathBuf>,
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
	pub(crate) async fn execute(&mut self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Deploy a rollup")?;
		let mut deployment = self.prepare_for_deployment(cli)?;
		let config = match self.prepare_for_registration(&mut deployment, cli).await {
			Ok(chain) => chain,
			Err(e) => {
				cli.outro_cancel(e.to_string())?;
				return Ok(());
			},
		};
		if !self.skip_registration {
			match config.register(cli).await {
				Ok(_) => cli.success(format!(
					"Registration successfully {}",
					style(format!(
						"https://polkadot.js.org/apps/?rpc={}#/parachains",
						config.chain.url
					))
					.dim()
				))?,
				Err(e) => cli.outro_cancel(e.to_string())?,
			}
		}
		match deployment.deploy(&config, cli).await {
			Ok(Some(result)) => cli.success(format!(
				"Deployment successfully\n   {}\n   {}",
				style(format!("{} Status: {}", console::Emoji("●", ">"), result.status)).dim(),
				style(format!(
					"{} View Deployment: {}",
					console::Emoji("●", ">"),
					style(result.message).magenta().underlined()
				))
			))?,
			Ok(None) => {},
			Err(e) => cli.outro_cancel(format!("{}", e))?,
		}
		Ok(())
	}

	// Prepares the chain for deployment by setting up its configuration.
	fn prepare_for_deployment(&mut self, cli: &mut impl Cli) -> Result<Deployment> {
		let provider = match prompt_provider(cli)? {
			Some(provider) => provider,
			None => return Ok(Deployment::default()),
		};
		warn_supported_templates(&provider, cli)?;
		let relay_chain = prompt_supported_chain(cli)?;
		let relay_chain_name = relay_chain.to_string();
		self.relay_chain_url = relay_chain.get_rpc_url().and_then(|url| Url::parse(&url).ok());

		let api_key = prompt_api_key(POP_API_KEY, cli)?;
		let api = Some(DeploymentApi::new(api_key, provider, relay_chain_name)?);
		Ok(Deployment { api, collator_file_id: None })
	}

	// Prepares the chain for registration by setting up its configuration.
	async fn prepare_for_registration(
		&self,
		deployment_config: &mut Deployment,
		cli: &mut impl Cli,
	) -> Result<Registration> {
		let chain =
			configure("Enter the relay chain node URL", DEFAULT_URL, &self.relay_chain_url, cli)
				.await?;
		let proxy = self.resolve_proxied_address(chain.url.as_str(), cli)?;
		let id = self.resolve_id(&chain, &proxy, cli).await?;
		let genesis_artifacts = self.resolve_genesis_files(deployment_config, id, cli).await?;
		Ok(Registration { id, genesis_artifacts, chain, proxy })
	}

	// Retrieves the proxied address, prompting the user if none is specified.
	fn resolve_proxied_address(&self, relay_chain_url: &str, cli: &mut impl Cli) -> Result<Proxy> {
		if let Some(addr) = &self.proxied_address {
			return Ok(parse_account(addr).map(|valid_addr| Some(format!("Id({valid_addr})")))?);
		}
		if cli.confirm("Would you like to use a pure proxy for registration? This is considered a best practice.").initial_value(true).interact()? {
			return Ok(Some(prompt_for_proxy_address(relay_chain_url, cli)?));
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
		deployment_config: &mut Deployment,
		id: u32,
		cli: &mut impl Cli,
	) -> Result<GenesisArtifacts> {
		if let (Some(code), Some(state), _) =
			(&self.genesis_code, &self.genesis_state, &deployment_config.api)
		{
			return Ok(GenesisArtifacts {
				genesis_code_file: Some(code.clone()),
				genesis_state_file: Some(state.clone()),
				..Default::default()
			});
		}
		cli.info("Generating the chain spec for your project")?;
		generate_spec_files(
			self.chain_spec.as_deref(),
			deployment_config,
			id,
			self.path.as_deref(),
			cli,
		)
		.await
	}
}

// Represents the configuration for deployment.
#[derive(Default)]
struct Deployment {
	api: Option<DeploymentApi>,
	collator_file_id: Option<String>,
}
impl Deployment {
	// Executes the deployment process.
	async fn deploy(
		&self,
		config: &Registration,
		cli: &mut impl Cli,
	) -> Result<Option<DeployResponse>> {
		let Some(api) = &self.api else {
			return Ok(None);
		};
		let Some(collator_file_id) = &self.collator_file_id else {
			return Err(anyhow::anyhow!("No collator_file_id was found"));
		};
		let mut request = DeployRequest::new(
			collator_file_id.to_string(),
			config.genesis_artifacts.clone(),
			config.proxy.clone(),
		)?;
		if request.runtime_template.is_none() {
			let template_name = prompt_template_used(cli)?;
			request.runtime_template = Some(template_name.to_string());
		}
		cli.info(format!("Starting deployment with {}", api.provider.name()))?;
		let result = api.deploy(config.id, request).await?;
		Ok(Some(result))
	}
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
	chain_spec: Option<&Path>,
	deployment_config: &mut Deployment,
	id: u32,
	path: Option<&Path>,
	cli: &mut impl Cli,
) -> anyhow::Result<GenesisArtifacts> {
	let chain_spec_path = chain_spec
		.map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()).display().to_string());
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
		chain: chain_spec_path,
		relay: deployment_config
			.api
			.as_ref()
			.map(|api| RelayChain::from_str(&api.relay_chain_name.to_lowercase()).ok())
			.flatten(),
		..Default::default()
	}
	.configure_build_spec(cli)
	.await?;

	let mut genesis_artifacts = build_spec.clone().build(cli)?;
	if let Some(api) = &deployment_config.api {
		cli.info("Fetching collator keys...")?;
		let keys = api.get_collator_keys(id).await?;
		cli.info("Rebuilding chain spec with updated collator keys...")?;
		build_spec
			.update_chain_spec_with_keys(keys.collator_keys, &genesis_artifacts.chain_spec)?;
		build_spec.enable_existing_plain_spec();
		genesis_artifacts = build_spec.build(cli)?;
		deployment_config.collator_file_id = Some(keys.collator_file_id);
	}
	Ok(genesis_artifacts)
}

// Prompt the user to input an address and return it formatted as `Id(address)`
fn prompt_for_proxy_address(relay_chain_url: &str, cli: &mut impl Cli) -> Result<String> {
	cli.info(format!(
		"Don't have a pure proxy?\n{}",
		style(format!("Create a proxy account using `pop call chain --pallet Proxy --function create_pure --args \"Any()\" \"0\" \"0\" --url {relay_chain_url} --use-wallet` and fund it with enough balance for the registration.")).dim()
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

// Prompts the user the user to select a deployment provider or only register.
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

// Prompts user to select a supported chain for deployment.
fn prompt_supported_chain(cli: &mut impl Cli) -> Result<&SupportedChains> {
	let mut chain_selected =
		cli.select("Select a Relay Chain\n\nChoose from the supported relay chains:");
	for chain in SupportedChains::VARIANTS {
		chain_selected = chain_selected.item(chain, chain.to_string(), "");
	}
	Ok(chain_selected.interact()?)
}

// Prompts for an API key and attempts to read from environment first.
fn prompt_api_key(env_var_name: &str, cli: &mut impl Cli) -> Result<String> {
	if let Ok(api_key) = env::var(env_var_name) {
		cli.info(format!("Using API key from environment variable ({env_var_name})."))?;
		return Ok(api_key);
	}
	cli.warning(format!("No API key found for the environment variable `{env_var_name}`.\n{}", style(format!("Note: Consider setting this variable in your shell (e.g., `export {env_var_name}=...`) or system environment so you won’t be prompted each time.")).dim()))?;
	let api_key = cli.password("Enter your API key:").interact()?;
	Ok(api_key)
}

// Prompts the user to choose which template was used.
fn prompt_template_used(cli: &mut impl Cli) -> Result<&str> {
	cli.warning("We could not automatically detect which template was used to build your rollup.")?;
	let mut template = cli.select("Select the template used:");
	for supported_template in Parachain::VARIANTS.iter().filter_map(|variant| {
		variant
			.deployment_name()
			.map(|name| (name, variant.name(), variant.description().trim()))
	}) {
		template = template.item(supported_template.0, supported_template.1, supported_template.2);
	}
	Ok(template.interact()?)
}

/// Warns user about which templates are supported for the given provider.
fn warn_supported_templates(provider: &DeploymentProvider, cli: &mut impl Cli) -> Result<()> {
	let supported_templates: Vec<String> = Parachain::VARIANTS
		.iter()
		.filter_map(|variant| variant.deployment_name().map(|_| variant.name().to_string()))
		.collect();
	cli.warning(format!(
		"Currently {} only supports the deployment of the following templates: {}.\n",
		provider.name(),
		style(supported_templates.join(", ")).dim()
	))?;
	Ok(())
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
			.expect_input("Enter the relay chain node URL", POLKADOT_NETWORK_URL.into());
		let (genesis_state, genesis_code) = create_temp_genesis_files()?;
		let chain_config = UpCommand {
			id: Some(2000),
			genesis_state: Some(genesis_state.clone()),
			genesis_code: Some(genesis_code.clone()),
			proxied_address: Some(MOCK_PROXIED_ADDRESS.to_string()),
			..Default::default()
		}
		.prepare_for_registration(&mut Deployment::default(), &mut cli)
		.await?;

		assert_eq!(chain_config.id, 2000);
		assert_eq!(chain_config.genesis_artifacts.genesis_code_file, Some(genesis_code));
		assert_eq!(chain_config.genesis_artifacts.genesis_state_file, Some(genesis_state));
		assert_eq!(chain_config.chain.url, Url::parse(POLKADOT_NETWORK_URL)?);
		assert_eq!(chain_config.proxy, Some(format!("Id({})", MOCK_PROXIED_ADDRESS.to_string())));
		cli.verify()
	}

	#[tokio::test]
	async fn prepare_for_deployment_works() -> Result<()> {
		let mut cli = MockCli::new().expect_select(
			"Select your deployment method:",
			Some(false),
			true,
			Some(
				DeploymentProvider::VARIANTS
					.into_iter()
					.map(|action| (action.name().to_string(), action.description().to_string()))
					.chain(std::iter::once((
						"Only Register in Relay Chain".to_string(),
						"Register the parachain in the relay chain without deploying".to_string(),
					)))
					.collect::<Vec<_>>(),
			),
			DeploymentProvider::VARIANTS.len(), // Only Register in Relay Chain
			None,
		);
		let chain_config = UpCommand::default().prepare_for_deployment(&mut cli)?;

		assert!(chain_config.api.is_none());
		assert!(chain_config.collator_file_id.is_none());
		cli.verify()?;

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
				DeploymentProvider::PDP as usize,
				None,
			)
			.expect_select(
				"Select a Relay Chain\n\nChoose from the supported relay chains:",
				Some(false),
				true,
				Some(
					SupportedChains::VARIANTS
						.into_iter()
						.map(|chain| (chain.to_string(), "".to_string()))
						.collect::<Vec<_>>(),
				),
				SupportedChains::PASEO as usize,
				None,
			);

		// A backup of the existing env variable to restore it at the end of the test.
		let original_api_key = env::var(POP_API_KEY).ok();
		env::set_var(POP_API_KEY, "test_api_key");
		let chain_config = UpCommand::default().prepare_for_deployment(&mut cli)?;
		assert!(!chain_config.api.is_none());
		let api = chain_config.api.unwrap();
		assert_eq!(api.api_key, "test_api_key");
		assert_eq!(api.relay_chain_name, "PASEO");

		// Ensure a clean environment.
		if let Some(original) = original_api_key {
			env::set_var("POP_API_KEY", original);
		} else {
			env::remove_var("POP_API_KEY");
		}
		cli.verify()
	}

	#[test]
	fn resolve_proxied_address_works() -> Result<()> {
		let relay_chain_url = "ws://127.0.0.1:9944";
		let mut cli = MockCli::new()
			.expect_confirm("Would you like to use a pure proxy for registration? This is considered a best practice.", true)
			.expect_info(format!(
				"Don't have a pure proxy?\n{}",
				style(format!("Create a proxy account using `pop call chain --pallet Proxy --function create_pure --args \"Any()\" \"0\" \"0\" --url {relay_chain_url} --use-wallet` and fund it with enough balance for the registration.")).dim()
			))
			.expect_input(
				"Enter your pure proxy account or the account that the proxy will make a call on behalf of",
				MOCK_PROXIED_ADDRESS.into(),
			);
		let proxied_address =
			UpCommand::default().resolve_proxied_address(relay_chain_url, &mut cli)?;
		assert_eq!(proxied_address, Some(format!("Id({})", MOCK_PROXIED_ADDRESS)));
		cli.verify()?;

		cli = MockCli::new().expect_confirm(
			"Would you like to use a pure proxy for registration? This is considered a best practice.",
			false,
		);
		let proxied_address =
			UpCommand::default().resolve_proxied_address(relay_chain_url, &mut cli)?;
		assert_eq!(proxied_address, None);
		cli.verify()?;

		cli = MockCli::new();
		let proxied_address = UpCommand {
			proxied_address: Some(MOCK_PROXIED_ADDRESS.to_string()),
			..Default::default()
		}
		.resolve_proxied_address(relay_chain_url, &mut cli)?;
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
							"Register the parachain in the relay chain without deploying".to_string(),
						)))
						.collect::<Vec<_>>(),
				),
				DeploymentProvider::VARIANTS.len(), // Only Register in Relay Chain
				None,
			)
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
			..Default::default()
		}
		.execute(&mut cli)
		.await?;

		cli.verify()
	}

	#[tokio::test]
	async fn register_fails_wrong_chain() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_intro("Deploy a rollup")
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
							"Register the parachain in the relay chain without deploying".to_string(),
						)))
						.collect::<Vec<_>>(),
				),
				DeploymentProvider::VARIANTS.len(), // Only Register in Relay Chain
				None,
			)
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
		let test_env_var = "TEST_POP_API_KEY";
		env::remove_var(test_env_var);

		let mut cli = MockCli::new()
			.expect_warning(format!("No API key found for the environment variable `{test_env_var}`.\n{}", style(format!("Note: Consider setting this variable in your shell (e.g., `export {test_env_var}=...`) or system environment so you won’t be prompted each time.")).dim()))
			.expect_password("Enter your API key:", "test_api_key".into());

		let api_key = prompt_api_key(test_env_var, &mut cli)?;
		assert_eq!(api_key, "test_api_key");
		cli.verify()?;

		// Test when API KEY exist in the env variable.
		env::set_var(test_env_var, "test_api_key");
		cli = MockCli::new()
			.expect_info(format!("Using API key from environment variable ({test_env_var})."));

		let api_key = prompt_api_key(test_env_var, &mut cli)?;
		assert_eq!(api_key, "test_api_key");
		cli.verify()
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
						.collect::<Vec<_>>(),
				),
				Parachain::Standard as usize,
				None,
			);
		assert_eq!(prompt_template_used(&mut cli)?, "POP_STANDARD");
		cli.verify()
	}

	#[test]
	fn warn_supported_templates_works() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_warning(
				format!(
					"Currently Polkadot Deployment Portal only supports the deployment of the following templates: {}.\n",
					style(format!("{}", Parachain::VARIANTS
					.iter()
					.filter_map(|variant| variant.deployment_name().map(|_| format!("{}", variant.name())))
					.collect::<Vec<String>>().join(", "))).dim()
				),
			);
		warn_supported_templates(&DeploymentProvider::PDP, &mut cli)?;
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
