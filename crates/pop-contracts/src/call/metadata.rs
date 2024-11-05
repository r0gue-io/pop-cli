// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_extrinsics::ContractArtifacts;
use contract_transcode::ink_metadata::MessageParamSpec;
use scale_info::form::PortableForm;
use std::path::Path;

#[derive(Clone, PartialEq, Eq)]
/// Describes a contract message.
pub struct Param {
	/// The label of the parameter.
	pub label: String,
	/// The type name of the parameter.
	pub type_name: String,
}
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
	pub args: Vec<Param>,
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

/// Extracts the information of a smart contract message parsing the metadata file.
///
/// # Arguments
/// * `path` -  Location path of the project.
/// * `message` - The label of the contract message.
fn get_message<P>(path: P, message: &str) -> Result<Message, Error>
where
	P: AsRef<Path>,
{
	get_messages(path.as_ref())?
		.into_iter()
		.find(|msg| msg.label == message)
		.ok_or_else(|| Error::InvalidMessageName(message.to_string()))
}

// Parse the message parameters into a vector of argument labels.
fn process_args(message_params: &[MessageParamSpec<PortableForm>]) -> Vec<Param> {
	let mut args: Vec<Param> = Vec::new();
	for arg in message_params {
		args.push(Param {
			label: arg.label().to_string(),
			type_name: arg.ty().display_name().to_string(),
		});
	}
	args
}

/// Processes a list of argument values for a specified contract message,
/// wrapping each value in `Some(...)` or replacing it with `None` if the argument is optional
///
/// # Arguments
/// * `path` -  Location path of the project.
/// * `message_label` - Label of the contract message to retrieve.
/// * `args` - Argument values provided by the user.
pub fn process_message_args<P>(
	path: P,
	message: &str,
	args: Vec<String>,
) -> Result<Vec<String>, Error>
where
	P: AsRef<Path>,
{
	let message = get_message(path, message)?;
	if args.len() != message.args.len() {
		return Err(Error::IncorrectArguments {
			expected: message.args.len(),
			provided: args.len(),
		});
	}
	Ok(args
		.into_iter()
		.zip(&message.args)
		.map(|(arg, param)| match (param.type_name.as_str(), arg.is_empty()) {
			("Option", true) => "None".to_string(), /* If the argument is Option and empty, */
			// replace it with `None`
			("Option", false) => format!("Some({})", arg), /* If the argument is Option and not */
			// empty, wrap it in `Some(...)`
			_ => arg, // If the argument is not Option, return it as is
		})
		.collect::<Vec<String>>())
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
		assert_eq!(message[2].args.len(), 2);
		assert_eq!(message[2].args[0].label, "new_value".to_string());
		assert_eq!(message[2].args[0].type_name, "bool".to_string());
		assert_eq!(message[2].args[1].label, "number".to_string());
		assert_eq!(message[2].args[1].type_name, "Option".to_string());
		Ok(())
	}

	#[test]
	fn get_message_work() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		assert!(matches!(
			get_message(&temp_dir.path().join("testing"), "wrong_flip"),
			Err(Error::InvalidMessageName(name)) if name == "wrong_flip".to_string()));
		let message = get_message(&temp_dir.path().join("testing"), "specific_flip")?;
		assert_eq!(message.label, "specific_flip");
		assert_eq!(message.docs, " A message for testing, flips the value of the stored `bool` with `new_value`  and is payable");
		// assert parsed arguments
		assert_eq!(message.args.len(), 2);
		assert_eq!(message.args[0].label, "new_value".to_string());
		assert_eq!(message.args[0].type_name, "bool".to_string());
		assert_eq!(message.args[1].label, "number".to_string());
		assert_eq!(message.args[1].type_name, "Option".to_string());
		Ok(())
	}

	#[test]
	fn process_message_args_work() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		assert!(matches!(
			process_message_args(temp_dir.path().join("testing"),"wrong_flip", Vec::new()),
			Err(Error::InvalidMessageName(error)) if error == "wrong_flip".to_string()));
		assert!(matches!(
			process_message_args(
				temp_dir.path().join("testing"),
				"specific_flip",
				Vec::new()
			),
			Err(Error::IncorrectArguments {expected, provided }) if expected == 2 && provided == 0
		));
		assert_eq!(
			process_message_args(
				temp_dir.path().join("testing"),
				"specific_flip",
				["true".to_string(), "2".to_string()].to_vec()
			)?,
			["true".to_string(), "Some(2)".to_string()]
		);
		assert_eq!(
			process_message_args(
				temp_dir.path().join("testing"),
				"specific_flip",
				["true".to_string(), "".to_string()].to_vec()
			)?,
			["true".to_string(), "None".to_string()]
		);
		Ok(())
	}
}
