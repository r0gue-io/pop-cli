// SPDX-License-Identifier: GPL-3.0

use crate::{
	build::spec::BuildSpecCommand,
	call::chain::Chain,
	cli::{self, traits::*},
	common::wallet::request_signature,
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, find_dispatchable_by_name, parse_chain_metadata, set_up_client,
	submit_signed_extrinsic, Action, OnlineClient, Payload, SubstrateConfig,
};

use std::path::PathBuf;
use url::Url;

const DEFAULT_URL: &str = "wss://paseo.rpc.amforc.com/";
const HELP_HEADER: &str = "Parachain deployment options";

#[derive(Args, Clone, Default)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpParachainCommand {
	/// Path to the contract build directory.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
	/// Parachain ID to be used when generating the chain spec files.
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
		let para_id = self.id.unwrap_or(reserve_para_id(chain, cli).await?);
		let (genesis_state, genesis_code) =
			match (self.genesis_state.clone(), self.genesis_code.clone()) {
				(Some(state), Some(code)) => (state, code),
				_ => generate_spec_files(para_id, cli).await?,
			};
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
		let mut pallets = parse_chain_metadata(&client).map_err(|e| {
			anyhow!(format!("Unable to fetch the chain metadata: {}", e.to_string()))
		})?;
		// Sort by name for display.
		pallets.sort_by(|a, b| a.name.cmp(&b.name));
		pallets.iter_mut().for_each(|p| p.functions.sort_by(|a, b| a.name.cmp(&b.name)));
		Ok(Chain { url, client, pallets })
	}
}

async fn reserve_para_id(chain: Chain, cli: &mut impl Cli) -> Result<u32> {
	cli.info("Reserving a parachain ID for your parachain...")?;
	let ex = find_dispatchable_by_name(
		&chain.pallets,
		Action::Reserve.pallet_name(),
		Action::Reserve.function_name(),
	)?;
	let xt = construct_extrinsic(ex, Vec::new())?;
	let call_data = xt.encode_call_data(&chain.client.metadata())?;
	submit_extrinsic_with_wallet(&chain.client, &chain.url, call_data, cli).await?;
	Ok(2000)
}

async fn generate_spec_files(
	id: u32,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<(PathBuf, PathBuf)> {
	cli.info("Generating the chain spec for your parachain—some extra information is needed:")?;
	let build_spec = BuildSpecCommand { id: Some(id), ..Default::default() }
		.configure_build_spec(cli)
		.await?;
	build_spec.generate_genesis_artifacts(cli)
}

// Sign and submit an extrinsic using wallet integration.
pub async fn submit_extrinsic_with_wallet(
	client: &OnlineClient<SubstrateConfig>,
	url: &Url,
	call_data: Vec<u8>,
	cli: &mut impl Cli,
) -> Result<()> {
	let maybe_payload = request_signature(call_data, url.to_string()).await?;
	if let Some(payload) = maybe_payload {
		cli.success("Signed payload received.")?;
		let spinner = cliclack::spinner();
		spinner.start(
			"Submitting the extrinsic and then waiting for finalization, please be patient...",
		);

		let result = submit_signed_extrinsic(client.clone(), payload)
			.await
			.map_err(|err| anyhow!("{}", format!("{err:?}")))?;

		spinner.stop(format!("Extrinsic submitted with hash: {:?}", result));
	} else {
		cli.outro_cancel("No signed payload received.")?;
	}
	Ok(())
}
