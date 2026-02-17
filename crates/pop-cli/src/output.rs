// SPDX-License-Identifier: GPL-3.0

use serde::Serialize;

/// Determines how CLI output is rendered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputMode {
	#[default]
	Human,
	Json,
}

/// Top-level JSON envelope returned by every command when `--json` is active.
#[derive(Debug, Serialize)]
pub(crate) struct CliResponse<T: Serialize> {
	schema_version: u32,
	success: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	data: Option<T>,
	#[serde(skip_serializing_if = "Option::is_none")]
	error: Option<CliError>,
}

impl<T: Serialize> CliResponse<T> {
	/// Build a successful response.
	pub(crate) fn ok(data: T) -> Self {
		Self { schema_version: 1, success: true, data: Some(data), error: None }
	}
}

impl CliResponse<()> {
	/// Build an error response.
	pub(crate) fn err(error: CliError) -> Self {
		Self { schema_version: 1, success: false, data: None, error: Some(error) }
	}

	/// Print this response as a single JSON line to stdout.
	pub(crate) fn print_json_err(&self) {
		match serde_json::to_string(self) {
			Ok(json) => println!("{json}"),
			Err(e) => eprintln!("fatal: failed to serialize JSON error response: {e}"),
		}
	}
}

impl<T: Serialize> CliResponse<T> {
	/// Print this response as a single JSON line to stdout.
	pub(crate) fn print_json(&self) {
		match serde_json::to_string(self) {
			Ok(json) => println!("{json}"),
			Err(e) => eprintln!("fatal: failed to serialize JSON response: {e}"),
		}
	}
}

/// Structured error included in the JSON envelope.
#[derive(Debug, Serialize)]
pub(crate) struct CliError {
	code: ErrorCode,
	message: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	details: Option<String>,
}

impl CliError {
	pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
		Self { code, message: message.into(), details: None }
	}

	#[allow(dead_code)]
	pub(crate) fn with_details(mut self, details: impl Into<String>) -> Self {
		self.details = Some(details.into());
		self
	}
}

/// Machine-readable error codes.
#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(dead_code)]
pub(crate) enum ErrorCode {
	Internal,
	InvalidInput,
	PromptRequired,
	NetworkError,
	BuildError,
	DeployError,
	UnsupportedJson,
}

/// Returns a JSON error response indicating that `--json` is not yet supported
/// for the given command, and exits with code 1.
pub(crate) fn reject_unsupported_json(command_name: &str) -> ! {
	CliResponse::err(CliError::new(
		ErrorCode::UnsupportedJson,
		format!("--json is not yet supported for the `{command_name}` command"),
	))
	.print_json_err();
	std::process::exit(1);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ok_response_serializes() {
		let resp = CliResponse::ok("hello");
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["schema_version"], 1);
		assert_eq!(json["success"], true);
		assert_eq!(json["data"], "hello");
		assert!(json.get("error").is_none());
	}

	#[test]
	fn err_response_serializes() {
		let resp =
			CliResponse::err(CliError::new(ErrorCode::Internal, "boom").with_details("stack"));
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["schema_version"], 1);
		assert_eq!(json["success"], false);
		assert!(json.get("data").is_none());
		assert_eq!(json["error"]["code"], "INTERNAL");
		assert_eq!(json["error"]["message"], "boom");
		assert_eq!(json["error"]["details"], "stack");
	}

	#[test]
	fn unsupported_json_error_serializes() {
		let resp = CliResponse::err(CliError::new(
			ErrorCode::UnsupportedJson,
			"--json is not yet supported for the `build` command",
		));
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["schema_version"], 1);
		assert_eq!(json["success"], false);
		assert!(json.get("data").is_none());
		assert_eq!(json["error"]["code"], "UNSUPPORTED_JSON");
		assert_eq!(json["error"]["message"], "--json is not yet supported for the `build` command");
	}

	#[test]
	fn error_codes_serialize_screaming_snake() {
		let cases = vec![
			(ErrorCode::Internal, "INTERNAL"),
			(ErrorCode::InvalidInput, "INVALID_INPUT"),
			(ErrorCode::PromptRequired, "PROMPT_REQUIRED"),
			(ErrorCode::NetworkError, "NETWORK_ERROR"),
			(ErrorCode::BuildError, "BUILD_ERROR"),
			(ErrorCode::DeployError, "DEPLOY_ERROR"),
			(ErrorCode::UnsupportedJson, "UNSUPPORTED_JSON"),
		];
		for (code, expected) in cases {
			let json = serde_json::to_value(&code).unwrap();
			assert_eq!(json, expected);
		}
	}
}
