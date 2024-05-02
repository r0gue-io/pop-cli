use reqwest::Client;
use serde_json::{json, Value};
use thiserror::Error;

const WEBSITE_ID: &str = "3da3a7d3-0d51-4f23-a4e0-5e3f7f9442c8";
const CLI_VERSION: &str = "v1.0.0";
const ENDPOINT_POSTFIX: &str = "/api/send";
const RETRY_LIMIT: u8 = 1;

#[derive(Error, Debug)]
pub enum TelemetryError {
	#[error("a reqwest error occurred: {0}")]
	NetworkError(reqwest::Error),
	#[error("opt-in is not set, can not report metrics")]
	NotOptedIn,
}

type Result<T> = std::result::Result<T, TelemetryError>;

struct Telemetry {
	endpoint: String,
	opt_in: bool,
	retry_limit: u8,
	client: Client,
}

impl Telemetry {
	fn new() -> Self {
		// environment variable `POP_TELEMETRY_ENDPOINT` is evaluated at compile time
		let endpoint = option_env!("POP_TELEMETRY_ENDPOINT").unwrap_or("http://127.0.0.1:3000");
		let mut endpoint: String = endpoint.to_string();
		endpoint.push_str(ENDPOINT_POSTFIX);

		let client = reqwest::Client::new();
		let opt_in = Self::check_opt_in();
		let retry_limit = RETRY_LIMIT;

		Telemetry { endpoint, opt_in, retry_limit, client }
	}

	fn check_opt_in() -> bool {
		// TODO
		true
	}

	async fn send_json(&self, payload: Value) -> Result<()> {
		if !self.opt_in {
			return Err(TelemetryError::NotOptedIn);
		}

		let request_builder = self.client.post(&self.endpoint);

		let response = request_builder
			.json(&payload)
			.send()
			.await
			.map_err(TelemetryError::NetworkError);

		println!("{:#?}", response);

		Ok(())
	}
}

pub async fn record_cli_used() -> Result<()> {
	let tel = Telemetry::new();

	let payload = generate_payload("cli", CLI_VERSION, "/", WEBSITE_ID, "", json!({}));

	let res = tel.send_json(payload).await;
	log::debug!("send_cli_used result: {:?}", res);

	Ok(())
}

pub async fn record_cli_command(command_name: &str, data: Value) -> Result<()> {
	let tel = Telemetry::new();

	let payload = generate_payload("cli", CLI_VERSION, "/", WEBSITE_ID, command_name, data);

	let res = tel.send_json(payload).await?;
	log::debug!("send_cli_used result: {:?}", res);

	Ok(())
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
