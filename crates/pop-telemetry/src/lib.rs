use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
	fs::{create_dir_all, File},
	io,
	io::{Read, Write},
	path::{Path, PathBuf},
};
use thiserror::Error;

const WEBSITE_ID: &str = "3da3a7d3-0d51-4f23-a4e0-5e3f7f9442c8";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Error, Debug)]
pub enum TelemetryError {
	#[error("a reqwest error occurred: {0}")]
	NetworkError(reqwest::Error),
	#[error("io error occurred: {0}")]
	IO(io::Error),
	#[error("opt-in is not set, can not report metrics")]
	NotOptedIn,
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
	// Has the user opted-in to anonymous telemetry
	opt_in: bool,
	// Reqwest client
	client: Client,
}

impl Telemetry {
	pub fn new(endpoint: String, config_path: PathBuf) -> Self {
		let opt_in = Self::check_opt_in_from_config(&config_path);

		Telemetry { endpoint, opt_in, client: Client::new() }
	}

	fn check_opt_in_from_config(config_file_path: &PathBuf) -> bool {
		let config: Config = match read_json_file(config_file_path) {
			Ok(config) => config,
			Err(err) => {
				log::debug!("{:?}", err.to_string());
				return false;
			},
		};

		config.opt_in.allow
	}

	/// Send JSON payload to saved api endpoint.
	/// Will return error and not send anything if opt-in is false.
	/// Will return error from reqwest if the sending fails.
	/// It sends message only once as "best effort". There is no retry on error
	/// in order to keep overhead to a minimal.
	async fn send_json(&self, payload: Value) -> Result<()> {
		if !self.opt_in {
			return Err(TelemetryError::NotOptedIn);
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
struct OptIn {
	// did user opt in
	allow: bool,
	// what telemetry version did they opt-in for
	version: String,
}

/// Type to represent pop cli configuration.
/// This will be written as json to a config.json file.
#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct Config {
	opt_in: OptIn,
}

/// Returns the configuration file path based on OS's default config directory.
pub fn config_file_path() -> Result<PathBuf> {
	let config_path = dirs::config_dir().ok_or(TelemetryError::ConfigFileNotFound)?.join("pop");
	// Creates pop dir if needed
	create_dir_all(config_path.as_path()).map_err(|err| TelemetryError::IO(err))?;
	Ok(config_path.join("config.json"))
}

/// Writes a default config to the configuration file at the specified path.
/// Returns true if file written.
/// Returns false if file existed and nothing was written.
///
/// parameters:
/// `config_path`: the path to write the config file to
pub fn write_default_config(config_path: &PathBuf) -> Result<bool> {
	if !Path::new(&config_path).exists() {
		let default_config =
			Config { opt_in: OptIn { allow: true, version: CARGO_PKG_VERSION.to_string() } };

		let default_config_json = serde_json::to_string_pretty(&default_config)
			.map_err(|err| TelemetryError::SerializeFailed(err.to_string()))?;

		let mut file = File::create(&config_path).map_err(|err| TelemetryError::IO(err))?;
		file.write_all(default_config_json.as_bytes())
			.map_err(|err| TelemetryError::IO(err))?;
	} else {
		// if the file already existed
		return Ok(false);
	}

	Ok(true)
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
		assert!(write_default_config(&config_path).is_ok());
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
	async fn write_default_config_works() {
		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let actual_config: Config = read_json_file(&config_path).unwrap();
		let expected_config =
			Config { opt_in: OptIn { allow: true, version: CARGO_PKG_VERSION.to_string() } };

		assert_eq!(actual_config, expected_config);
	}

	#[tokio::test]
	async fn new_telemetry_works() {
		let _ = env_logger::try_init();
		// assert that invalid config file results in a false opt_in (hence disabling telemetry)
		assert!(!Telemetry::new("".to_string(), PathBuf::new()).opt_in);

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let _: Config = read_json_file(&config_path).unwrap();

		let tel = Telemetry::new("127.0.0.1".to_string(), config_path);
		let expected_telemetry = Telemetry {
			endpoint: "127.0.0.1".to_string(),
			opt_in: true,
			client: Default::default(),
		};

		assert_eq!(tel.endpoint, expected_telemetry.endpoint);
		assert_eq!(tel.opt_in, expected_telemetry.opt_in);
	}

	#[tokio::test]
	async fn test_record_cli_used() {
		let _ = env_logger::try_init();
		let mut mock_server = mockito::Server::new_async().await;

		let mut endpoint = mock_server.url();
		endpoint.push_str("/api/send");

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let expected_payload = generate_payload("", json!({})).to_string();

		let mock = default_mock(&mut mock_server, expected_payload).await;

		let tel = Telemetry::new(endpoint.clone(), config_path);

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
		let config_path = create_temp_config(&temp_dir);

		let expected_payload = generate_payload("new", json!("parachain")).to_string();

		let mock = default_mock(&mut mock_server, expected_payload).await;

		let tel = Telemetry::new(endpoint.clone(), config_path);

		assert!(record_cli_command(tel, "new", json!("parachain")).await.is_ok());
		mock.assert_async().await;
	}

	#[tokio::test]
	async fn opt_in_fails() {
		let _ = env_logger::try_init();
		let mut mock_server = mockito::Server::new_async().await;

		let endpoint = mock_server.url();

		// Mock config file path function to return a temporary path
		let temp_dir = TempDir::new().unwrap();
		let config_path = create_temp_config(&temp_dir);

		let mock = mock_server.mock("POST", "/").create_async().await;
		let mock = mock.expect_at_most(0);

		let mut tel_with_bad_config_path =
			Telemetry::new(endpoint.clone(), config_path.join("break_path"));

		assert!(matches!(
			tel_with_bad_config_path.send_json(json!("foo")).await,
			Err(TelemetryError::NotOptedIn)
		));
		assert!(matches!(
			record_cli_used(tel_with_bad_config_path.clone()).await,
			Err(TelemetryError::NotOptedIn)
		));
		assert!(matches!(
			record_cli_command(tel_with_bad_config_path.clone(), "foo", json!("bar")).await,
			Err(TelemetryError::NotOptedIn)
		));
		mock.assert_async().await;

		// test it's set to true and works
		tel_with_bad_config_path.opt_in = true;
		let mock = mock.expect_at_most(3);
		assert!(tel_with_bad_config_path.send_json(json!("foo")).await.is_ok(),);
		assert!(record_cli_used(tel_with_bad_config_path.clone()).await.is_ok());
		assert!(record_cli_command(tel_with_bad_config_path, "foo", json!("bar")).await.is_ok());
		mock.assert_async().await;
	}
}
