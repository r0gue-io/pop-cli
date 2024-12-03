// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_extrinsics::ContractArtifacts;
use contract_transcode::ink_metadata::MessageParamSpec;
use pop_common::format_type;
use scale_info::{form::PortableForm, PortableRegistry};
use std::path::Path;

/// Describes a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
	/// The label of the parameter.
	pub label: String,
	/// The type name of the parameter.
	pub type_name: String,
}

/// Describes a contract function.
#[derive(Clone, PartialEq, Eq)]
pub struct ContractFunction {
	/// The label of the function.
	pub label: String,
	/// If the function accepts any `value` from the caller.
	pub payable: bool,
	/// The parameters of the deployment handler.
	pub args: Vec<Param>,
	/// The function documentation.
	pub docs: String,
	/// If the message/constructor is the default for off-chain consumers (e.g UIs).
	pub default: bool,
	/// If the message is allowed to mutate the contract state. true for constructors.
	pub mutates: bool,
}

/// Specifies the type of contract funtion, either a constructor or a message.
#[derive(Clone, PartialEq, Eq)]
pub enum FunctionType {
	Constructor,
	Message,
}

/// Extracts a list of smart contract messages parsing the contract artifact.
///
/// # Arguments
/// * `path` -  Location path of the project or contract artifact.
pub fn get_messages<P>(path: P) -> Result<Vec<ContractFunction>, Error>
where
	P: AsRef<Path>,
{
	get_contract_functions(path.as_ref(), FunctionType::Message)
}

/// Extracts a list of smart contract contructors parsing the contract artifact.
///
/// # Arguments
/// * `path` -  Location path of the project or contract artifact.
pub fn get_constructors<P>(path: P) -> Result<Vec<ContractFunction>, Error>
where
	P: AsRef<Path>,
{
	get_contract_functions(path.as_ref(), FunctionType::Constructor)
}

/// Extracts a list of smart contract functions (messages or constructors) parsing the contract
/// artifact.
///
/// # Arguments
/// * `path` - Location path of the project or contract artifact.
/// * `function_type` - Specifies whether to extract messages or constructors.
fn get_contract_functions(
	path: &Path,
	function_type: FunctionType,
) -> Result<Vec<ContractFunction>, Error> {
	let contract_artifacts = if path.is_dir() || path.ends_with("Cargo.toml") {
		let cargo_toml_path =
			if path.ends_with("Cargo.toml") { path.to_path_buf() } else { path.join("Cargo.toml") };
		ContractArtifacts::from_manifest_or_file(Some(&cargo_toml_path), None)?
	} else {
		ContractArtifacts::from_manifest_or_file(None, Some(&path.to_path_buf()))?
	};
	let transcoder = contract_artifacts.contract_transcoder()?;
	let metadata = transcoder.metadata();

	Ok(match function_type {
		FunctionType::Message => metadata
			.spec()
			.messages()
			.iter()
			.map(|message| ContractFunction {
				label: message.label().to_string(),
				mutates: message.mutates(),
				payable: message.payable(),
				args: process_args(message.args(), metadata.registry()),
				docs: message.docs().join(" "),
				default: *message.default(),
			})
			.collect(),
		FunctionType::Constructor => metadata
			.spec()
			.constructors()
			.iter()
			.map(|constructor| ContractFunction {
				label: constructor.label().to_string(),
				payable: *constructor.payable(),
				args: process_args(constructor.args(), metadata.registry()),
				docs: constructor.docs().join(" "),
				default: *constructor.default(),
				mutates: true,
			})
			.collect(),
	})
}

/// Extracts the information of a smart contract message parsing the contract artifact.
///
/// # Arguments
/// * `path` -  Location path of the project or contract artifact.
/// * `message` - The label of the contract message.
fn get_message<P>(path: P, message: &str) -> Result<ContractFunction, Error>
where
	P: AsRef<Path>,
{
	get_messages(path.as_ref())?
		.into_iter()
		.find(|msg| msg.label == message)
		.ok_or_else(|| Error::InvalidMessageName(message.to_string()))
}

/// Extracts the information of a smart contract constructor parsing the contract artifact.
///
/// # Arguments
/// * `path` -  Location path of the project or contract artifact.
/// * `constructor` - The label of the constructor.
fn get_constructor<P>(path: P, constructor: &str) -> Result<ContractFunction, Error>
where
	P: AsRef<Path>,
{
	get_constructors(path.as_ref())?
		.into_iter()
		.find(|c| c.label == constructor)
		.ok_or_else(|| Error::InvalidConstructorName(constructor.to_string()))
}

// Parse the parameters into a vector of argument labels.
fn process_args(
	params: &[MessageParamSpec<PortableForm>],
	registry: &PortableRegistry,
) -> Vec<Param> {
	let mut args: Vec<Param> = Vec::new();
	for arg in params {
		// Resolve type from registry to provide full type representation.
		let type_name =
			format_type(registry.resolve(arg.ty().ty().id).expect("type not found"), registry);
		args.push(Param { label: arg.label().to_string(), type_name });
	}
	args
}

/// Processes a list of argument values for a specified contract function,
/// wrapping each value in `Some(...)` or replacing it with `None` if the argument is optional.
///
/// # Arguments
/// * `path` -  Location path of the project or contract artifact.
/// * `label` - Label of the contract message to retrieve.
/// * `args` - Argument values provided by the user.
/// * `function_type` - Specifies whether to process arguments of messages or constructors.
pub fn process_function_args<P>(
	path: P,
	label: &str,
	args: Vec<String>,
	function_type: FunctionType,
) -> Result<Vec<String>, Error>
where
	P: AsRef<Path>,
{
	let function = match function_type {
		FunctionType::Message => get_message(path, label)?,
		FunctionType::Constructor => get_constructor(path, label)?,
	};
	if args.len() != function.args.len() {
		return Err(Error::IncorrectArguments {
			expected: function.args.len(),
			provided: args.len(),
		});
	}
	Ok(args
		.into_iter()
		.zip(&function.args)
		.map(|(arg, param)| match (param.type_name.starts_with("Option<"), arg.is_empty()) {
			// If the argument is Option and empty, replace it with `None`
			(true, true) => "None".to_string(),
			// If the argument is Option and not empty, wrap it in `Some(...)`
			(true, false) => format!("Some({})", arg),
			// If the argument is not Option, return it as is
			_ => arg,
		})
		.collect::<Vec<String>>())
}

#[cfg(test)]
mod tests {
	use std::env;

	use super::*;
	use crate::{mock_build_process, new_environment};
	use anyhow::Result;

	#[test]
	fn get_messages_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");

		// Helper function to avoid duplicated code
		fn assert_contract_metadata_parsed(message: Vec<ContractFunction>) -> Result<()> {
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
			assert_eq!(message[2].args[1].type_name, "Option<u32>: None, Some(u32)".to_string());
			Ok(())
		}

		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;

		// Test with a directory path
		let message = get_messages(&temp_dir.path().join("testing"))?;
		assert_contract_metadata_parsed(message)?;

		// Test with a metadata file path
		let message = get_messages(&current_dir.join("./tests/files/testing.contract"))?;
		assert_contract_metadata_parsed(message)?;

		Ok(())
	}

	#[test]
	fn get_message_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
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
		assert_eq!(message.args[1].type_name, "Option<u32>: None, Some(u32)".to_string());
		Ok(())
	}

	#[test]
	fn get_constructors_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let constructor = get_constructors(&temp_dir.path().join("testing"))?;
		assert_eq!(constructor.len(), 2);
		assert_eq!(constructor[0].label, "new");
		assert_eq!(
			constructor[0].docs,
			"Constructor that initializes the `bool` value to the given `init_value`."
		);
		assert_eq!(constructor[1].label, "default");
		assert_eq!(
			constructor[1].docs,
			"Constructor that initializes the `bool` value to `false`.  Constructors can delegate to other constructors."
		);
		// assert parsed arguments
		assert_eq!(constructor[0].args.len(), 1);
		assert_eq!(constructor[0].args[0].label, "init_value".to_string());
		assert_eq!(constructor[0].args[0].type_name, "bool".to_string());
		assert_eq!(constructor[1].args.len(), 2);
		assert_eq!(constructor[1].args[0].label, "init_value".to_string());
		assert_eq!(constructor[1].args[0].type_name, "bool".to_string());
		assert_eq!(constructor[1].args[1].label, "number".to_string());
		assert_eq!(constructor[1].args[1].type_name, "Option<u32>: None, Some(u32)".to_string());
		Ok(())
	}

	#[test]
	fn get_constructor_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		assert!(matches!(
			get_constructor(&temp_dir.path().join("testing"), "wrong_constructor"),
			Err(Error::InvalidConstructorName(name)) if name == "wrong_constructor".to_string()));
		let constructor = get_constructor(&temp_dir.path().join("testing"), "default")?;
		assert_eq!(constructor.label, "default");
		assert_eq!(
			constructor.docs,
			"Constructor that initializes the `bool` value to `false`.  Constructors can delegate to other constructors."
		);
		// assert parsed arguments
		assert_eq!(constructor.args.len(), 2);
		assert_eq!(constructor.args[0].label, "init_value".to_string());
		assert_eq!(constructor.args[0].type_name, "bool".to_string());
		assert_eq!(constructor.args[1].label, "number".to_string());
		assert_eq!(constructor.args[1].type_name, "Option<u32>: None, Some(u32)".to_string());
		Ok(())
	}

	#[test]
	fn process_function_args_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		assert!(matches!(
			process_function_args(temp_dir.path().join("testing"),"wrong_flip", Vec::new(), FunctionType::Message),
			Err(Error::InvalidMessageName(error)) if error == "wrong_flip".to_string()));
		assert!(matches!(
			process_function_args(
				temp_dir.path().join("testing"),
				"specific_flip",
				Vec::new(),
				FunctionType::Message
			),
			Err(Error::IncorrectArguments {expected, provided }) if expected == 2 && provided == 0
		));
		assert_eq!(
			process_function_args(
				temp_dir.path().join("testing"),
				"specific_flip",
				["true".to_string(), "2".to_string()].to_vec(),
				FunctionType::Message
			)?,
			["true".to_string(), "Some(2)".to_string()]
		);
		assert_eq!(
			process_function_args(
				temp_dir.path().join("testing"),
				"specific_flip",
				["true".to_string(), "".to_string()].to_vec(),
				FunctionType::Message
			)?,
			["true".to_string(), "None".to_string()]
		);

		// Test constructors
		assert!(matches!(
			process_function_args(temp_dir.path().join("testing"),"wrong_constructor", Vec::new(), FunctionType::Constructor),
			Err(Error::InvalidConstructorName(error)) if error == "wrong_constructor".to_string()));
		assert!(matches!(
			process_function_args(
				temp_dir.path().join("testing"),
				"default",
				Vec::new(),
				FunctionType::Constructor
			),
			Err(Error::IncorrectArguments {expected, provided }) if expected == 2 && provided == 0
		));
		assert_eq!(
			process_function_args(
				temp_dir.path().join("testing"),
				"default",
				["true".to_string(), "2".to_string()].to_vec(),
				FunctionType::Constructor
			)?,
			["true".to_string(), "Some(2)".to_string()]
		);
		assert_eq!(
			process_function_args(
				temp_dir.path().join("testing"),
				"default",
				["true".to_string(), "".to_string()].to_vec(),
				FunctionType::Constructor
			)?,
			["true".to_string(), "None".to_string()]
		);
		Ok(())
	}
}
