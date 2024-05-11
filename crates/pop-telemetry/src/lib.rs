// SPDX-License-Identifier: GPL-3.0
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
	fs::{create_dir_all, File},
	io,
	io::{Read, Write},
	path::PathBuf,
};
use thiserror::Error;

const ENDPOINT: &str = "https://insights.onpop.io/api/send";
const WEBSITE_ID: &str = "0cbea0ba-4752-45aa-b3cd-8fd11fa722f7";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Error, Debug)]
pub enum TelemetryError {
	#[error("a reqwest error occurred: {0}")]
	NetworkError(reqwest::Error),
	#[error("io error occurred: {0}")]
	IO(io::Error),
	#[error("opt-out has been set, can not report metrics")]
	OptedOut,
	#[error("unable to find config file")]
	ConfigFileNotFound,
	#[error("serialization failed: {0}")]
	SerializeFailed(String),
}

pub type Result<T> = std::result::Result<T, TelemetryError>;

#[derive(Debug, Clone)]
pub struct Telemetry {
	// Endpoint to the telemetry API.
	// This should include the domain and api path (e.g. localhost:3000/api/send)
	endpoint: String,
	// Has the user opted-out to anonymous telemetry
	opt_out: bool,
	// Reqwest client
	client: Client,
}

impl Telemetry {
	/// Create a new Telemetry instance.
	///
	/// parameters:
	/// `config_path`: the path to the configuration file (used for opt-out checks)
	pub fn new(config_path: PathBuf) -> Self {
		Self::init(ENDPOINT.to_string(), config_path)
	}

	/// Initialize a new Telemetry instance with parameters.
	/// Can be used in tests to provide mock endpoints.
	/// parameters:
	/// `endpoint`: the API endpoint that telemetry will call
	///	`config_path`: the path to the configuration file (used for opt-out checks)
	fn init(endpoint: String, config_path: PathBuf) -> Self {
		let opt_out = Self::is_opt_out_from_config(&config_path);

		Telemetry { endpoint, opt_out, client: Client::new() }
	}

	fn is_opt_out_from_config(config_file_path: &PathBuf) -> bool {
		let config: Config = match read_json_file(config_file_path) {
			Ok(config) => config,
			Err(err) => {
				log::debug!("{:?}", err.to_string());
				return false;
			},
		};

		!config.opt_out.version.is_empty()
	}

	/// Send JSON payload to saved api endpoint.
	/// Returns error and will not send anything if opt-out is true.
	/// Returns error from reqwest if the sending fails.
	/// It sends message only once as "best effort". There is no retry on error
	/// in order to keep overhead to a minimal.
	async fn send_json(&self, payload: Value) -> Result<()> {
		if self.opt_out {
			return Err(TelemetryError::OptedOut);
		}

		let request_builder = self.client.post(&self.endpoint);

		request_builder
			.json(&payload)
			.send()
			.await
			.map_err(TelemetryError::NetworkError)?;

		Ok(())
	}
}

/// Generically reports that the CLI was used to the telemetry endpoint.
/// There is explicitly no reqwest retries on failure to ensure overhead
/// stays to a minimum.
pub async fn record_cli_used(tel: Telemetry) -> Result<()> {
	let payload = generate_payload("", json!({}));

	let res = tel.send_json(payload).await;
	log::debug!("send_cli_used result: {:?}", res);

	res
}

/// Reports what CLI command was called to telemetry.
///
/// parameters:
/// `command_name`: the name of the command entered (new, up, build, etc)
/// `data`: the JSON representation of subcommands. This should never include any user inputted
/// data like a file name.
pub async fn record_cli_command(tel: Telemetry, command_name: &str, data: Value) -> Result<()> {
	let payload = generate_payload(command_name, data);

	let res = tel.send_json(payload).await;
	log::debug!("send_cli_used result: {:?}", res);

	res
}

#[derive(PartialEq, Serialize, Deserialize, Debug)]
struct OptOut {
	// what telemetry version did they opt-out for
	version: String,
}

/// Type to represent pop cli configuration.
/// This will be written as json to a config.json file.
#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct Config {
	opt_out: OptOut,
}

/// Returns the configuration file path based on OS's default config directory.
pub fn config_file_path() -> Result<PathBuf> {
	let config_path = dirs::config_dir().ok_or(TelemetryError::ConfigFileNotFound)?.join("pop");
	// Creates pop dir if needed
	create_dir_all(config_path.as_path()).map_err(|err| TelemetryError::IO(err))?;
	Ok(config_path.join("config.json"))
}

/// Writes opt-out to the configuration file at the specified path.
/// opt-out is currently the only config type. Hence, if the file exists, it will be overwritten.
///
/// parameters:
/// `config_path`: the path to write the config file to
pub fn write_config_opt_out(config_path: &PathBuf) -> Result<()> {
	let config = Config { opt_out: OptOut { version: CARGO_PKG_VERSION.to_string() } };

	let config_json = serde_json::to_string_pretty(&config)
		.map_err(|err| TelemetryError::SerializeFailed(err.to_string()))?;

	// overwrites file if it exists
	let mut file = File::create(&config_path).map_err(|err| TelemetryError::IO(err))?;
	file.write_all(config_json.as_bytes()).map_err(|err| TelemetryError::IO(err))?;

	Ok(())
}

fn read_json_file<T>(file_path: &PathBuf) -> std::result::Result<T, io::Error>
where
	T: DeserializeOwned,
{
	let mut file = File::open(file_path)?;

	let mut json = String::new();
	file.read_to_string(&mut json)?;

	let deserialized: T = serde_json::from_str(&json)?;

	Ok(deserialized)
}

fn generate_payload(event_name: &str, data: Value) -> Value {
	json!({
		"payload": {
			"hostname": "cli",
			"language": "en-US",
			"referrer": "",
			"screen": "1920x1080",
			"title": CARGO_PKG_VERSION,
			"url": "/",
			"website": WEBSITE_ID,
			"name": event_name,
			"data": data
		},
		"type": "event"
	})
}

#[cfg(test)]
mod tests {

	use super::*;
	use mockito::{Matcher, Mock, Server};
	use serde_json::json;
	use tempfile::TempDir;

	fn create_temp_config(temp_dir: &TempDir) -> PathBuf {
		let config_path = temp_dir.path().join("config.json");
		assert!(write_config_opt_out(&config_path).is_ok());
		config_path
	}
	async fn default_mock(mock_server: &mut Server, payload: String) -> Mock {
		mock_server
			.mock("POST", "/api/send")
			.match_header("content-type", "application/json")
			.match_header("accept", "*/*")
			.match_body(Matcher::JsonString(payload.clone()))
			.match_header("content-length", payload.len().to_string().as_str())
			.match_header("host", mock_server.host_with_port().trim())
			.create_async()
			.await
	}
	#[tokio::test]
	async fn write_config_opt_out_works() {
		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let actual_config: Config = read_json_file(&config_path).unwrap();
		let expected_config = Config { opt_out: OptOut { version: CARGO_PKG_VERSION.to_string() } };

		assert_eq!(actual_config, expected_config);
	}

	#[tokio::test]
	async fn new_telemetry_works() {
		let _ = env_logger::try_init();
		// assert that invalid config file results in a false opt_in (hence disabling telemetry)
		assert!(!Telemetry::init("".to_string(), PathBuf::new()).opt_out);

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let _: Config = read_json_file(&config_path).unwrap();

		let tel = Telemetry::init("127.0.0.1".to_string(), config_path);
		let expected_telemetry = Telemetry {
			endpoint: "127.0.0.1".to_string(),
			opt_out: true,
			client: Default::default(),
		};

		assert_eq!(tel.endpoint, expected_telemetry.endpoint);
		assert_eq!(tel.opt_out, expected_telemetry.opt_out);
	}

	#[tokio::test]
	async fn test_record_cli_used() {
		let _ = env_logger::try_init();
		let mut mock_server = mockito::Server::new_async().await;

		let mut endpoint = mock_server.url();
		endpoint.push_str("/api/send");

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = temp_dir.path().join("config.json");

		let expected_payload = generate_payload("", json!({})).to_string();

		let mock = default_mock(&mut mock_server, expected_payload).await;

		let tel = Telemetry::init(endpoint.clone(), config_path);

		assert!(record_cli_used(tel).await.is_ok());
		mock.assert_async().await;
	}

	#[tokio::test]
	async fn test_record_cli_command() {
		let _ = env_logger::try_init();
		let mut mock_server = mockito::Server::new_async().await;

		let mut endpoint = mock_server.url();
		endpoint.push_str("/api/send");

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();

		let config_path = temp_dir.path().join("config.json");

		let expected_payload = generate_payload("new", json!("parachain")).to_string();

		let mock = default_mock(&mut mock_server, expected_payload).await;

		let tel = Telemetry::init(endpoint.clone(), config_path);

		assert!(record_cli_command(tel, "new", json!("parachain")).await.is_ok());
		mock.assert_async().await;
	}

	#[tokio::test]
	async fn opt_out_fails() {
		let _ = env_logger::try_init();
		let mut mock_server = mockito::Server::new_async().await;

		let endpoint = mock_server.url();

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let mock = mock_server.mock("POST", "/").create_async().await;
		let mock = mock.expect_at_most(0);

		let mut tel = Telemetry::init(endpoint.clone(), config_path);

		assert!(matches!(tel.send_json(json!("foo")).await, Err(TelemetryError::OptedOut)));
		assert!(matches!(record_cli_used(tel.clone()).await, Err(TelemetryError::OptedOut)));
		assert!(matches!(
			record_cli_command(tel.clone(), "foo", json!("bar")).await,
			Err(TelemetryError::OptedOut)
		));
		mock.assert_async().await;

		// test it's set to true and works
		tel.opt_out = false;
		let mock = mock.expect_at_most(3);
		assert!(tel.send_json(json!("foo")).await.is_ok(),);
		assert!(record_cli_used(tel.clone()).await.is_ok());
		assert!(record_cli_command(tel, "foo", json!("bar")).await.is_ok());
		mock.assert_async().await;
	}
}
