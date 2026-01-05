// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::{Cli, Confirm},
	wallet_integration::{FrontendFromString, TransactionData, WalletIntegrationManager},
};
use cliclack::{log, spinner};
#[cfg(feature = "chain")]
use {
	anyhow::{Result, anyhow},
	pop_chains::{
		ExtrinsicEvents, OnlineClient, SubstrateConfig, parse_and_format_events,
		submit_signed_extrinsic,
	},
	url::Url,
};

/// The prompt to ask the user if they want to use the wallet for signing.
pub const USE_WALLET_PROMPT: &str = "Do you want to use your browser wallet to sign the extrinsic? (Selecting 'No' will prompt you to manually enter the secret key URI for signing, e.g., '//Alice')";

/// Launches the wallet integration for in-browser signing. Blocks until the signature is received.
///
/// # Arguments
/// * `call_data` - The call data to be signed.
/// * `url` - Chain rpc.
/// # Returns
/// * The signed payload, if it exists.
pub async fn request_signature(call_data: Vec<u8>, rpc: String) -> anyhow::Result<Option<String>> {
	let ui = FrontendFromString::new(include_str!("../assets/index.html").to_string());

	let transaction_data = TransactionData::new(rpc, call_data);
	// Starts server with port 9090.
	let mut wallet = WalletIntegrationManager::new(ui, transaction_data, Some(9090));
	let url = format!("http://{}", &wallet.server_url);
	log::step(format!("Wallet signing portal started at {url}."))?;

	let spinner = spinner();
	spinner.start(format!("Opening browser to {url}"));
	if let Err(e) = open::that(url) {
		spinner.error(format!("Failed to launch browser. Please open link manually. {e}"));
	}

	spinner.start("Waiting for signature... Press Ctrl+C to terminate early.");
	loop {
		// Display error, if any.
		if let Some(error) = wallet.take_error().await {
			log::error(format!("Signing portal error: {error}"))?;
		}

		let state = wallet.state.lock().await;
		// If the payload is submitted we terminate the frontend.
		if !wallet.is_running() || state.signed_payload.is_some() {
			wallet.task_handle.await??;
			break;
		}
	}
	spinner.clear();

	let signed_payload = wallet.state.lock().await.signed_payload.take();
	Ok(signed_payload)
}

/// Prompts the user to use the wallet for signing.
/// # Arguments
/// * `cli` - The CLI instance.
/// # Returns
/// * `true` if the user wants to use the wallet, `false` otherwise.
pub fn prompt_to_use_wallet(cli: &mut impl Cli) -> anyhow::Result<bool> {
	if cli.confirm(USE_WALLET_PROMPT).initial_value(true).interact()? {
		Ok(true)
	} else {
		Ok(false)
	}
}

// Sign and submit an extrinsic using wallet integration.
#[cfg(feature = "chain")]
pub(crate) async fn submit_extrinsic(
	client: &OnlineClient<SubstrateConfig>,
	url: &Url,
	call_data: Vec<u8>,
	cli: &mut impl Cli,
) -> Result<ExtrinsicEvents<SubstrateConfig>> {
	let maybe_payload = request_signature(call_data, url.to_string()).await?;
	let payload = maybe_payload.ok_or_else(|| anyhow!("No signed payload received."))?;
	cli.success("Signed payload received.")?;
	let spinner = spinner();
	spinner.start("Submitting the extrinsic and waiting for finalization, please be patient...");

	let result = submit_signed_extrinsic(client.clone(), payload)
		.await
		.map_err(anyhow::Error::from)?;

	let events = parse_and_format_events(client, url, &result).await?;
	spinner.stop(format!(
		"Extrinsic submitted with hash: {:?}\n{}",
		result.extrinsic_hash(),
		events
	));
	Ok(result)
}
