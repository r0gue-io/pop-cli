// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::{Cli, Confirm},
	wallet_integration::{
		FrontendFromString, SubmitRequest, TransactionData, WalletIntegrationManager,
	},
};
use cliclack::{log, spinner};
use sp_core::{bytes::from_hex, sr25519::Signature};
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
/// * The signed payload and the associated contract address, if provided by the wallet.
pub async fn request_signature(call_data: Vec<u8>, rpc: String) -> Result<SubmitRequest> {
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
	let contract_address = wallet.state.lock().await.contract_address.take();

	Ok(SubmitRequest { signed_payload, contract_address })
}

pub async fn request_remote_signature_inner(url: &str, payload: &[u8]) -> Result<Signature> {
	let signature_hex = request_signature(payload.to_vec(), url.to_string())
		.await?
		.signed_payload
		.ok_or_else(|| anyhow::anyhow!("No signed payload received"))?;
	let signature_bytes = from_hex(&signature_hex)?;

	let array: [u8; 64] = signature_bytes[..64].try_into()?;
	Ok(Signature::from_raw(array))
}

pub fn request_remote_signature(url: &str, payload: &[u8]) -> Signature {
	let url_str = url.to_string();
	let payload = payload.to_vec();
	tokio::task::block_in_place(|| {
		tokio::runtime::Handle::current()
			.block_on(request_remote_signature_inner(&url_str, &payload))
			.expect("remote signature creation failed")
	})
}

/// Prompts the user to use the wallet for signing.
/// # Arguments
/// * `cli` - The CLI instance.
/// * `skip_confirm` - Whether to skip the confirmation prompt.
/// # Returns
/// * `true` if the user wants to use the wallet, `false` otherwise.
pub fn prompt_to_use_wallet(cli: &mut impl Cli, skip_confirm: bool) -> Result<bool> {
	if skip_confirm {
		return Ok(true);
	}

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
	let maybe_payload = request_signature(call_data, url.to_string()).await?.signed_payload;
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
