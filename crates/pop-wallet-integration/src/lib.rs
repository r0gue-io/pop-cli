use axum::{
	routing::{get, post},
	Router,
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{oneshot, Mutex};
use tower_http::services::ServeDir;

/// Data to be sent to frontend for signing.
#[derive(Serialize)]
pub struct Data {
	chain_rpc: String,
	data_type: DataType,
}

/// The type of transaction with specific data for signing (contract or parachain).
#[derive(Serialize)]
pub enum DataType {
	/// Parachain call, where Vec<u8> is the encoded call data.
	Parachain(Vec<u8>),
	Contract(ContractArgs),
}

/// Contract specific data variations.
pub mod contracts {
	use super::Serialize;
	#[derive(Serialize)]
	pub struct ContractArgs {
		call_type: ContractCallType,
		storage_deposit_limit: Option<String>,
		/// The binary (wasm, polkavm) of the contract.
		code: Option<Vec<u8>>,
	}

	#[derive(Serialize)]
	pub enum ContractCallType {
		// no unique fields.
		Upload,
		Instantiate(InstantiateArgs),
		Call(CallArgs),
	}

	/// Arguments for instantiating a contract.
	#[derive(Serialize)]
	pub struct InstantiateArgs {
		constructor: String,
		args: Vec<String>,
		value: String,
		gas_limit: Option<u64>,
		proof_size: Option<u64>,
		salt: Option<Vec<u8>>,
	}

	/// Arguments for calling a contract.
	#[derive(Serialize)]
	pub struct CallArgs {
		address: String,
		message: String,
		args: Vec<String>,
		value: String,
		gas_limit: Option<u64>,
		proof_size: Option<u64>,
	}
}
use crate::contracts::ContractArgs;

struct StateHandler {
	shutdown_tx: Option<oneshot::Sender<()>>,
	signed_payload: Option<String>,
}

/// Manages the wallet integration for secure signing of transactions.
pub struct WalletIntegrationManager {
	frontend_path: PathBuf,
	// cloning can be expensive (e.g. contract code stored in-memory)
	data: Arc<Data>,
	signed_payload: Option<String>,
}

impl WalletIntegrationManager {
	/// - frontend_path: Path to the wallet-integration frontend.
	/// - data: Data to be sent to the frontend for signing.
	pub fn new(frontend_path: PathBuf, data: Data) -> Self {
		Self { frontend_path, data: Arc::new(data), signed_payload: Default::default() }
	}

	/// Serves the wallet-integration frontend and an API for the wallet to get
	/// the necessary data and submit the signed payload.
	pub async fn run(&mut self) {
		// used to signal shutdown.
		let (tx, rx) = oneshot::channel();

		// shared state between routes. Will be used to store the signed payload.
		let state =
			Arc::new(Mutex::new(StateHandler { shutdown_tx: Some(tx), signed_payload: None }));

		// will shutdown when the signed payload is received
		let app = Router::new()
			// cloning Arcs is cheap
			.route("/data", get(routes::get_data_handler).with_state(self.data.clone()))
			.route("/submit", post(routes::handle_submit).with_state(state.clone()))
			.nest_service("/", ServeDir::new(self.frontend_path.clone()));

		let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
		axum::serve(listener, app)
			.with_graceful_shutdown(async move {
				let _ = rx.await.ok();
			})
			.await
			.unwrap();

		self.signed_payload = state.lock().await.signed_payload.take();
	}
}

mod routes {
	use super::{Arc, Data, Mutex, StateHandler};
	use axum::{extract::State, Json};
	use serde_json::json;

	/// Responds with the serialized JSON data for signing.
	pub(super) async fn get_data_handler(State(data): State<Arc<Data>>) -> Json<serde_json::Value> {
		Json(serde_json::to_value(&*data).unwrap())
	}

	/// Receives the signed payload from the wallet.
	/// Will signal for shutdown on success.
	pub(super) async fn handle_submit(
		State(state): State<Arc<Mutex<StateHandler>>>,
		Json(payload): Json<String>,
	) -> Json<serde_json::Value> {
		let mut state = state.lock().await;
		state.signed_payload = Some(payload.clone());

		// signal shutdown
		if let Some(shutdown_tx) = state.shutdown_tx.take() {
			let _ = shutdown_tx.send(());
		}

		// graceful shutdown ensures response is sent before shutdown.
		Json(json!({"status": "success", "payload": payload}))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn wallet_integration_manager() {
		let path = PathBuf::from("/Users/peter/dev/r0gue/react-teleport-example/dist");
		let data =
			Data { chain_rpc: "chain_rpc".to_string(), data_type: DataType::Parachain(vec![]) };
		let mut wim = WalletIntegrationManager::new(path, data);

		wim.run().await;

		println!("{:?}", wim.signed_payload);
	}
}
