// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::{BuildSpec, BuildSpecCommand},
	call::chain::{submit_extrinsic_with_wallet, Chain},
	cli::{self, traits::*},
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, find_dispatchable_by_name, parse_chain_metadata, set_up_client, Action,
	Payload,
};

use std::path::PathBuf;
use url::Url;

const DEFAULT_URL: &str = "wss://paseo.rpc.amforc.com/";
const HELP_HEADER: &str = "Parachain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpParachainCommand {
	/// Path to the chain directory.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
	/// Parachain ID to use. If not specified, a new ID will be reserved.
	#[arg(short, long)]
	pub(crate) id: Option<u32>,
	/// Path to the genesis state file.
	#[arg(short = 'G', long = "genesis-state")]
	pub(crate) genesis_state: Option<PathBuf>,
	/// Path to the genesis code file.
	#[arg(short = 'C', long = "genesis-code")]
	pub(crate) genesis_code: Option<PathBuf>,
	/// Websocket endpoint of the relay chain.
	#[arg(long)]
	pub(crate) relay_url: Option<Url>,
}

impl UpParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Deploy a parachain")?;
		let chain = self.configure_chain(cli).await?;
		let para_id = match self.id {
			Some(id) => id,
			None => {
				cli.info("Reserving a parachain ID...")?;
				match reserve_para_id(&chain, cli).await {
					Ok(id) => id,
					Err(e) => {
						cli.outro_cancel(&format!("Failed to reserve parachain ID: {}", e))?;
						return Err(e);
					},
				}
			},
		};
		let (genesis_state, genesis_code) =
			match (self.genesis_state.clone(), self.genesis_code.clone()) {
				(Some(state), Some(code)) => (state, code),
				_ => {
					cli.info("Generating the chain spec for your parachain, some extra information is needed:")?;
					match generate_spec_files(para_id, self.path, cli).await {
						Ok(files) => files,
						Err(e) => {
							cli.outro_cancel(&format!("Failed to generate spec files: {}", e))?;
							return Err(e);
						},
					}
				},
			};
		cli.info("Registering a parachain ID")?;
		if let Err(e) = register_parachain(&chain, para_id, genesis_state, genesis_code, cli).await
		{
			cli.outro_cancel(&format!("Failed to register parachain: {}", e))?;
			return Err(e);
		}

		cli.outro("Parachain deployment complete.")?;
		Ok(())
	}

	// Configures the chain by resolving the URL and fetching its metadata.
	async fn configure_chain(&self, cli: &mut impl Cli) -> Result<Chain> {
		// Resolve url.
		let url = match &self.relay_url {
			Some(url) => url.clone(),
			None => {
				// Prompt for url.
				let url: String = cli
					.input("Enter the relay chain node URL to deploy your parachain:")
					.default_input(DEFAULT_URL)
					.interact()?;
				Url::parse(&url)?
			},
		};

		// Parse metadata from chain url.
		let client = set_up_client(url.as_str()).await?;
		let pallets = parse_chain_metadata(&client).map_err(|e| {
			anyhow!(format!("Unable to fetch the chain metadata: {}", e.to_string()))
		})?;
		Ok(Chain { url, client, pallets })
	}
}

/// Reserves a parachain ID by submitting an extrinsic.
async fn reserve_para_id(chain: &Chain, cli: &mut impl Cli) -> Result<u32> {
	let call_data = prepare_reserve_para_id_extrinsic(chain)?;
	submit_extrinsic_with_wallet(&chain.client, &chain.url, call_data, cli).await?;
	Ok(2000)
}

/// Constructs an extrinsic for reserving a parachain ID.
fn prepare_reserve_para_id_extrinsic(chain: &Chain) -> Result<Vec<u8>> {
	let function = find_dispatchable_by_name(
		&chain.pallets,
		Action::Reserve.pallet_name(),
		Action::Reserve.function_name(),
	)?;
	let xt = construct_extrinsic(function, Vec::new())?;
	Ok(xt.encode_call_data(&chain.client.metadata())?)
}

/// Generates chain spec files for the parachain.
async fn generate_spec_files(
	id: u32,
	path: Option<PathBuf>,
	cli: &mut impl Cli,
) -> anyhow::Result<(PathBuf, PathBuf)> {
	change_working_directory(&path)?;
	let build_spec = configure_build_spec(id, cli).await?;
	build_spec.generate_genesis_artifacts(cli)
}

/// Changes the working directory if a path is provided, ensuring the build spec process runs in the
/// correct context.
fn change_working_directory(path: &Option<PathBuf>) -> Result<()> {
	if let Some(path) = path {
		std::env::set_current_dir(path)?;
	}
	Ok(())
}

/// Configures the chain specification requirements.
async fn configure_build_spec(id: u32, cli: &mut impl Cli) -> Result<BuildSpec> {
	Ok(BuildSpecCommand {
		id: Some(id),
		genesis_code: true,
		genesis_state: true,
		..Default::default()
	}
	.configure_build_spec(cli)
	.await?)
}

/// Registers a parachain by submitting an extrinsic.
async fn register_parachain(
	chain: &Chain,
	id: u32,
	genesis_state: PathBuf,
	genesis_code: PathBuf,
	cli: &mut impl Cli,
) -> Result<()> {
	let call_data = prepare_register_parachain_extrinsic(chain, id, genesis_state, genesis_code)?;
	submit_extrinsic_with_wallet(&chain.client, &chain.url, call_data, cli).await?;
	Ok(())
}

/// Constructs an extrinsic for registering a parachain.
fn prepare_register_parachain_extrinsic(
	chain: &Chain,
	id: u32,
	genesis_state: PathBuf,
	genesis_code: PathBuf,
) -> Result<Vec<u8>> {
	let ex = find_dispatchable_by_name(
		&chain.pallets,
		Action::Register.pallet_name(),
		Action::Register.function_name(),
	)?;
	let state = std::fs::read_to_string(genesis_state)
		.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
	let code = std::fs::read_to_string(genesis_code)
		.map_err(|err| anyhow!("Failed to read genesis state file: {}", err.to_string()))?;
	let xt = construct_extrinsic(ex, vec![id.to_string(), state, code])?;
	Ok(xt.encode_call_data(&chain.client.metadata())?)
}
