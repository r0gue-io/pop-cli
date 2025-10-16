// SPDX-License-Identifier: GPL-3.0

//! Functionality for processing and extracting metadata from ink! smart contracts.

use crate::errors::Error;
use pop_common::{DefaultConfig, format_type};
use scale_info::{PortableRegistry, form::PortableForm};
use std::path::Path;
use url::Url;
#[cfg(feature = "v5")]
use {
	contract_extrinsics::ContractArtifacts,
	contract_extrinsics::ContractStorageRpc,
	contract_transcode::ContractMessageTranscoder,
	contract_transcode::ink_metadata::{MessageParamSpec, layout::Layout},
	ink_env::DefaultEnvironment,
	pop_common::parse_account,
	sp_core::{Encode, blake2_128},
};
#[cfg(feature = "v6")]
use {
	contract_extrinsics_inkv6::ContractArtifacts,
	contract_extrinsics_inkv6::ContractStorageRpc,
	contract_transcode_inkv6::ContractMessageTranscoder,
	contract_transcode_inkv6::ink_metadata::{MessageParamSpec, layout::Layout},
	ink_env_v6::DefaultEnvironment,
	pop_common::account_id::parse_h160_account as parse_account,
	sp_core_inkv6::{Encode, blake2_128},
};

/// Represents a callable entity within a smart contract, either a function or storage item.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ContractCallable {
	/// A callable function (message or constructor).
	Function(ContractFunction),
	/// A storage item that can be queried.
	Storage(ContractStorage),
}

impl ContractCallable {
	/// Returns the name/label of the callable entity.
	///
	/// For functions, returns the function label.
	/// For storage items, returns the storage field name.
	///
	/// # Returns
	/// A string containing the name of the callable entity.
	pub fn name(&self) -> String {
		match self {
			ContractCallable::Function(f) => f.label.clone(),
			ContractCallable::Storage(s) => s.name.clone(),
		}
	}
}

/// Describes a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
	/// The label of the parameter.
	pub label: String,
	/// The type name of the parameter.
	pub type_name: String,
}

/// Describes a contract function.
#[derive(Clone, PartialEq, Eq, Debug)]
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

/// Describes a contract storage item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractStorage {
	/// The name of the storage field.
	pub name: String,
	/// The type name of the storage field.
	pub type_name: String,
	/// The storage key used to fetch the value from the contract.
	pub storage_key: u32,
	/// The type ID from the metadata registry, used for decoding storage values.
	pub type_id: u32,
}

/// Specifies the type of contract function, either a constructor or a message.
#[derive(Clone, PartialEq, Eq)]
pub enum FunctionType {
	/// Function that initializes and creates a new contract instance.
	Constructor,
	/// Function that can be called on an instantiated contract.
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

fn collapse_docs(docs: &[String]) -> String {
	docs.iter()
		.map(|s| if s.is_empty() { " " } else { s })
		.collect::<Vec<_>>()
		.join("")
		.trim()
		.to_string()
}

fn get_contract_transcoder(path: &Path) -> anyhow::Result<ContractMessageTranscoder> {
	let contract_artifacts = if path.is_dir() || path.ends_with("Cargo.toml") {
		let cargo_toml_path =
			if path.ends_with("Cargo.toml") { path.to_path_buf() } else { path.join("Cargo.toml") };
		ContractArtifacts::from_manifest_or_file(Some(&cargo_toml_path), None)?
	} else {
		ContractArtifacts::from_manifest_or_file(None, Some(&path.to_path_buf()))?
	};
	contract_artifacts.contract_transcoder()
}

/// Fetches and decodes a storage value from a deployed smart contract.
///
/// # Arguments
/// * `storage` - Storage item descriptor containing key and type information
/// * `account` - Contract address as string
/// * `rpc_url` - URL of the RPC endpoint to connect to
/// * `path` - Path to contract artifacts for metadata access
///
/// # Returns
/// * `Ok(String)` - The decoded storage value as a string
/// * `Err(anyhow::Error)` - If any step fails
pub async fn fetch_contract_storage(
	storage: &ContractStorage,
	account: &str,
	rpc_url: &Url,
	path: &Path,
) -> anyhow::Result<String> {
	// Get the transcoder to decode the storage value
	let transcoder = get_contract_transcoder(path)?;

	// Create RPC client
	let rpc = ContractStorageRpc::<DefaultConfig>::new(rpc_url).await?;

	// Parse account address to AccountId
	let account_id = parse_account(account)?;

	// Fetch contract info to get the trie_id
	let contract_info = rpc.fetch_contract_info::<DefaultEnvironment>(&account_id).await?;
	let trie_id = contract_info.trie_id();

	// Encode the storage key as bytes
	// The storage key needs to be properly formatted:
	// blake2_128 hash (16 bytes) + root_key (4 bytes)
	let root_key_bytes = storage.storage_key.encode();
	let mut full_key = blake2_128(&root_key_bytes).to_vec();
	full_key.extend_from_slice(&root_key_bytes);

	// Fetch the storage value
	let bytes = full_key.into();
	let value = rpc.fetch_contract_storage(trie_id, &bytes, None).await?;

	match value {
		Some(data) => {
			// Decode the raw bytes using the type_id from storage
			let decoded_value = transcoder.decode(storage.type_id, &mut &data.0[..])?;
			Ok(decoded_value.to_string())
		},
		None => Ok("No value found".to_string()),
	}
}

/// Extracts a list of smart contract storage items parsing the contract artifact.
///
/// # Arguments
/// * `path` - Location path of the project or contract artifact.
pub fn get_contract_storage_info(path: &Path) -> Result<Vec<ContractStorage>, Error> {
	let transcoder = get_contract_transcoder(path)?;
	let metadata = transcoder.metadata();
	let layout = metadata.layout();
	let registry = metadata.registry();

	let mut storage_items = Vec::new();
	extract_storage_fields(layout, registry, &mut storage_items);

	Ok(storage_items)
}

// Recursively extracts storage fields from the layout
fn extract_storage_fields(
	layout: &Layout<PortableForm>,
	registry: &PortableRegistry,
	storage_items: &mut Vec<ContractStorage>,
) {
	match layout {
		Layout::Root(root_layout) => {
			// For root layout, capture the root key and traverse into the nested layout
			let root_key = *root_layout.root_key().key();
			extract_storage_fields_with_key(
				root_layout.layout(),
				registry,
				storage_items,
				root_key,
			);
		},
		Layout::Struct(struct_layout) => {
			// For struct layout at the top level (no root key yet), skip it
			// This shouldn't normally happen as Root should be the outermost layout
			for field in struct_layout.fields() {
				extract_storage_fields(field.layout(), registry, storage_items);
			}
		},
		Layout::Leaf(_) => {
			// Leaf nodes represent individual storage items but without a name at this level
			// They are typically accessed through their parent (struct field)
		},
		Layout::Hash(_) | Layout::Array(_) | Layout::Enum(_) => {
			// For complex layouts (hash maps, arrays, enums), we could expand this
			// but for now we focus on simple struct fields
		},
	}
}

// Helper function to extract storage fields with a known root key
fn extract_storage_fields_with_key(
	layout: &Layout<PortableForm>,
	registry: &PortableRegistry,
	storage_items: &mut Vec<ContractStorage>,
	root_key: u32,
) {
	match layout {
		Layout::Root(root_layout) => {
			// Nested root layout, update the root key
			let new_root_key = *root_layout.root_key().key();
			extract_storage_fields_with_key(
				root_layout.layout(),
				registry,
				storage_items,
				new_root_key,
			);
		},
		Layout::Struct(struct_layout) => {
			// For struct layout, extract all fields with the current root key
			for field in struct_layout.fields() {
				extract_field(field.name(), field.layout(), registry, storage_items, root_key);
			}
		},
		Layout::Leaf(_) => {
			// Leaf nodes represent individual storage items but without a name at this level
		},
		Layout::Hash(_) | Layout::Array(_) | Layout::Enum(_) => {
			// For complex layouts, we could expand this later
		},
	}
}

// Extracts a single field and recursively processes nested layouts
fn extract_field(
	name: &str,
	layout: &Layout<PortableForm>,
	registry: &PortableRegistry,
	storage_items: &mut Vec<ContractStorage>,
	root_key: u32,
) {
	match layout {
		Layout::Leaf(leaf_layout) => {
			// Get the type ID and resolve it to get the type name
			let type_id = leaf_layout.ty();
			if let Some(ty) = registry.resolve(type_id.id) {
				let type_name = format_type(ty, registry);
				storage_items.push(ContractStorage {
					name: name.to_string(),
					type_name,
					storage_key: root_key,
					type_id: type_id.id,
				});
			}
		},
		Layout::Struct(struct_layout) => {
			// Nested struct - recursively extract its fields with qualified names
			for field in struct_layout.fields() {
				let qualified_name = format!("{}.{}", name, field.name());
				extract_field(&qualified_name, field.layout(), registry, storage_items, root_key);
			}
		},
		Layout::Hash(_) | Layout::Array(_) | Layout::Enum(_) | Layout::Root(_) => {
			// For complex nested types, we could add more detailed handling
			// For now, we'll skip or handle them simply
		},
	}
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
	let transcoder = get_contract_transcoder(path)?;
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
				docs: collapse_docs(message.docs()),
				#[cfg(feature = "v5")]
				default: *message.default(),
				#[cfg(feature = "v6")]
				default: message.default(),
			})
			.collect(),
		FunctionType::Constructor => metadata
			.spec()
			.constructors()
			.iter()
			.map(|constructor| ContractFunction {
				label: constructor.label().to_string(),
				#[cfg(feature = "v5")]
				payable: *constructor.payable(),
				#[cfg(feature = "v6")]
				payable: constructor.payable(),
				args: process_args(constructor.args(), metadata.registry()),
				docs: collapse_docs(constructor.docs()),
				#[cfg(feature = "v5")]
				default: *constructor.default(),
				#[cfg(feature = "v6")]
				default: constructor.default(),
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
pub fn get_message<P>(path: P, message: &str) -> Result<ContractFunction, Error>
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

/// Extracts the information of a smart contract function (message or constructor) parsing the
/// contract artifact.
///
/// # Arguments
/// * `path` - Location path of the project or contract artifact.
/// * `label` - The label of the contract function.
/// * `function_type` - Specifies whether to extract a message or constructor.
pub fn extract_function<P>(
	path: P,
	label: &str,
	function_type: FunctionType,
) -> Result<ContractFunction, Error>
where
	P: AsRef<Path>,
{
	match function_type {
		FunctionType::Message => get_message(path.as_ref(), label),
		FunctionType::Constructor => get_constructor(path.as_ref(), label),
	}
}

/// Processes a list of argument values for a specified contract function,
/// wrapping each value in `Some(...)` or replacing it with `None` if the argument is optional.
///
/// # Arguments
/// * `function` - The contract function to process.
/// * `args` - Argument values provided by the user.
pub fn process_function_args(
	function: &ContractFunction,
	args: Vec<String>,
) -> Result<Vec<String>, Error> {
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
		.collect())
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
			assert_eq!(
				message[0].docs,
				"A message that can be called on instantiated contracts. This one flips the value of the stored `bool` from `true` to `false` and vice versa."
			);
			assert_eq!(message[1].label, "get");
			assert_eq!(message[1].docs, "Simply returns the current value of our `bool`.");
			assert_eq!(message[2].label, "specific_flip");
			assert_eq!(
				message[2].docs,
				"A message for testing, flips the value of the stored `bool` with `new_value` and is payable"
			);
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
		let message = get_messages(temp_dir.path().join("testing"))?;
		assert_contract_metadata_parsed(message)?;

		// Test with a metadata file path
		let message = get_messages(current_dir.join("./tests/files/testing.contract"))?;
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
			get_message(temp_dir.path().join("testing"), "wrong_flip"),
			Err(Error::InvalidMessageName(name)) if name == *"wrong_flip"));
		let message = get_message(temp_dir.path().join("testing"), "specific_flip")?;
		assert_eq!(message.label, "specific_flip");
		assert_eq!(
			message.docs,
			"A message for testing, flips the value of the stored `bool` with `new_value` and is payable"
		);
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
		let constructor = get_constructors(temp_dir.path().join("testing"))?;
		assert_eq!(constructor.len(), 2);
		assert_eq!(constructor[0].label, "new");
		assert_eq!(
			constructor[0].docs,
			"Constructor that initializes the `bool` value to the given `init_value`."
		);
		assert_eq!(constructor[1].label, "default");
		assert_eq!(
			constructor[1].docs,
			"Constructor that initializes the `bool` value to `false`. Constructors can delegate to other constructors."
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
			get_constructor(temp_dir.path().join("testing"), "wrong_constructor"),
			Err(Error::InvalidConstructorName(name)) if name == *"wrong_constructor"));
		let constructor = get_constructor(temp_dir.path().join("testing"), "default")?;
		assert_eq!(constructor.label, "default");
		assert_eq!(
			constructor.docs,
			"Constructor that initializes the `bool` value to `false`. Constructors can delegate to other constructors."
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

		// Test messages
		assert!(matches!(
			extract_function(temp_dir.path().join("testing"), "wrong_flip", FunctionType::Message),
			Err(Error::InvalidMessageName(error)) if error == *"wrong_flip"));

		let specific_flip = extract_function(
			temp_dir.path().join("testing"),
			"specific_flip",
			FunctionType::Message,
		)?;

		assert!(matches!(
			process_function_args(&specific_flip, Vec::new()),
			Err(Error::IncorrectArguments {expected, provided }) if expected == 2 && provided == 0
		));

		assert_eq!(
			process_function_args(&specific_flip, ["true".to_string(), "2".to_string()].to_vec())?,
			["true".to_string(), "Some(2)".to_string()]
		);

		assert_eq!(
			process_function_args(&specific_flip, ["true".to_string(), "".to_string()].to_vec())?,
			["true".to_string(), "None".to_string()]
		);

		// Test constructors
		assert!(matches!(
			extract_function(temp_dir.path().join("testing"), "wrong_constructor", FunctionType::Constructor),
			Err(Error::InvalidConstructorName(error)) if error == *"wrong_constructor"));

		let default_constructor = extract_function(
			temp_dir.path().join("testing"),
			"default",
			FunctionType::Constructor,
		)?;
		assert!(matches!(
			process_function_args(&default_constructor, Vec::new()),
			Err(Error::IncorrectArguments {expected, provided }) if expected == 2 && provided == 0
		));

		assert_eq!(
			process_function_args(
				&default_constructor,
				["true".to_string(), "2".to_string()].to_vec()
			)?,
			["true".to_string(), "Some(2)".to_string()]
		);

		assert_eq!(
			process_function_args(
				&default_constructor,
				["true".to_string(), "".to_string()].to_vec()
			)?,
			["true".to_string(), "None".to_string()]
		);
		Ok(())
	}

	#[test]
	fn get_contract_storage_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;

		// Test with a directory path
		let storage = get_contract_storage_info(temp_dir.path().join("testing").as_path())?;
		assert_eq!(storage.len(), 2);
		assert_eq!(storage[0].name, "value");
		assert_eq!(storage[0].type_name, "bool");
		assert_eq!(storage[1].name, "number");
		// The exact type name may vary, but it should contain u32
		assert!(storage[1].type_name.contains("u32"));

		// Test with a metadata file path
		let storage = get_contract_storage_info(
			current_dir.join("./tests/files/testing.contract").as_path(),
		)?;
		assert_eq!(storage.len(), 2);
		assert_eq!(storage[0].name, "value");
		assert_eq!(storage[0].type_name, "bool");

		Ok(())
	}
}
