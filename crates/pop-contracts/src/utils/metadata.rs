// SPDX-License-Identifier: GPL-3.0

//! Functionality for processing and extracting metadata from ink! smart contracts.

use crate::{DefaultEnvironment, errors::Error};
use anyhow::Context;
use contract_build::CrateMetadata;
use contract_extrinsics::{ContractStorageRpc, TrieId};
use contract_metadata::{ContractMetadata, Language, compatibility};
use contract_transcode::{
	ContractMessageTranscoder,
	ink_metadata::{MessageParamSpec, layout::Layout},
};
use ink_env::call::utils::EncodeArgsWith;
use pop_common::{DefaultConfig, format_type, parse_h160_account};
use scale_info::{PortableRegistry, Type, form::PortableForm};
use sp_core::blake2_128;
use std::path::{Path, PathBuf};
use url::Url;

const MAPPING_TYPE_PATH: &str = "ink_storage::lazy::mapping::Mapping";

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

	/// Returns a descriptive hint string indicating the type of this callable entity.
	pub fn hint(&self) -> String {
		match self {
			ContractCallable::Function(f) => {
				let prelude = if f.mutates { "📝 [MUTATES] " } else { "[READS] " };
				format!("{}{}", prelude, f.label)
			},
			ContractCallable::Storage(s) => {
				format!("[STORAGE] {}", &s.name)
			},
		}
	}

	/// Returns a descriptive documentation string for this callable entity.
	pub fn docs(&self) -> String {
		match self {
			ContractCallable::Function(f) => f.docs.clone(),
			ContractCallable::Storage(s) => s.type_name.clone(),
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
	/// The type name of the mapping key, when this storage is a mapping. None otherwise.
	pub key_type_name: Option<String>,
}

/// Prepared contract artifact path for extrinsics operations.
///
/// When compatibility warnings are present, this may point to a temporary sanitized artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedContractArtifact {
	/// Path to an artifact consumable by contract-extrinsics.
	pub path: PathBuf,
	/// Compatibility warning to display to users, if any.
	pub compatibility_warning: Option<String>,
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

/// Extracts contract messages and storage fields using a single metadata parse.
///
/// This avoids repeated transcoder initialization when both datasets are needed.
pub fn get_messages_and_storage<P>(
	path: P,
) -> Result<(Vec<ContractFunction>, Vec<ContractStorage>), Error>
where
	P: AsRef<Path>,
{
	let transcoder = get_contract_transcoder(path.as_ref())?;
	let metadata = transcoder.metadata();
	let messages = metadata
		.spec()
		.messages()
		.iter()
		.map(|message| ContractFunction {
			label: message.label().to_string(),
			mutates: message.mutates(),
			payable: message.payable(),
			args: process_args(message.args(), metadata.registry()),
			docs: collapse_docs(message.docs()),
			default: message.default(),
		})
		.collect();

	let mut storage_items = Vec::new();
	extract_storage_fields(metadata.layout(), metadata.registry(), &mut storage_items);
	Ok((messages, storage_items))
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
	let metadata_path = resolve_contract_metadata_path(path)?;
	let metadata = ContractMetadata::load(&metadata_path)?;
	ContractMessageTranscoder::try_from(metadata)
		.context("Failed to deserialize ink project metadata from contract metadata")
}

fn resolve_contract_metadata_path(path: &Path) -> anyhow::Result<PathBuf> {
	if path.is_dir() || path.ends_with("Cargo.toml") {
		let cargo_toml_path =
			if path.ends_with("Cargo.toml") { path.to_path_buf() } else { path.join("Cargo.toml") };
		let crate_metadata = CrateMetadata::from_manifest_path(Some(&cargo_toml_path))?;
		if crate_metadata.contract_bundle_path().exists() {
			Ok(crate_metadata.contract_bundle_path())
		} else if crate_metadata.metadata_path().exists() {
			Ok(crate_metadata.metadata_path())
		} else {
			anyhow::bail!(
				"Failed to find any contract artifacts in target directory. \nRun `pop build --path {}` to generate the artifacts.",
				path.display()
			)
		}
	} else if path.extension().and_then(|ext| ext.to_str()) == Some("polkavm") {
		let file_name = path
			.file_stem()
			.context("PolkaVM bundle file has unreadable name")?
			.to_str()
			.context("Error parsing filename string")?;
		let dir = path.parent().map_or_else(PathBuf::new, PathBuf::from);
		let metadata_path = dir.join(format!("{file_name}.json"));
		if metadata_path.exists() {
			Ok(metadata_path)
		} else {
			anyhow::bail!("No contract metadata found. Expected file {}", metadata_path.display())
		}
	} else {
		Ok(path.to_path_buf())
	}
}

/// Resolves the artifact path to use for extrinsics and optionally prepares a sanitized copy when
/// compatibility warnings would otherwise be emitted repeatedly.
pub fn prepare_artifact_for_extrinsics(path: &Path) -> anyhow::Result<PreparedContractArtifact> {
	let metadata_path = resolve_contract_metadata_path(path)?;
	let mut metadata = ContractMetadata::load(&metadata_path)?;
	let compatibility_warning = if matches!(metadata.source.language.language, Language::Ink) {
		compatibility::check_contract_ink_compatibility(&metadata.source.language.version, None)
			.err()
			.map(|err| err.to_string())
	} else {
		None
	};

	if compatibility_warning.is_none() {
		return Ok(PreparedContractArtifact { path: metadata_path, compatibility_warning: None });
	}

	// Skip compatibility checks in downstream loaders while preserving ABI/transcoding data.
	metadata.source.language.language = Language::Solidity;
	let mut sanitized = tempfile::Builder::new()
		.prefix("pop-contract-artifact-")
		.suffix(".json")
		.tempfile()?;
	serde_json::to_writer(sanitized.as_file_mut(), &metadata)?;
	let (_, artifact_path) = sanitized.keep()?;
	Ok(PreparedContractArtifact { path: artifact_path, compatibility_warning })
}

async fn decode_mapping(
	storage: &ContractStorage,
	rpc: &ContractStorageRpc<DefaultConfig>,
	trie_id: &TrieId,
	ty: &Type<PortableForm>,
	transcoder: &ContractMessageTranscoder,
	key_filter: Option<&str>,
) -> anyhow::Result<String> {
	// Fetch ALL contract keys, then filter to those belonging to this mapping's root_key.
	// This mirrors contract-extrinsics behavior and is robust across hashing strategies.
	let mut all_keys = Vec::new();
	let mut start_key: Option<Vec<u8>> = None;
	// Page size is chosen to be large enough to cover all mappings in a single trie.
	const PAGE: u32 = 1000;
	loop {
		let page_keys = rpc
			.fetch_storage_keys_paged(
				trie_id,
				None, // no prefix: page through entire child trie
				PAGE,
				start_key.as_deref(),
				None,
			)
			.await?;
		let count = page_keys.len();
		if count == 0 {
			break;
		}
		start_key = page_keys.last().map(|b| b.0.clone());
		all_keys.extend(page_keys);
		if (count as u32) < PAGE {
			break;
		}
	}

	// Filter keys by matching the embedded root key at bytes [16..20].
	//
	// Storage key format in ink!:
	// - Bytes [0..16]:  Blake2-128 hash of the root key (used for key distribution)
	// - Bytes [16..20]: Root key as u32 in little-endian format (identifies the storage field)
	// - Bytes [20..]:   SCALE-encoded mapping key (the user's key for this mapping entry)
	//
	// This format is defined by ink!'s storage layout and is stable across ink! v4 and v5.
	// If future versions change this format, this validation check should catch it.
	let keys: Vec<_> = all_keys
		.into_iter()
		.filter(|k| {
			// Validate minimum key length: must contain hash (16) + root_key (4) = 20 bytes minimum
			if k.0.len() < 20 {
				return false;
			}
			// Extract the root key from bytes [16..20] and compare with expected storage key
			let mut rk = [0u8; 4];
			rk.copy_from_slice(&k.0[16..20]);
			let root = u32::from_le_bytes(rk);
			root == storage.storage_key
		})
		.collect();

	if keys.is_empty() {
		return Ok("Mapping is empty".to_string());
	}

	// Fetch values for all keys in a single batch
	let values = rpc.fetch_storage_entries(trie_id, &keys, None).await?;

	// Determine K and V type ids from the Mapping<K, V> type
	let (key_type_id, value_type_id) = match (param_type_id(ty, "K"), param_type_id(ty, "V")) {
		(Some(k), Some(v)) => (k, v),
		_ => {
			// Fallback: cannot determine generics; show raw count
			return Ok(format!("Mapping {{ {} entries }}", values.len()));
		},
	};

	// Zip keys and values into a simple Vec for decoding/formatting
	let pairs: Vec<(Vec<u8>, Option<Vec<u8>>)> = keys
		.into_iter()
		.zip(values.into_iter())
		.map(|(k, v)| (k.0, v.map(|b| b.0)))
		.collect();

	decode_mapping_impl(pairs, key_type_id, value_type_id, transcoder, key_filter)
}

// A small helper to make mapping decoding logic unit-testable without RPC.
// It expects full storage keys (including the 20-byte prefix) paired with optional values.
pub(crate) fn decode_mapping_impl(
	pairs: Vec<(Vec<u8>, Option<Vec<u8>>)>,
	key_type_id: u32,
	value_type_id: u32,
	transcoder: &ContractMessageTranscoder,
	key_filter: Option<&str>,
) -> anyhow::Result<String> {
	// Prepare optional filter string (trimmed) for comparison with decoded key rendering
	let key_filter = key_filter.map(|s| s.trim()).filter(|s| !s.is_empty());

	if pairs.is_empty() {
		return Ok("Mapping is empty".to_string());
	}

	let mut rendered_pairs: Vec<String> = Vec::new();
	for (key, val_opt) in pairs.into_iter() {
		if let Some(val) = val_opt {
			// Extract the SCALE-encoded mapping key bytes following the 20-byte prefix
			let key_bytes = if key.len() > 20 { &key[20..] } else { &[] };
			let k_decoded = transcoder.decode(key_type_id, &mut &key_bytes[..])?;
			let v_decoded = transcoder.decode(value_type_id, &mut &val[..])?;
			let k_str = k_decoded.to_string();
			if let Some(filter) = key_filter {
				if k_str == filter {
					// Found the requested key; stop early and return only the value
					return Ok(v_decoded.to_string());
				}
			} else {
				rendered_pairs.push(format!("{{ {k_str} => {v_decoded} }}"));
			}
		}
	}
	if rendered_pairs.is_empty() {
		if key_filter.is_some() {
			Ok("No value found for the provided key".to_string())
		} else {
			Ok("Mapping is empty".to_string())
		}
	} else {
		Ok(rendered_pairs.join("\n"))
	}
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
	fetch_contract_storage_with_param(storage, account, rpc_url, path, None).await
}

/// Fetches and decodes a storage value from a deployed smart contract,
/// with optional filtering for mappings.
///
/// This function retrieves the value of a storage item from a deployed smart contract.
/// For regular storage items, it returns the decoded value.
/// For mapping types (Mapping<K,V>), it can either return all key-value pairs or filter for a
/// specific key if provided.
///
/// # Arguments
/// * `storage` - Storage item descriptor containing key and type information
/// * `account` - Contract address as string (typically in H160 format)
/// * `rpc_url` - URL of the RPC endpoint to connect to
/// * `path` - Path to contract artifacts for metadata access
/// * `mapping_key` - Optional key string for filtering mapping entries. Only used if the storage
///   item is a Mapping<K,V>. The key string must be compatible with the mapping's key type K.
pub async fn fetch_contract_storage_with_param(
	storage: &ContractStorage,
	account: &str,
	rpc_url: &Url,
	path: &Path,
	mapping_key: Option<&str>,
) -> anyhow::Result<String> {
	// Get the transcoder to decode the storage value
	let transcoder = get_contract_transcoder(path)?;

	// Create RPC client
	let rpc = ContractStorageRpc::<DefaultConfig>::new(rpc_url).await?;

	// Parse account address to AccountId
	let account_id = parse_h160_account(account)?;

	// Fetch contract info to get the trie_id
	let contract_info = rpc.fetch_contract_info::<DefaultEnvironment>(&account_id).await?;
	let trie_id = contract_info.trie_id();

	// Detect if this storage item is a Mapping<K, V> from its type information
	let registry = transcoder.metadata().registry();
	if let Some(ty) = registry.resolve(storage.type_id) {
		let path = ty.path.to_string();
		if path == MAPPING_TYPE_PATH {
			return decode_mapping(storage, &rpc, trie_id, ty, &transcoder, mapping_key).await;
		}
	}

	// Non-mapping storage: fetch a single value by its root key
	// Encode the storage key as bytes: blake2_128 hash (16 bytes) + root_key (4 bytes)
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
				Some(root_layout.ty().id),
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
	root_type_id: Option<u32>,
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
				Some(root_layout.ty().id),
			);
		},
		Layout::Struct(struct_layout) => {
			// For struct layout, extract all fields with the current root key
			for field in struct_layout.fields() {
				extract_field(
					field.name(),
					field.layout(),
					registry,
					storage_items,
					root_key,
					root_type_id,
				);
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

fn try_extract_mapping(
	name: &str,
	tid: u32,
	root_key: u32,
	registry: &PortableRegistry,
	storage_items: &mut Vec<ContractStorage>,
) -> bool {
	if let Some(ty) = registry.resolve(tid) &&
		ty.path.to_string() == MAPPING_TYPE_PATH
	{
		let type_name = format_type(ty, registry);
		let key_type_name = param_type_id(ty, "K")
			.and_then(|kid| registry.resolve(kid))
			.map(|kty| format_type(kty, registry));
		storage_items.push(ContractStorage {
			name: name.to_string(),
			type_name,
			storage_key: root_key,
			type_id: tid,
			key_type_name,
		});
		return true;
	}
	false
}

// Extracts a single field and recursively processes nested layouts
fn extract_field(
	name: &str,
	layout: &Layout<PortableForm>,
	registry: &PortableRegistry,
	storage_items: &mut Vec<ContractStorage>,
	root_key: u32,
	root_type_id: Option<u32>,
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
					key_type_name: None,
				});
			}
		},
		Layout::Struct(struct_layout) => {
			// Nested struct - recursively extract its fields with qualified names
			for field in struct_layout.fields() {
				let qualified_name = format!("{}.{}", name, field.name());
				extract_field(
					&qualified_name,
					field.layout(),
					registry,
					storage_items,
					root_key,
					root_type_id,
				);
			}
		},
		Layout::Array(array_layout) => {
			// For arrays, iterate over indices and recurse into element layout
			let len = array_layout.len();
			for i in 0..len {
				let qualified_name = format!("{}[{}]", name, i);
				extract_field(
					&qualified_name,
					array_layout.layout(),
					registry,
					storage_items,
					root_key,
					root_type_id,
				);
			}
		},
		Layout::Enum(enum_layout) => {
			// For enums, iterate over variants and their fields
			for variant_layout in enum_layout.variants().values() {
				let variant_prefix = format!("{}::{}", name, variant_layout.name());
				for field in variant_layout.fields() {
					let qualified_name = format!("{}.{}", variant_prefix, field.name());
					extract_field(
						&qualified_name,
						field.layout(),
						registry,
						storage_items,
						root_key,
						root_type_id,
					);
				}
			}
		},
		Layout::Hash(hash_layout) => {
			// Hash maps (e.g., Mapping) don't have statically enumerable keys.
			// If this Root represents a Mapping<K,V>, create a single storage entry for the mapping
			// itself.
			if let Some(tid) = root_type_id &&
				try_extract_mapping(name, tid, root_key, registry, storage_items)
			{
				return;
			}
			// Otherwise, recurse into the value layout to capture leaf type information.
			extract_field(
				name,
				hash_layout.layout(),
				registry,
				storage_items,
				root_key,
				root_type_id,
			);
		},
		Layout::Root(root_layout) => {
			// Nested root updates the storage key; keep the same field name prefix
			let new_root_key = *root_layout.root_key().key();
			let tid = root_layout.ty().id;
			// Some contracts represent Mapping as a Root whose inner layout is a Leaf (value type).
			// Detect Mapping here and emit a single storage entry for the mapping container.
			if try_extract_mapping(name, tid, new_root_key, registry, storage_items) {
				return;
			}
			extract_field(
				name,
				root_layout.layout(),
				registry,
				storage_items,
				new_root_key,
				Some(tid),
			);
		},
	}
}

// Helper to extract a generic parameter type id by name (e.g., "K" or "V")
fn param_type_id(type_def: &Type<PortableForm>, param_name: &str) -> Option<u32> {
	type_def
		.type_params
		.iter()
		.find(|p| p.name == param_name)
		.and_then(|p| p.ty.as_ref())
		.map(|pt| pt.id)
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
				default: message.default(),
			})
			.collect(),
		FunctionType::Constructor => metadata
			.spec()
			.constructors()
			.iter()
			.map(|constructor| ContractFunction {
				label: constructor.label().to_string(),
				payable: constructor.payable(),
				args: process_args(constructor.args(), metadata.registry()),
				docs: collapse_docs(constructor.docs()),
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
	use scale_info::{Registry, TypeDef, TypeDefPrimitive, TypeInfo};
	use std::marker::PhantomData;
	// No need for SCALE encoding helpers in tests; for u8 values the SCALE encoding is the byte
	// itself.

	const CONTRACT_FILE: &str = "./tests/files/testing.contract";

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
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;

		// Test with a directory path
		let message = get_messages(temp_dir.path().join("testing"))?;
		assert_contract_metadata_parsed(message)?;

		// Test with a metadata file path
		let message = get_messages(current_dir.join(CONTRACT_FILE))?;
		assert_contract_metadata_parsed(message)?;

		Ok(())
	}

	#[test]
	fn get_message_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
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
	fn get_messages_and_storage_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let (messages, storage) = get_messages_and_storage(temp_dir.path().join("testing"))?;
		assert_eq!(messages.len(), 3);
		assert_eq!(messages[0].label, "flip");
		assert_eq!(messages[1].label, "get");
		assert_eq!(messages[2].label, "specific_flip");
		assert_eq!(storage.len(), 2);
		assert_eq!(storage[0].name, "value");
		assert_eq!(storage[1].name, "number");
		Ok(())
	}

	#[test]
	fn prepare_artifact_for_extrinsics_warns_and_sanitizes_incompatible_ink() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let prepared = prepare_artifact_for_extrinsics(temp_dir.path().join("testing").as_path())?;
		assert!(prepared.compatibility_warning.is_some());
		assert!(prepared.path.exists());

		let sanitized = ContractMetadata::load(&prepared.path)?;
		assert!(matches!(
			sanitized.source.language.language,
			contract_metadata::Language::Solidity
		));
		Ok(())
	}

	#[test]
	fn get_constructors_work() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
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
			current_dir.join(CONTRACT_FILE),
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
			current_dir.join(CONTRACT_FILE),
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

	#[derive(TypeInfo)]
	struct DummyKV<K, V>(PhantomData<(K, V)>);

	#[test]
	fn param_type_id_resolves_generic_k_v() -> Result<()> {
		// Build a registry that includes a dummy generic type with params named K and V
		let mut reg = Registry::new();
		let _ = reg.register_type(&scale_info::meta_type::<DummyKV<u32, bool>>());
		let portable: PortableRegistry = reg.into();
		// Find our dummy type by its last path segment
		let type_id = portable
			.types
			.iter()
			.find(|t| t.ty.path.segments.last().map(|s| s == "DummyKV").unwrap_or(false))
			.map(|t| t.id)
			.expect("dummy type must exist");
		let ty = portable.resolve(type_id).unwrap();
		// Ensure helper extracts K and V type ids
		let k_id = param_type_id(ty, "K").expect("K param must exist");
		let v_id = param_type_id(ty, "V").expect("V param must exist");
		let k_ty = portable.resolve(k_id).unwrap();
		let v_ty = portable.resolve(v_id).unwrap();
		match &k_ty.type_def {
			TypeDef::Primitive(p) => assert_eq!(*p, TypeDefPrimitive::U32),
			other => panic!("Expected primitive u32 for K, got {:?}", other),
		}
		match &v_ty.type_def {
			TypeDef::Primitive(p) => assert_eq!(*p, TypeDefPrimitive::Bool),
			other => panic!("Expected primitive bool for V, got {:?}", other),
		}
		Ok(())
	}

	// Helper to build a transcoder from the bundled testing.contract file
	fn test_transcoder() -> Result<ContractMessageTranscoder> {
		let current_dir = env::current_dir().expect("Failed to get current directory");
		get_contract_transcoder(current_dir.join("./tests/files/testing.contract").as_path())
	}

	// Helper to find the type id for a primitive in the registry
	fn find_primitive(reg: &PortableRegistry, prim: TypeDefPrimitive) -> u32 {
		reg.types
			.iter()
			.find_map(|t| match &t.ty.type_def {
				TypeDef::Primitive(p) if *p == prim => Some(t.id),
				_ => None,
			})
			.expect("primitive type must exist in registry")
	}

	#[test]
	fn decode_mapping_impl_empty_returns_message() -> Result<()> {
		let transcoder = test_transcoder()?;
		let reg = transcoder.metadata().registry();
		let u8_id = find_primitive(reg, TypeDefPrimitive::U8);

		let out = decode_mapping_impl(Vec::new(), u8_id, u8_id, &transcoder, None)?;
		assert_eq!(out, "Mapping is empty");
		Ok(())
	}

	#[test]
	fn decode_mapping_impl_renders_single_entry() -> Result<()> {
		let transcoder = test_transcoder()?;
		let reg = transcoder.metadata().registry();
		let u8_id = find_primitive(reg, TypeDefPrimitive::U8);

		// Build a full storage key: 16-byte hash + 4-byte root + SCALE(key)
		let mut full_key = vec![0u8; 16];
		full_key.extend_from_slice(&1u32.to_le_bytes());
		full_key.push(4u8); // SCALE encoding of u8 is itself
		let value_bytes = vec![8u8];

		let out = decode_mapping_impl(
			vec![(full_key, Some(value_bytes))],
			u8_id,
			u8_id,
			&transcoder,
			None,
		)?;

		assert_eq!(out, "{ 4 => 8 }");
		Ok(())
	}

	#[test]
	fn decode_mapping_impl_filter_match_returns_value_only() -> Result<()> {
		let transcoder = test_transcoder()?;
		let reg = transcoder.metadata().registry();
		let u8_id = find_primitive(reg, TypeDefPrimitive::U8);

		let mut full_key = vec![0u8; 16];
		full_key.extend_from_slice(&1u32.to_le_bytes());
		full_key.push(4u8);
		let value_bytes = vec![8u8];

		let out = decode_mapping_impl(
			vec![(full_key, Some(value_bytes))],
			u8_id,
			u8_id,
			&transcoder,
			Some("4"),
		)?;
		assert_eq!(out, "8");
		Ok(())
	}

	#[test]
	fn decode_mapping_impl_filter_no_match() -> Result<()> {
		let transcoder = test_transcoder()?;
		let reg = transcoder.metadata().registry();
		let u8_id = find_primitive(reg, TypeDefPrimitive::U8);

		let mut full_key = vec![0u8; 16];
		full_key.extend_from_slice(&1u32.to_le_bytes());
		full_key.push(4u8);
		let value_bytes = vec![8u8];

		let out = decode_mapping_impl(
			vec![(full_key, Some(value_bytes))],
			u8_id,
			u8_id,
			&transcoder,
			Some("5"),
		)?;
		assert_eq!(out, "No value found for the provided key");
		Ok(())
	}
}
