// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_extrinsics::ContractArtifacts;
use contract_transcode::ink_metadata::MessageParamSpec;
use scale_info::form::PortableForm;
use std::path::Path;

#[derive(Clone, PartialEq, Eq)]
// TODO: We are ignoring selector, return type for now.
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
//TODO: We are ignoring the type of the argument.
fn process_args(message_params: &[MessageParamSpec<PortableForm>]) -> Vec<String> {
	let mut args: Vec<String> = Vec::new();
	for arg in message_params {
		args.push(arg.label().to_string());
	}
	args
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{create_smart_contract, errors::Error, Contract};
	use anyhow::Result;
	use std::{env, fs, path::PathBuf};

	fn generate_smart_contract_test_environment() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("testing");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("testing", temp_contract_dir.as_path(), &Contract::Standard)?;
		Ok(temp_dir)
	}
	// Function that mocks the build process generating the contract artifacts.
	fn mock_build_process(temp_contract_dir: PathBuf) -> Result<(), Error> {
		// Create a target directory
		let target_contract_dir = temp_contract_dir.join("target");
		fs::create_dir(&target_contract_dir)?;
		fs::create_dir(&target_contract_dir.join("ink"))?;
		// Copy a mocked testing.contract file inside the target directory
		let current_dir = env::current_dir().expect("Failed to get current directory");
		let contract_file = current_dir.join("../../tests/files/testing.contract");
		fs::copy(contract_file, &target_contract_dir.join("ink/testing.contract"))?;
		Ok(())
	}
	#[test]
	fn get_messages_work() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
		let message = get_messages(&temp_dir.path().join("testing"))?;
		assert_eq!(message.len(), 2);
		assert_eq!(message[0].label, "flip");
		assert_eq!(message[0].docs, " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.");
		assert_eq!(message[1].label, "get");
		assert_eq!(message[1].docs, " Simply returns the current value of our `bool`.");
		Ok(())
	}
}
