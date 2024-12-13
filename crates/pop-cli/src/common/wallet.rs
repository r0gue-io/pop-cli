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

#[ignore]
#[cfg(test)]
mod tests {
	use super::*;
	use subxt::utils::to_hex;

	// TODO: delete this test.
	// This is a helper test for an actual running pop CLI.
	// It can serve as the "frontend" to query the payload, sign it
	// and submit back to the CLI.
	#[tokio::test]
	async fn sign_call_data() -> anyhow::Result<()> {
		use subxt::{config::DefaultExtrinsicParamsBuilder as Params, tx::Payload};
		// This struct implements the [`Payload`] trait and is used to submit
		// pre-encoded SCALE call data directly, without the dynamic construction of transactions.
		struct CallData(Vec<u8>);

		impl Payload for CallData {
			fn encode_call_data_to(
				&self,
				_: &subxt::Metadata,
				out: &mut Vec<u8>,
			) -> Result<(), subxt::ext::subxt_core::Error> {
				out.extend_from_slice(&self.0);
				Ok(())
			}
		}

		use subxt_signer::sr25519::dev;
		let payload = reqwest::get(&format!("{}/payload", "http://127.0.0.1:9090"))
			.await
			.expect("Failed to get payload")
			.json::<TransactionData>()
			.await
			.expect("Failed to parse payload");

		let url = "ws://localhost:9944";
		let rpc_client = subxt::backend::rpc::RpcClient::from_url(url).await?;
		let client =
			subxt::OnlineClient::<subxt::SubstrateConfig>::from_rpc_client(rpc_client).await?;

		let signer = dev::alice();

		let payload = CallData(payload.call_data());
		let ext_params = Params::new().build();
		let signed = client.tx().create_signed(&payload, &signer, ext_params).await?;

		let response = reqwest::Client::new()
			.post(&format!("{}/submit", "http://localhost:9090"))
			.json(&to_hex(signed.encoded()))
			.send()
			.await
			.expect("Failed to submit payload")
			.text()
			.await
			.expect("Failed to parse JSON response");

		Ok(())
	}
}
