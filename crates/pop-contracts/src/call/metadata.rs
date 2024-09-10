// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_extrinsics::ContractArtifacts;
use contract_transcode::ink_metadata::MessageParamSpec;
use scale_info::form::PortableForm;
use std::path::Path;

#[derive(Clone, PartialEq, Eq)]
/// Describes a contract message.
pub struct Message {
	/// The label of the message.
	pub label: String,
	/// If the message is allowed to mutate the contract state.
	pub mutates: bool,
	/// If the message accepts any `value` from the caller.
	pub payable: bool,
	/// The parameters of the deployment handler.
	pub args: Vec<String>,
	/// The message documentation.
	pub docs: String,
	/// If the message is the default for off-chain consumers (e.g UIs).
	pub default: bool,
}

/// Extracts a list of smart contract messages parsing the metadata file.
///
/// # Arguments
/// * `path` -  Location path of the project.
pub fn get_messages(path: &Path) -> Result<Vec<Message>, Error> {
	let cargo_toml_path = match path.ends_with("Cargo.toml") {
		true => path.to_path_buf(),
		false => path.join("Cargo.toml"),
	};
	let contract_artifacts =
		ContractArtifacts::from_manifest_or_file(Some(&cargo_toml_path), None)?;
	let transcoder = contract_artifacts.contract_transcoder()?;
	let mut messages: Vec<Message> = Vec::new();
	for message in transcoder.metadata().spec().messages() {
		messages.push(Message {
			label: message.label().to_string(),
			mutates: message.mutates(),
			payable: message.payable(),
			args: process_args(message.args()),
			docs: message.docs().join(" "),
			default: *message.default(),
		});
	}
	Ok(messages)
}
// Parse the message parameters into a vector of argument labels.
fn process_args(message_params: &[MessageParamSpec<PortableForm>]) -> Vec<String> {
	let mut args: Vec<String> = Vec::new();
	for arg in message_params {
		args.push(arg.label().to_string());
	}
	args
}

#[cfg(test)]
mod tests {
	use std::env;

	use super::*;
	use crate::{generate_smart_contract_test_environment, mock_build_process};
	use anyhow::Result;

	#[test]
	fn get_messages_work() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let message = get_messages(&temp_dir.path().join("testing"))?;
		assert_eq!(message.len(), 3);
		assert_eq!(message[0].label, "flip");
		assert_eq!(message[0].docs, " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.");
		assert_eq!(message[1].label, "get");
		assert_eq!(message[1].docs, " Simply returns the current value of our `bool`.");
		assert_eq!(message[2].label, "specific_flip");
		assert_eq!(message[2].docs, " A message for testing, flips the value of the stored `bool` with `new_value`  and is payable");
		// assert parsed arguments
		assert_eq!(message[2].args, vec!["new_value".to_string()]);
		Ok(())
	}
}
