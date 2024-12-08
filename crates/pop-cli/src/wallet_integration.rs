use axum::{
	http::HeaderValue,
	response::Html,
	routing::{get, post},
	Router,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::{
	sync::{oneshot, Mutex},
	task::JoinHandle,
};
use tower_http::{cors::Any, services::ServeDir};

/// Make frontend sourcing more flexible by allowing a custom route
/// to be defined.
pub trait Frontend {
	fn serve_content(&self) -> Router;
}

/// Transaction payload to be sent to frontend for signing.
#[derive(Serialize, Debug)]
#[cfg_attr(test, derive(Deserialize))]
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
	// signed payload received from UI.
	pub signed_payload: Option<String>,
	pub error: Option<String>,
}

/// Manages the wallet integration for secure signing of transactions.
pub struct WalletIntegrationManager {
	// shared state between routes.
	pub state: Arc<Mutex<StateHandler>>,
	// node rpc address
	pub addr: String,
	// axum server task handle
	pub task_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl WalletIntegrationManager {
	/// Launches a server for hosting the wallet integration. Server launched in separate task.
	/// Uses default address of 127.0.0.1:9090.
	/// # Arguments
	/// * `frontend`: A frontend with custom route to serve content.
	/// * `payload`: Payload to be sent to the frontend for signing.
	///
	/// # Returns
	/// A `WalletIntegrationManager` instance, with access to the state and task handle for the
	/// server.
	pub fn new<F: Frontend>(frontend: F, payload: TransactionData) -> Self {
		Self::new_with_address(frontend, payload, "127.0.0.1:9090")
	}

	/// Same as `new`, but allows specifying the address to bind to.
	pub fn new_with_address<F: Frontend>(
		frontend: F,
		payload: TransactionData,
		addr: &str,
	) -> Self {
		// channel to signal shutdown
		let (tx, rx) = oneshot::channel();

		let state = Arc::new(Mutex::new(StateHandler {
			shutdown_tx: Some(tx),
			signed_payload: None,
			error: None,
		}));

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
			let listener = tokio::net::TcpListener::bind(&addr_owned)
				.await
				.map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr_owned, e))?;

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

	/// Signals the wallet integration server to shut down.
	pub async fn terminate(&mut self) {
		// signal shutdown
		if let Some(shutdown_tx) = self.state.lock().await.shutdown_tx.take() {
			let _ = shutdown_tx.send(());
		}
	}

	/// Checks if the server task is still running.
	pub fn is_running(&self) -> bool {
		!self.task_handle.is_finished()
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

	// must be public for axum
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
		// error should never occur.
		let json_payload = serde_json::to_value(&*payload)
			.map_err(|e| anyhow::anyhow!("Failed to serialize payload: {}", e))?;
		Ok(Json(json_payload))
	}

	/// Receives the signed payload from the wallet.
	/// Will signal for shutdown on success.
	pub(super) async fn submit_handler(
		State(state): State<Arc<Mutex<StateHandler>>>,
		Json(payload): Json<String>,
	) -> Json<serde_json::Value> {
		let mut state = state.lock().await;
		state.signed_payload = Some(payload.clone());

		// Signal shutdown.
		// Using WalletIntegrationManager::terminate() introduces unnecessary complexity.
		if let Some(shutdown_tx) = state.shutdown_tx.take() {
			let _ = shutdown_tx.send(());
		}

		// graceful shutdown ensures response is sent before shutdown.
		Json(json!({"status": "success"}))
	}

	/// Receives an error message from the wallet.
	pub(super) async fn error_handler(
		State(state): State<Arc<Mutex<StateHandler>>>,
		Json(error): Json<String>,
	) {
		let mut state = state.lock().await;
		state.error = Some(error);
	}

	/// Allows the server to be terminated from the frontend.
	pub(super) async fn terminate_handler(State(state): State<Arc<Mutex<StateHandler>>>) {
		let mut state = state.lock().await;
		if let Some(shutdown_tx) = state.shutdown_tx.take() {
			let _ = shutdown_tx.send(());
		}
	}
}

/// Serves static files from a directory.
pub struct FrontendFromDir {
	content: PathBuf,
}

#[allow(dead_code)]
impl FrontendFromDir {}

impl Frontend for FrontendFromDir {
	fn serve_content(&self) -> Router {
		Router::new().nest_service("/", ServeDir::new(self.content.clone()))
	}
}

/// Serves a hard-coded HTML string as the frontend.
pub struct FrontendFromString {
	content: String,
}

impl FrontendFromString {
	pub fn new(content: String) -> Self {
		Self { content }
	}
}

impl Frontend for FrontendFromString {
	fn serve_content(&self) -> Router {
		let content = self.content.clone();
		Router::new().route("/", get(move || async { Html(content) }))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	const TEST_HTML: &str = "<html><body>Hello, world!</body></html>";

	// wait for server to launch
	async fn wait() {
		tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
	}

	#[tokio::test]
	async fn new_works() {
		let frontend = FrontendFromString::new(TEST_HTML.to_string());
		let payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![] };
		let mut wim = WalletIntegrationManager::new(frontend, payload);

		assert_eq!(wim.addr, "127.0.0.1:9090");
		assert_eq!(wim.is_running(), true);
		assert!(wim.state.lock().await.shutdown_tx.is_some());
		assert!(wim.state.lock().await.signed_payload.is_none());

		// terminate the server and make sure result is ok
		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn payload_handler_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9091";
		let frontend = FrontendFromString::new(TEST_HTML.to_string());

		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let addr = format!("http://{}", wim.addr);
		let actual_payload = reqwest::get(&format!("{}/payload", addr))
			.await
			.expect("Failed to get payload")
			.json::<TransactionData>()
			.await
			.expect("Failed to parse payload");

		assert_eq!(actual_payload.chain_rpc, "localhost:9944");
		assert_eq!(actual_payload.call_data, vec![1, 2, 3]);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn submit_handler_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9092";
		let frontend = FrontendFromString::new(TEST_HTML.to_string());

		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let addr = format!("http://{}", wim.addr);
		let response = reqwest::Client::new()
			.post(&format!("{}/submit", addr))
			.json(&"0xDEADBEEF")
			.send()
			.await
			.expect("Failed to submit payload")
			.json::<serde_json::Value>()
			.await
			.expect("Failed to parse JSON response");

		assert_eq!(response, json!({"status": "success"}));
		assert_eq!(wim.state.lock().await.signed_payload, Some("0xDEADBEEF".to_string()));
		assert_eq!(wim.is_running(), false);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn error_handler_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9093";
		let frontend = FrontendFromString::new(TEST_HTML.to_string());

		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let addr = format!("http://{}", wim.addr);
		let response = reqwest::Client::new()
			.post(&format!("{}/error", addr))
			.json(&"an error occurred")
			.send()
			.await
			.expect("Failed to submit error")
			.text()
			.await
			.expect("Failed to parse response");

		// no response expected
		assert_eq!(response.len(), 0);
		assert_eq!(wim.state.lock().await.error, Some("an error occurred".to_string()));
		assert_eq!(wim.is_running(), true);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn terminate_handler_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9094";
		let frontend = FrontendFromString::new(TEST_HTML.to_string());

		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let addr = format!("http://{}", wim.addr);
		let response = reqwest::Client::new()
			.post(&format!("{}/terminate", addr))
			.send()
			.await
			.expect("Failed to terminate")
			.text()
			.await
			.expect("Failed to parse response");

		// no response expected
		assert_eq!(response.len(), 0);
		assert_eq!(wim.is_running(), false);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn wallet_terminate_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9095";

		let frontend = FrontendFromString::new(TEST_HTML.to_string());

		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);

		assert_eq!(wim.is_running(), true);
		wim.terminate().await;
		wait().await;
		assert_eq!(wim.is_running(), false);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn frontend_from_string_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9096";

		let frontend = FrontendFromString::new(TEST_HTML.to_string());
		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let actual_payload = reqwest::get(&format!("http://{}", addr))
			.await
			.expect("Failed to get web page")
			.text()
			.await
			.expect("Failed to parse page");

		assert_eq!(actual_payload, TEST_HTML);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn frontend_from_dir_works() {
		use std::fs;
		use tempfile::tempdir;

		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9097";

		let temp_dir = tempdir().expect("Failed to create temp directory");
		let index_file_path = temp_dir.path().join("index.html");

		let test_html = "<html><body>Hello, world from Directory!</body></html>";
		fs::write(&index_file_path, test_html).expect("Failed to write index.html");

		let frontend = FrontendFromDir::new(temp_dir.path().to_path_buf());
		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let actual_payload = reqwest::get(&format!("http://{}", addr))
			.await
			.expect("Failed to get web page")
			.text()
			.await
			.expect("Failed to parse page");

		assert_eq!(actual_payload, test_html);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn large_payload_works() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9098";
		let frontend = FrontendFromString::new(TEST_HTML.to_string());

		let call_data_5mb = vec![99u8; 5 * 1024 * 1024];

		let expected_payload = TransactionData {
			chain_rpc: "localhost:9944".to_string(),
			call_data: call_data_5mb.clone(),
		};
		let mut wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		let addr = format!("http://{}", wim.addr);
		let actual_payload = reqwest::get(&format!("{}/payload", addr))
			.await
			.expect("Failed to get payload")
			.json::<TransactionData>()
			.await
			.expect("Failed to parse payload");

		assert_eq!(actual_payload.chain_rpc, "localhost:9944");
		assert_eq!(actual_payload.call_data, call_data_5mb);

		wim.terminate().await;
		assert!(wim.task_handle.await.is_ok());
	}

	#[tokio::test]
	async fn new_with_conflicting_address_fails() {
		// offset port per test to avoid conflicts
		let addr = "127.0.0.1:9099";

		let frontend = FrontendFromString::new(TEST_HTML.to_string());
		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let wim = WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		assert_eq!(wim.is_running(), true);

		let frontend = FrontendFromString::new(TEST_HTML.to_string());
		let expected_payload =
			TransactionData { chain_rpc: "localhost:9944".to_string(), call_data: vec![1, 2, 3] };
		let wim_conflict =
			WalletIntegrationManager::new_with_address(frontend, expected_payload, addr);
		wait().await;

		assert_eq!(wim_conflict.is_running(), false);
		let task_result = wim_conflict.task_handle.await.unwrap();
		match task_result {
			Err(e) => assert!(e
				.to_string()
				.starts_with(&format!("Failed to bind to {}: Address already in use", addr))),
			Ok(_) => panic!("Expected error, but task succeeded"),
		}
	}
}
