// SPDX-License-Identifier: GPL-3.0

use crate::wallet_integration::{FrontendFromDir, TransactionData, WalletIntegrationManager};
use cliclack::log;
use sp_core::bytes::to_hex;
use std::path::PathBuf;

pub async fn wait_for_signature(call_data: Vec<u8>, url: String) -> anyhow::Result<Option<String>> {
	// TODO: to be addressed in future PR. Should not use FromDir (or local path).
	let ui = FrontendFromDir::new(PathBuf::from(
		"/Users/alexbean/Documents/react-teleport-example/dist",
	));

	let transaction_data = TransactionData::new(url, call_data);
	let call_data_bytes = to_hex(&transaction_data.call_data(), false);
	println!("transaction_data: {:?}", call_data_bytes);
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
