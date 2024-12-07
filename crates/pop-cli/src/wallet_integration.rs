use axum::{
	http::HeaderValue,
	response::Html,
	routing::{get, post},
	Router,
};
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tokio::{
	sync::{oneshot, Mutex},
	task::JoinHandle,
};
use tower_http::{cors::Any, services::ServeDir};

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

impl TransactionData {
	pub fn new(chain_rpc: String, call_data: Vec<u8>) -> Self {
		Self { chain_rpc, call_data }
	}
	#[allow(dead_code)]
	pub fn call_data(&self) -> Vec<u8> {
		self.call_data.clone()
	}
}

/// Shared state between routes. Serves two purposes:
/// - Maintains a channel to signal shutdown to the main app.
/// - Stores the signed payload received from the wallet.
#[derive(Default)]
pub struct StateHandler {
	shutdown_tx: Option<oneshot::Sender<()>>,
	pub signed_payload: Option<String>,
}

/// Manages the wallet integration for secure signing of transactions.
pub struct WalletIntegrationManager {
	pub state: Arc<Mutex<StateHandler>>,
	pub addr: String,
	pub task_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl WalletIntegrationManager {
	/// - frontend: A frontend with custom route to serve content.
	/// - payload: Payload to be sent to the frontend for signing.
	pub fn new<F: Frontend>(frontend: F, payload: TransactionData) -> Self {
		// Self { frontend, payload: Arc::new(payload), state: Default::default() }
		let (tx, rx) = oneshot::channel();

		let state =
			Arc::new(Mutex::new(StateHandler { shutdown_tx: Some(tx), signed_payload: None }));

		// TODO: temporary until we host from here.
		let cors = tower_http::cors::CorsLayer::new()
			.allow_origin("http://localhost:9090".parse::<HeaderValue>().unwrap())
			.allow_origin("http://127.0.0.1:9090".parse::<HeaderValue>().unwrap())
			.allow_methods(Any) // Allow any HTTP method
			.allow_headers(Any); // Allow any headers (like 'Content-Type')

		let app = Router::new()
			.route("/payload", get(routes::get_payload_handler).with_state(payload))
			.route("/submit", post(routes::submit_handler).with_state(state.clone()))
			.route("/error", post(routes::error_handler).with_state(state.clone()))
			.route("/terminate", post(routes::terminate_handler).with_state(state.clone()))
			.merge(frontend.serve_content()) // Custom route for serving frontend.
			.layer(cors);

		let addr = "127.0.0.1:9090";

		// will shut down when the signed payload is received
		let task_handle = tokio::spawn(async move {
			let listener = tokio::net::TcpListener::bind(addr)
				.await
				.map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

			axum::serve(listener, app)
				.with_graceful_shutdown(async move {
					let _ = rx.await.ok();
				})
				.await
				.map_err(|e| anyhow::anyhow!("Server encountered an error: {}", e))?;
			Ok(())
		});

		Self { state, addr: addr.to_string(), task_handle }
	}

	pub async fn terminate(&mut self) {
		// signal shutdown
		if let Some(shutdown_tx) = self.state.lock().await.shutdown_tx.take() {
			let _ = shutdown_tx.send(());
		}
	}

	pub fn is_running(&self) -> bool {
		!self.task_handle.is_finished()
	}

	pub async fn finish(self) -> anyhow::Result<()> {
		self.task_handle.await?
	}
}

mod routes {
	use super::{Arc, Mutex, StateHandler, TransactionData};
	use anyhow::Error;
	use axum::{
		extract::State,
		http::StatusCode,
		response::{IntoResponse, Response},
		Json,
	};
	use serde_json::json;

	pub struct ApiError(Error);

	impl From<Error> for ApiError {
		fn from(err: Error) -> Self {
			ApiError(err)
		}
	}

	// Implementing IntoResponse for ApiError allows us to return it directly from a route handler.
	impl IntoResponse for ApiError {
		fn into_response(self) -> Response {
			let body = json!({
				"error": self.0.to_string(),
			});
			(StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
		}
	}

	/// Responds with the serialized JSON data for signing.
	pub(super) async fn get_payload_handler(
		State(payload): State<Arc<TransactionData>>,
	) -> Result<Json<serde_json::Value>, ApiError> {
		let json_payload = serde_json::to_value(&*payload)
			.map_err(|e| anyhow::anyhow!("Failed to serialize payload: {}", e))?;
		Ok(Json(json_payload))
	}

	/// Receives the signed payload from the wallet.
	/// Will signal for shutdown on success.
	pub(super) async fn handle_submit(
		State(state): State<Arc<Mutex<StateHandler>>>,
		Json(payload): Json<String>,
	) -> Json<serde_json::Value> {
		let mut state = state.lock().await;
		state.signed_payload = Some(payload.clone());

		// signal shutdown.
		// TODO: decide if we want to shutdown on submit, or at some other time.
		// Using WalletIntegrationManager::terminate() introduces complexity unnecessary for a TODO.
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

#[allow(dead_code)]
impl FrontendFromDir {}

impl Frontend for DefaultFrontend {
	fn serve_content(&self) -> Router {
		Router::new().nest_service("/", ServeDir::new(self.content.clone()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn new_works() {
		let path = PathBuf::from("/path/to/frontend");
		let default_frontend = DefaultFrontend::new(path.clone());
		let data = TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![] };
		let wim = WalletIntegrationManager::new(default_frontend, data);

		assert_eq!(wim.frontend.content, path);
		assert_eq!(wim.payload.chain_rpc, "localhost:9944");
		assert_eq!(wim.payload.call_data, vec![] as Vec<u8>);
		assert!(wim.state.lock().await.shutdown_tx.is_none());
		assert!(wim.state.lock().await.signed_payload.is_none());
	}
}
