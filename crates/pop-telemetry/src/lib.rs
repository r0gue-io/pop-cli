// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]

use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
	env,
	fs::{File, create_dir_all},
	io,
	io::{Read, Write},
	path::PathBuf,
};
use thiserror::Error;

const ENDPOINT: &str = "https://insights.onpop.io/api/send";
const WEBSITE_ID: &str = "0cbea0ba-4752-45aa-b3cd-8fd11fa722f7";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A telemetry error.
#[derive(Error, Debug)]
pub enum TelemetryError {
	/// A network error occurred.
	#[error("a reqwest error occurred: {0}")]
	NetworkError(reqwest::Error),
	/// An IO error occurred.
	#[error("io error occurred: {0}")]
	IO(io::Error),
	/// The user has opted out and metrics cannot be reported.
	#[error("opt-out has been set, can not report metrics")]
	OptedOut,
	/// The configuration file cannot be found.
	#[error("unable to find config file")]
	ConfigFileNotFound,
	/// The configuration could not be serialized.
	#[error("serialization failed: {0}")]
	SerializeFailed(String),
}

/// A result that represents either success ([`Ok`]) or failure ([`TelemetryError`]).
pub type Result<T> = std::result::Result<T, TelemetryError>;

/// Anonymous collection of usage metrics.
#[derive(Debug, Clone)]
pub struct Telemetry {
	// Endpoint to the telemetry API.
	// This should include the domain and api path (e.g. localhost:3000/api/send)
	endpoint: String,
	// Unami Website ID for telemetry tracking
	website_id: String,
	// Has the user opted-out to anonymous telemetry
	opt_out: bool,
	// Reqwest client
	client: Client,
}

impl Telemetry {
	/// Create a new [Telemetry] instance.
	///
	/// parameters:
	/// `config_path`: the path to the configuration file (used for opt-out checks)
	pub fn new(config_path: &PathBuf) -> Self {
		Self::init_with_website_id(ENDPOINT.to_string(), WEBSITE_ID.to_string(), config_path)
	}

	/// Initialize a new [Telemetry] instance with a custom endpoint.
	/// Uses the default WEBSITE_ID constant.
	/// Can be used in tests to provide mock endpoints.
	/// parameters:
	/// `endpoint`: the API endpoint that telemetry will call
	/// `config_path`: the path to the configuration file (used for opt-out checks)
	#[allow(dead_code)]
	fn init(endpoint: String, config_path: &PathBuf) -> Self {
		Self::init_with_website_id(endpoint, WEBSITE_ID.to_string(), config_path)
	}

	/// Initialize a new [Telemetry] instance with custom endpoint and website_id.
	/// Can be used in tests to provide mock endpoints and website IDs.
	/// in addition to making this crate useful to other projects.
	/// parameters:
	/// `endpoint`: the API endpoint that telemetry will call
	/// `website_id`: the website ID for telemetry tracking
	/// `config_path`: the path to the configuration file (used for opt-out checks)
	pub fn init_with_website_id(
		endpoint: String,
		website_id: String,
		config_path: &PathBuf,
	) -> Self {
		let opt_out = Self::is_opt_out(config_path);

		Telemetry { endpoint, website_id, opt_out, client: Client::new() }
	}

	fn is_opt_out_from_config(config_file_path: &PathBuf) -> bool {
		let config: Config = match read_json_file(config_file_path) {
			Ok(config) => config,
			Err(err) => {
				log::debug!("{:?}", err.to_string());
				return false;
			},
		};

		// if the version is empty, then the user has not opted out
		!config.opt_out.version.is_empty()
	}

	// Checks two env variables, CI & DO_NOT_TRACK. If either are set to true, disable telemetry
	fn is_opt_out_from_env() -> bool {
		// CI first as it is more likely to be set
		let ci = env::var("CI").unwrap_or_default();
		let do_not_track = env::var("DO_NOT_TRACK").unwrap_or_default();
		ci == "true" || ci == "1" || do_not_track == "true" || do_not_track == "1"
	}

	/// Check if the user has opted out of telemetry through two methods:
	/// 1. Check environment variable DO_NOT_TRACK. If not set check...
	/// 2. Configuration file
	fn is_opt_out(config_file_path: &PathBuf) -> bool {
		Self::is_opt_out_from_env() || Self::is_opt_out_from_config(config_file_path)
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

		log::debug!("send_json payload: {:?}", payload);
		match request_builder
			.json(&payload)
			.send()
			.await
			.map_err(TelemetryError::NetworkError)
		{
			Ok(res) => match res.error_for_status() {
				Ok(res) => {
					let text = res.text().await.unwrap_or_default();
					log::debug!("send_json response: {}", text);
				},
				Err(e) => {
					log::debug!("send_json server error: {:?}", e);
				},
			},
			Err(e) => {
				log::debug!("send_json network error: {:?}", e);
			},
		}

		Ok(())
	}
}

/// Generically reports that the CLI was used to the telemetry endpoint.
/// There is explicitly no reqwest retries on failure to ensure overhead
/// stays to a minimum.
pub async fn record_cli_used(tel: Telemetry) -> Result<()> {
	let payload = generate_payload("init", json!({}), &tel.website_id);
	tel.send_json(payload).await
}

/// Reports what CLI command was called to telemetry.
///
/// parameters:
/// `event`: the name of the event to record (new, up, build, etc)
/// `data`: additional data to record.
pub async fn record_cli_command(tel: Telemetry, event: &str, data: Value) -> Result<()> {
	let payload = generate_payload(event, data, &tel.website_id);
	tel.send_json(payload).await
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
	create_dir_all(config_path.as_path()).map_err(TelemetryError::IO)?;
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
	let mut file = File::create(config_path).map_err(TelemetryError::IO)?;
	file.write_all(config_json.as_bytes()).map_err(TelemetryError::IO)?;

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

fn generate_payload(event: &str, data: Value, website_id: &str) -> Value {
	json!({
		"payload": {
			"hostname": "cli",
			"language": "en-US",
			"referrer": "",
			"screen": "1920x1080",
			"title": CARGO_PKG_VERSION,
			"url": "/",
			"website": website_id,
			"name": event,
			"data": data,
		},
		"type": "event"
	})
}

#[cfg(test)]
mod tests {

	use super::*;
	use mockito::{Matcher, Mock, Server};
	use tempfile::TempDir;

	fn create_temp_config(temp_dir: &TempDir) -> Result<PathBuf> {
		let config_path = temp_dir.path().join("config.json");
		write_config_opt_out(&config_path)?;
		Ok(config_path)
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
	async fn write_config_opt_out_works() -> Result<()> {
		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir)?;

		let actual_config: Config = read_json_file(&config_path).unwrap();
		let expected_config = Config { opt_out: OptOut { version: CARGO_PKG_VERSION.to_string() } };

		assert_eq!(actual_config, expected_config);
		Ok(())
	}

	#[tokio::test]
	async fn new_telemetry_works() -> Result<()> {
		let _ = env_logger::try_init();

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		// write a config file with opt-out set
		let config_path = create_temp_config(&temp_dir)?;

		let _: Config = read_json_file(&config_path).unwrap();

		let tel = Telemetry::init_with_website_id(
			"127.0.0.1".to_string(),
			"test-website-id".to_string(),
			&config_path,
		);
		let expected_telemetry = Telemetry {
			endpoint: "127.0.0.1".to_string(),
			website_id: "test-website-id".to_string(),
			opt_out: true,
			client: Default::default(),
		};

		assert_eq!(tel.endpoint, expected_telemetry.endpoint);
		assert_eq!(tel.website_id, expected_telemetry.website_id);
		assert_eq!(tel.opt_out, expected_telemetry.opt_out);

		let tel = Telemetry::new(&config_path);

		let expected_telemetry = Telemetry {
			endpoint: ENDPOINT.to_string(),
			website_id: WEBSITE_ID.to_string(),
			opt_out: true,
			client: Default::default(),
		};

		assert_eq!(tel.endpoint, expected_telemetry.endpoint);
		assert_eq!(tel.website_id, expected_telemetry.website_id);
		assert_eq!(tel.opt_out, expected_telemetry.opt_out);
		Ok(())
	}

	#[test]
	fn new_telemetry_env_vars_works() {
		let _ = env_logger::try_init();

		// assert that no config file, and env vars not existing sets opt-out to false
		unsafe {
			env::remove_var("DO_NOT_TRACK");
			env::set_var("CI", "false");
		}
		assert!(!Telemetry::init("".to_string(), &PathBuf::new()).opt_out);

		// assert that if DO_NOT_TRACK env var is set, opt-out is true
		unsafe {
			env::set_var("DO_NOT_TRACK", "true");
		}
		assert!(Telemetry::init("".to_string(), &PathBuf::new()).opt_out);
		unsafe {
			env::remove_var("DO_NOT_TRACK");
		}

		// assert that if CI env var is set, opt-out is true
		unsafe {
			env::set_var("CI", "true");
		}
		assert!(Telemetry::init("".to_string(), &PathBuf::new()).opt_out);
		unsafe {
			env::remove_var("CI");
		}
	}

	#[tokio::test]
	async fn test_record_cli_used() -> Result<()> {
		let _ = env_logger::try_init();
		let mut mock_server = Server::new_async().await;

		let mut endpoint = mock_server.url();
		endpoint.push_str("/api/send");

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = temp_dir.path().join("config.json");
		let expected_payload = generate_payload("init", json!({}), WEBSITE_ID).to_string();
		let mock = default_mock(&mut mock_server, expected_payload).await;

		let mut tel = Telemetry::init(endpoint.clone(), &config_path);
		tel.opt_out = false; // override as endpoint is mocked

		record_cli_used(tel).await?;
		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn test_record_cli_command() -> Result<()> {
		let _ = env_logger::try_init();
		let mut mock_server = Server::new_async().await;

		let mut endpoint = mock_server.url();
		endpoint.push_str("/api/send");

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();

		let config_path = temp_dir.path().join("config.json");

		let expected_payload =
			generate_payload("new", json!({"command": "chain"}), WEBSITE_ID).to_string();

		let mock = default_mock(&mut mock_server, expected_payload).await;

		let mut tel = Telemetry::init(endpoint.clone(), &config_path);
		tel.opt_out = false; // override as endpoint is mocked

		record_cli_command(
			tel,
			"new",
			json!({
				"command": "chain"
			}),
		)
		.await?;
		mock.assert_async().await;
		Ok(())
	}

	#[tokio::test]
	async fn opt_out_set_fails() {
		let _ = env_logger::try_init();
		let mut mock_server = Server::new_async().await;

		let endpoint = mock_server.url();

		let mock = mock_server.mock("POST", "/").create_async().await;
		let mock = mock.expect_at_most(0);

		let mut tel = Telemetry::init(endpoint.clone(), &PathBuf::new());
		tel.opt_out = true;

		assert!(matches!(tel.send_json(Value::Null).await, Err(TelemetryError::OptedOut)));
		assert!(matches!(record_cli_used(tel.clone()).await, Err(TelemetryError::OptedOut)));
		assert!(matches!(
			record_cli_command(tel, "foo", json!({})).await,
			Err(TelemetryError::OptedOut)
		));
		mock.assert_async().await;
	}
}
