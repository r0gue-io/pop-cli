use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{create_dir_all, File};
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

const WEBSITE_ID: &str = "3da3a7d3-0d51-4f23-a4e0-5e3f7f9442c8";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Error, Debug)]
pub enum TelemetryError {
	#[error("a reqwest error occurred: {0}")]
	NetworkError(reqwest::Error),
	#[error("opt-in is not set, can not report metrics")]
	NotOptedIn,
	#[error("unable to find config file")]
	ConfigFileNotFound,
	#[error("io error occurred: {0}")]
	IO(io::Error),
	#[error("serialization failed: {0}")]
	SerializeFailed(String),
}

type Result<T> = std::result::Result<T, TelemetryError>;

struct Telemetry {
	endpoint: String,
	opt_in: bool,
	client: Client,
}

impl Telemetry {
	fn new(endpoint: String, config_path: PathBuf) -> Self {
		let client = reqwest::Client::new();
		let opt_in = Self::check_opt_in(&config_path);

		Telemetry { endpoint, opt_in, client }
	}

	fn check_opt_in(config_file_path: &PathBuf) -> bool {
		let mut file = File::open(config_file_path)
			.map_err(|err| {
				log::debug!("{}", err.to_string());
				return false;
			})
			.expect("error mapped above");

		let mut config_json = String::new();
		file.read_to_string(&mut config_json)
			.map_err(|err| {
				log::debug!("{}", err.to_string());
				return false;
			})
			.expect("error mapped above");

		let config: Config = serde_json::from_str(&config_json)
			.map_err(|err| {
				log::debug!("{}", err.to_string());
				return false;
			})
			.expect("error mapped above");

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

pub async fn record_cli_used() -> Result<()> {
	// environment variable `POP_TELEMETRY_ENDPOINT` is evaluated at compile time
	let endpoint =
		option_env!("POP_TELEMETRY_ENDPOINT").unwrap_or("http://127.0.0.1:3000/api/send");

	let tel = Telemetry::new(endpoint.into(), config_file_path()?);

	let payload = generate_payload("cli", CARGO_PKG_VERSION, "/", WEBSITE_ID, "", json!({}));

	let res = tel.send_json(payload).await;
	log::debug!("send_cli_used result: {:?}", res);

	Ok(())
}

pub async fn record_cli_command(command_name: &str, data: Value) -> Result<()> {
	// environment variable `POP_TELEMETRY_ENDPOINT` is evaluated at *compile* time
	let endpoint =
		option_env!("POP_TELEMETRY_ENDPOINT").unwrap_or("http://127.0.0.1:3000/api/send");

	let tel = Telemetry::new(endpoint.into(), config_file_path()?);

	let payload = generate_payload("cli", CARGO_PKG_VERSION, "/", WEBSITE_ID, command_name, data);

	let res = tel.send_json(payload).await?;
	log::debug!("send_cli_used result: {:?}", res);

	Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
struct OptIn {
	allow: bool,
	version: String,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
	opt_in: OptIn,
}
pub fn config_file_path() -> Result<PathBuf> {
	let config_path = dirs::config_dir().ok_or(TelemetryError::ConfigFileNotFound)?.join("pop");
	// Creates pop dir if needed
	create_dir_all(config_path.as_path()).map_err(|err| TelemetryError::IO(err))?;
	Ok(config_path.join("config.json"))
}

pub fn write_default_config() -> Result<Option<PathBuf>> {
	let config_path = config_file_path()?;
	if !Path::new(&config_path).exists() {
		let default_config =
			Config { opt_in: OptIn { allow: true, version: CARGO_PKG_VERSION.to_string() } };

		let default_config_json = serde_json::to_string_pretty(&default_config)
			.map_err(|err| TelemetryError::SerializeFailed(err.to_string()))?;

		let mut file = File::create(&config_path).map_err(|err| TelemetryError::IO(err))?;
		file.write_all(default_config_json.as_bytes())
			.map_err(|err| TelemetryError::IO(err))?;
	} else {
		// if the file already existed, return None
		return Ok(None);
	}

	Ok(Some(config_path))
}

fn generate_payload(
	hostname: &str,
	title: &str,
	url: &str,
	website_id: &str,
	event_name: &str,
	data: Value,
) -> Value {
	json!({
		"payload": {
			"hostname": hostname,
			"language": "en-US",
			"referrer": "",
			"screen": "1920x1080",
			"title": title,
			"url": url,
			"website": website_id,
			"name": event_name,
			"data": data
		},
		"type": "event"
	})
}
