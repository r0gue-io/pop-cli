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

/// Error returned when `--json` mode requires a flag that was not provided.
#[derive(Debug)]
pub(crate) struct PromptRequiredError(pub String);

impl std::fmt::Display for PromptRequiredError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::error::Error for PromptRequiredError {}

/// Error returned when `--json` is requested for a command that doesn't support it.
#[derive(Debug)]
pub(crate) struct UnsupportedJsonError(pub String);

impl std::fmt::Display for UnsupportedJsonError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "--json is not yet supported for the `{}` command", self.0)
	}
}

impl std::error::Error for UnsupportedJsonError {}

/// Returns an error indicating that `--json` is not yet supported for the given command.
pub(crate) fn reject_unsupported_json(command_name: &str) -> anyhow::Result<()> {
	Err(UnsupportedJsonError(command_name.to_string()).into())
}

/// Error returned when command inputs are invalid.
#[derive(Debug)]
pub(crate) struct InvalidInputError(pub String);

impl std::fmt::Display for InvalidInputError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::error::Error for InvalidInputError {}

/// Returns an invalid-input error that maps to `INVALID_INPUT` in the JSON envelope.
#[allow(dead_code)]
pub(crate) fn invalid_input_error(message: impl Into<String>) -> anyhow::Error {
	InvalidInputError(message.into()).into()
}

/// Message used when `--json` mode cannot satisfy an interactive prompt.
pub(crate) const JSON_PROMPT_ERR: &str = "interactive prompt required but --json mode is active";

/// Builds an I/O error carrying a typed prompt-required cause.
#[allow(dead_code)]
pub(crate) fn prompt_required_io_error() -> std::io::Error {
	std::io::Error::other(PromptRequiredError(JSON_PROMPT_ERR.to_string()))
}

/// Error returned when a build/test command fails while producing JSON output.
#[derive(Debug)]
pub(crate) struct BuildCommandError {
	message: String,
	details: Option<String>,
}

impl BuildCommandError {
	pub(crate) fn new(message: impl Into<String>) -> Self {
		Self { message: message.into(), details: None }
	}

	pub(crate) fn with_details(mut self, details: impl Into<String>) -> Self {
		self.details = Some(details.into());
		self
	}

	pub(crate) fn details(&self) -> Option<&str> {
		self.details.as_deref()
	}
}

impl std::fmt::Display for BuildCommandError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.message)
	}
}

impl std::error::Error for BuildCommandError {}

/// Returns a build/test error that maps to `BUILD_ERROR` in the JSON envelope.
pub(crate) fn build_error(message: impl Into<String>) -> anyhow::Error {
	BuildCommandError::new(message).into()
}

/// Returns a build/test error with details that maps to `BUILD_ERROR` in the JSON envelope.
pub(crate) fn build_error_with_details(
	message: impl Into<String>,
	details: impl Into<String>,
) -> anyhow::Error {
	BuildCommandError::new(message).with_details(details).into()
}
/// Error returned when deployment/launch fails while producing JSON output.
#[derive(Debug)]
pub(crate) struct DeployCommandError(pub String);

impl std::fmt::Display for DeployCommandError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::error::Error for DeployCommandError {}

/// Returns a deployment error that maps to `DEPLOY_ERROR` in the JSON envelope.
#[allow(dead_code)]
pub(crate) fn deploy_error(message: impl Into<String>) -> anyhow::Error {
	DeployCommandError(message.into()).into()
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
	fn prompt_required_error_maps_to_correct_code() {
		// Simulate the downcast logic from main.rs.
		let err: anyhow::Error = PromptRequiredError("--version is required".into()).into();
		assert!(err.downcast_ref::<PromptRequiredError>().is_some());

		// Build the same envelope main.rs would produce.
		let code = if err.downcast_ref::<UnsupportedJsonError>().is_some() {
			ErrorCode::UnsupportedJson
		} else if err.downcast_ref::<PromptRequiredError>().is_some() {
			ErrorCode::PromptRequired
		} else {
			ErrorCode::Internal
		};
		let resp = CliResponse::err(CliError::new(code, err.to_string()));
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["error"]["code"], "PROMPT_REQUIRED");
		assert_eq!(json["error"]["message"], "--version is required");
		assert_eq!(json["success"], false);
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
