// SPDX-License-Identifier: GPL-3.0

use crate::wallet_integration::{FrontendFromString, TransactionData, WalletIntegrationManager};
use cliclack::log;

pub async fn wait_for_signature(call_data: Vec<u8>, url: String) -> anyhow::Result<Option<String>> {
	let ui = FrontendFromString::new(include_str!("../assets/index.html").to_string());

	let transaction_data = TransactionData::new(url, call_data);
	// starts server
	let mut wallet = WalletIntegrationManager::new(ui, transaction_data);
	log::step(format!("Wallet signing portal started at http://{}", wallet.rpc_url))?;

	log::step("Waiting for signature... Press Ctrl+C to terminate early.")?;
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

	let signed_payload = wallet.state.lock().await.signed_payload.clone();
	Ok(signed_payload)
}
