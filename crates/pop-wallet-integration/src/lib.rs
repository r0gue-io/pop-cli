use axum::{
	response::Html,
	routing::{get, post},
	Router,
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{oneshot, Mutex};
use tower_http::services::ServeDir;

/// Make frontend sourcing more flexible by allowing a custom route
/// to be defined. For example, sourcing frontend from a cached directory,
/// or simply an HTML string built-in to the binary.
pub trait Frontend {
	fn serve_content(&self) -> Router;
}

/// Transaction payload to be sent to frontend for signing.
#[derive(Serialize, Debug)]
pub struct TransactionData {
	chain_rpc: String,
	call_data: Vec<u8>,
}

// Shared state between routes. Serves two purposes:
// - Maintains a channel to signal shutdown to the main app.
// - Stores the signed payload received from the wallet.
struct StateHandler {
	shutdown_tx: Option<oneshot::Sender<()>>,
	signed_payload: Option<String>,
}

/// Manages the wallet integration for secure signing of transactions.
pub struct WalletIntegrationManager<F: Frontend> {
	frontend: F,
	// Cloning can be expensive (e.g. contract code in payload). Better to use Arc to avoid this.
	payload: Arc<TransactionData>,
	// The final payload signed by the wallet. Updated after server closure.
	signed_payload: Option<String>,
}

impl<F: Frontend> WalletIntegrationManager<F> {
	/// - frontend: A frontend with custom route to serve content.
	/// - payload: Payload to be sent to the frontend for signing.
	pub fn new(frontend: F, payload: TransactionData) -> Self {
		Self { frontend, payload: Arc::new(payload), signed_payload: Default::default() }
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
			.route("/payload", get(routes::get_payload_handler).with_state(self.payload.clone()))
			.route("/submit", post(routes::handle_submit).with_state(state.clone()))
			.merge(self.frontend.serve_content()); // custom route for serving frontend

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
	use super::{Arc, Mutex, StateHandler, TransactionData};
	use axum::{extract::State, Json};
	use serde_json::json;

	/// Responds with the serialized JSON data for signing.
	pub(super) async fn get_payload_handler(
		State(payload): State<Arc<TransactionData>>,
	) -> Json<serde_json::Value> {
		Json(serde_json::to_value(&*payload).unwrap())
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
		Json(json!({"status": "success"}))
	}
}

/// Default frontend. Current implementation serves static files from a directory.
pub struct DefaultFrontend {
	content: PathBuf,
}
impl DefaultFrontend {
	pub fn new(content: PathBuf) -> Self {
		Self { content }
	}
}

impl Frontend for DefaultFrontend {
	fn serve_content(&self) -> Router {
		Router::new().nest_service("/", ServeDir::new(self.content.clone()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn new_works() {
		let path = PathBuf::from("/path/to/frontend");
		let default_frontend = DefaultFrontend::new(path.clone());
		let data = TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![] };
		let wim = WalletIntegrationManager::new(default_frontend, data);

		assert_eq!(wim.frontend.content, path);
		assert_eq!(wim.payload.chain_rpc, "localhost:9944");
		assert_eq!(wim.signed_payload, None);
	}
}
