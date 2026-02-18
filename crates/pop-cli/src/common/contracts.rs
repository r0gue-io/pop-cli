// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{Spinner, traits::*},
	common::{
		binary::{BinaryGenerator, check_and_prompt},
		wallet::prompt_to_use_wallet,
	},
	impl_binary_generator,
	style::style,
};
use pop_common::{DefaultConfig, Keypair, manifest::from_path};
use pop_contracts::{
	AccountMapper, BuildMode, ContractFunction, DefaultEnvironment, ExtrinsicOpts, MetadataSpec,
	Verbosity, build_smart_contract, eth_rpc_generator, ink_node_generator,
};
use regex::{Captures, Regex};
use std::{
	path::{Path, PathBuf},
	process::{Child, Command},
	sync::LazyLock,
};
use tempfile::NamedTempFile;

impl_binary_generator!(InkNodeGenerator, ink_node_generator);
impl_binary_generator!(EthRpcGenerator, eth_rpc_generator);

const CONTRACTS_NODE_BINARY: &str = "ink-node";
const ETH_RPC_BINARY: &str = "eth-rpc";

// Precompiled regex for hex byte strings (optional 0x prefix, any even number of hex chars)
static HEX_BYTES: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^(?:0x)?[0-9a-fA-F]*$").expect("Valid hex regex"));
// Regex for fixed-size byte arrays like [u8; 32]
static FIXED_U8_ARRAY: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"^\[\s*u8\s*;\s*(\d+)\s*]$").expect("Valid fixed u8 array regex"));

///  Checks the status of the contracts node binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_ink_node_and_prompt(
	cli: &mut impl Cli,
	spinner: &Spinner,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<(PathBuf, PathBuf)> {
	let ink_node = check_and_prompt::<InkNodeGenerator>(
		cli,
		spinner,
		CONTRACTS_NODE_BINARY,
		cache_path,
		skip_confirm,
	)
	.await?;
	// This should have already been downloaded by the previous step.
	let eth_rpc =
		check_and_prompt::<EthRpcGenerator>(cli, spinner, ETH_RPC_BINARY, cache_path, skip_confirm)
			.await?;
	Ok((ink_node, eth_rpc))
}

/// Handles the optional termination of a local running node.
/// # Arguments
/// * `cli`: Command line interface.
/// * `process`: Tuple identifying the child process to terminate and its log file.
/// * `skip_confirm`: Whether to skip confirmation prompt.
pub async fn terminate_nodes(
	cli: &mut impl Cli,
	processes: Option<((Child, NamedTempFile), (Child, NamedTempFile))>,
	skip_confirm: bool,
) -> anyhow::Result<()> {
	// Prompt to close any launched node
	if let Some(mut processes) = processes {
		if skip_confirm ||
			cli.confirm("Would you like to terminate the local node?")
				.initial_value(true)
				.interact()?
		{
			processes.0.0.kill()?;
			processes.1.0.kill()?;
			processes.0.0.wait()?;
			processes.1.0.wait()?;
		} else {
			cli.warning("You can terminate the process by pressing Ctrl+C.")?;
			Command::new("tail")
				.args(["-F", &processes.0.1.path().to_string_lossy()])
				.spawn()?;
			Command::new("tail")
				.args(["-F", &processes.1.1.path().to_string_lossy()])
				.spawn()?;
			tokio::signal::ctrl_c().await?;
			cli.plain("\n")?;
		}
		cli.info("✅ Local node terminated.")?;
	}
	Ok(())
}

/// Checks if a contract has been built by verifying the existence of the {name}.contract file in
/// the project or workspace target/ink directories.
///
/// # Arguments
/// * `path` - Path to the project directory or Cargo.toml.
pub fn has_contract_been_built(path: &Path) -> bool {
	let Ok(manifest) = from_path(path) else {
		return false;
	};
	let Some(package) = manifest.package.as_ref() else {
		return false;
	};

	let project_root =
		if path.ends_with("Cargo.toml") { path.parent().unwrap_or(path) } else { path };
	find_contract_artifact_path(project_root, package.name()).is_some()
}

/// Builds contract artifacts and reports progress/errors to the user.
#[allow(dead_code)]
pub fn build_contract_artifacts(
	cli: &mut impl Cli,
	path: &Path,
	release: bool,
	verbosity: Verbosity,
	metadata: Option<MetadataSpec>,
) -> anyhow::Result<()> {
	let build_mode = if release { BuildMode::Release } else { BuildMode::Debug };
	let build_profile = if release { "RELEASE" } else { "DEBUG" };
	let spinner = cli.spinner();
	spinner.start(format!("Building contract in {build_profile} mode..."));
	let results = match build_smart_contract(path, build_mode, verbosity, metadata, None) {
		Ok(results) => results,
		Err(e) => {
			return Err(anyhow::anyhow!(
				"🚫 An error occurred building your contract: {e}\nUse `pop build` to retry with build output.",
			));
		},
	};
	spinner.stop(format!(
		"Your contract artifacts are ready. You can find them in: {}",
		results
			.iter()
			.map(|result| result.target_directory.display().to_string())
			.collect::<Vec<_>>()
			.join("\n")
	));
	Ok(())
}

fn validate_fixed_u8_array(value: &str, caps: &Captures) -> Result<(), &'static str> {
	let len: usize = caps.get(1).unwrap().as_str().parse().unwrap_or(0);
	if HEX_BYTES.is_match(value) {
		let hex = value.strip_prefix("0x").unwrap_or(value);
		if !hex.len().is_multiple_of(2) {
			return Err(
				"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix).",
			);
		}
		let expected = len.saturating_mul(2);
		if hex.len() == expected {
			Ok(())
		} else {
			Err("Invalid hex bytes length. Expected N bytes (2*N hex chars, optional 0x prefix).")
		}
	} else {
		Err("Invalid hex bytes. Provide an even-length hex string (optional 0x prefix).")
	}
}

fn validate_basic_type(value: &str, base_type: &str) -> Result<(), &'static str> {
	match base_type {
		"bool" => value
			.parse::<bool>()
			.map(|_| ())
			.map_err(|_| "Invalid boolean. Use 'true' or 'false'."),
		"u8" | "u16" | "u32" | "u64" | "u128" =>
			value.parse::<u128>().map(|_| ()).map_err(|_| "Invalid unsigned integer."),
		"i8" | "i16" | "i32" | "i64" | "i128" =>
			value.parse::<i128>().map(|_| ()).map_err(|_| "Invalid signed integer."),
		"Vec<u8>" | "[u8]" => {
			// Validate hex string for bytes
			if HEX_BYTES.is_match(value) {
				let hex = value.strip_prefix("0x").unwrap_or(value);
				if hex.len().is_multiple_of(2) {
					Ok(())
				} else {
					Err(
						"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix).",
					)
				}
			} else {
				Err("Invalid hex bytes. Provide an even-length hex string (optional 0x prefix).")
			}
		},
		"str" | "String" => Ok(()),
		_ => Ok(()), // For complex types, skip validation
	}
}

fn validate_type(value: &str, type_name: &str, is_optional: bool) -> Result<(), &'static str> {
	// Empty is valid for Optional types
	if is_optional && value.trim().is_empty() {
		return Ok(());
	}

	// Extract base type for Option<T>
	let base_type = if is_optional {
		type_name
			.strip_prefix("Option<")
			.and_then(|s| s.strip_suffix('>'))
			.unwrap_or(type_name)
			.split(':')
			.next()
			.unwrap_or(type_name)
	} else {
		type_name
	};

	if let Some(caps) = FIXED_U8_ARRAY.captures(base_type) {
		validate_fixed_u8_array(value, &caps)
	} else {
		validate_basic_type(value, base_type)
	}
}

/// Resolves function arguments by reusing provided values and prompting for any missing entries.
///
/// # Arguments
/// * `function` - The contract function containing argument definitions.
/// * `cli` - Command line interface implementation for user interaction.
/// * `args` - Argument values provided by the user. Missing items will be requested.
pub fn resolve_function_args(
	function: &ContractFunction,
	cli: &mut impl Cli,
	args: &mut Vec<String>,
	skip_confirm: bool,
) -> anyhow::Result<()> {
	if args.len() > function.args.len() {
		return Err(anyhow::anyhow!(
			"Expected {} arguments for `{}`, but received {}. Remove the extra values or run \
			 without `--args` to be prompted.",
			function.args.len(),
			function.label,
			args.len()
		));
	}

	for arg in function.args.iter().skip(args.len()) {
		if skip_confirm {
			anyhow::bail!("When skipping confirmation, all arguments must be provided.")
		}
		let is_optional = arg.type_name.starts_with("Option<");
		let type_name = arg.type_name.clone();
		let mut input = cli
			.input(format!("Enter the value for the parameter: {}", arg.label))
			.placeholder(&format!("Type required: {}", arg.type_name))
			.validate(move |value: &String| validate_type(value, &type_name, is_optional));

		// Set default input only if the parameter type is `Option` (Not mandatory)
		if is_optional {
			input = input.default_input("");
		}
		args.push(input.interact()?);
	}

	Ok(())
}

/// Normalizes contract arguments before execution.
///
/// # Arguments
/// * `args` - The mutable list of argument values provided by the user.
/// * `function` - The contract function containing argument definitions.
pub(crate) fn normalize_call_args(args: &mut [String], function: &ContractFunction) {
	for (arg, param) in args.iter_mut().zip(&function.args) {
		// If "None" return empty string
		if param.type_name.starts_with("Option<") && arg == "None" {
			*arg = "".to_string();
		}
		// For `str` params, ensure the value is wrapped in double quotes.
		else if param.type_name == "str" {
			*arg = ensure_double_quoted(arg);
		}
	}
}

/// Ensures a string is wrapped in double quotes.
fn ensure_double_quoted(s: &str) -> String {
	let trimmed = s.trim();
	if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
		trimmed.to_string()
	} else {
		format!("\"{}\"", trimmed)
	}
}

pub(crate) async fn map_account(
	extrinsic_opts: &ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair>,
	cli: &mut impl Cli,
) -> anyhow::Result<()> {
	let mapper = AccountMapper::new(extrinsic_opts).await?;
	if mapper.needs_mapping().await? &&
		cli.confirm("Your account is not yet mapped. Would you like to map it?")
			.initial_value(true)
			.interact()?
	{
		let address = mapper.map_account().await?;
		cli.success(format!(
			"Account mapped successfully.\n{}",
			style(format!("Address {:?}.", address)).dim()
		))?;
	}
	Ok(())
}

/// Resolves the signer for contract operations (deployment or calls).
/// If neither `--suri` nor `--use-wallet` was provided, prompts the user to choose, unless
/// `--skip-confirm` was provided.`
///
/// # Arguments
/// * `skip_confirm` - Whether to skip the confirmation prompt.
/// * `use_wallet` - Mutable reference to the use_wallet flag.
/// * `suri` - Mutable reference to the optional suri string.
/// * `cli` - The CLI instance for user interaction.
///
/// # Returns
/// * `Ok(())` if signer was resolved successfully.
/// * `Err` if there was an error during prompting.
pub fn resolve_signer(
	skip_confirm: bool,
	use_wallet: &mut bool,
	suri: &mut Option<String>,
	cli: &mut impl Cli,
) -> anyhow::Result<()> {
	if suri.is_none() {
		if skip_confirm {
			if !*use_wallet {
				anyhow::bail!(
					"When skipping confirmation, a signer must be provided via --use-wallet or --suri."
				)
			}
			return Ok(());
		}
		if prompt_to_use_wallet(cli)? {
			*use_wallet = true;
		} else {
			*suri = Some(
				cli.input("Specify the signer:")
					.placeholder("//Alice")
					.default_input("//Alice")
					.interact()?,
			);
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::{MockCli, Spinner};
	use pop_common::{resolve_port, set_executable_permission};
	use pop_contracts::{Param, Verbosity, is_chain_alive, run_eth_rpc_node, run_ink_node};
	use std::{
		fs::{self, File},
		path::{Path, PathBuf},
	};
	use url::Url;

	// Minimal package layout for manifest parsing without shelling out to cargo.
	fn write_package(root: &Path, name: &str) -> anyhow::Result<PathBuf> {
		let package_root = root.join(name);
		fs::create_dir_all(package_root.join("src"))?;
		fs::write(
			package_root.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"),
		)?;
		fs::write(package_root.join("src/lib.rs"), "")?;
		Ok(package_root)
	}

	#[test]
	fn validate_type_optional_empty_is_ok() {
		// Empty (or whitespace) is valid for Option<T>
		assert!(validate_type("", "Option<u32>", true).is_ok());
		assert!(validate_type("   ", "Option<bool>", true).is_ok());
	}

	#[test]
	fn validate_type_bool_parsing() {
		// Valid booleans
		assert!(validate_type("true", "bool", false).is_ok());
		assert!(validate_type("false", "bool", false).is_ok());
		// Invalid boolean (case sensitive)
		let err = validate_type("TRUE", "bool", false).unwrap_err();
		assert_eq!(err, "Invalid boolean. Use 'true' or 'false'.");
	}

	#[test]
	fn validate_type_unsigned_integers() {
		// All unsigned map to u128 parsing internally
		assert!(validate_type("0", "u8", false).is_ok());
		assert!(validate_type("255", "u8", false).is_ok());
		assert!(validate_type("42", "u32", false).is_ok());
		// Invalid values
		let err = validate_type("-1", "u32", false).unwrap_err();
		assert_eq!(err, "Invalid unsigned integer.");
		let err = validate_type("abc", "u128", false).unwrap_err();
		assert_eq!(err, "Invalid unsigned integer.");
	}

	#[test]
	fn validate_type_signed_integers() {
		assert!(validate_type("0", "i32", false).is_ok());
		assert!(validate_type("-42", "i64", false).is_ok());
		let err = validate_type("not-a-number", "i128", false).unwrap_err();
		assert_eq!(err, "Invalid signed integer.");
	}

	#[test]
	fn validate_type_string_types_are_ok() {
		assert!(validate_type("hello", "str", false).is_ok());
		assert!(validate_type("hello", "String", false).is_ok());
		// Optional string: empty is allowed, non-empty is also allowed
		assert!(validate_type("", "Option<str>", true).is_ok());
		assert!(validate_type("world", "Option<String>", true).is_ok());
	}

	#[test]
	fn validate_type_unknown_types_are_accepted() {
		// Unknown/complex types are skipped by validation and accepted
		assert!(validate_type("{complex}", "MyCrate::module::Type", false).is_ok());
	}

	#[test]
	fn validate_type_vec_u8_and_slice_hex_ok() {
		// Accept both with and without 0x, case-insensitive, any even length (including zero)
		for ty in ["Vec<u8>", "[u8]"] {
			assert!(validate_type("", ty, false).is_ok());
			assert!(validate_type("0x", ty, false).is_ok());
			assert!(validate_type("00", ty, false).is_ok());
			assert!(validate_type("0x00", ty, false).is_ok());
			assert!(validate_type("00ff", ty, false).is_ok());
			assert!(validate_type("ABcd", ty, false).is_ok());
			assert!(validate_type("0xdeadBEEF", ty, false).is_ok());
		}
	}

	#[test]
	fn validate_type_vec_u8_and_slice_hex_invalid() {
		for ty in ["Vec<u8>", "[u8]"] {
			let err = validate_type("0x0", ty, false).unwrap_err();
			assert_eq!(
				err,
				"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
			);
			let err = validate_type("abc", ty, false).unwrap_err(); // odd length
			assert_eq!(
				err,
				"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
			);
			let err = validate_type("0xz1", ty, false).unwrap_err(); // non-hex char
			assert_eq!(
				err,
				"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
			);
			let err = validate_type("gh", ty, false).unwrap_err(); // non-hex chars
			assert_eq!(
				err,
				"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
			);
		}
	}

	#[test]
	fn validate_type_option_vec_u8_empty_and_non_empty() {
		// Empty is accepted for Option<Vec<u8>>
		assert!(validate_type("", "Option<Vec<u8>>", true).is_ok());
		// Non-empty must be valid hex
		assert!(validate_type("0x00ff", "Option<Vec<u8>>", true).is_ok());
		let err = validate_type("zz", "Option<Vec<u8>>", true).unwrap_err();
		assert_eq!(
			err,
			"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
		);
	}

	#[test]
	fn validate_type_fixed_u8_array_ok() {
		// [u8;20] => 20 bytes => 40 hex chars
		assert!(validate_type(&"00".repeat(20), "[u8;20]", false).is_ok());
		assert!(validate_type(&format!("0x{}", "ab".repeat(20)), "[u8;20]", false).is_ok());
		// [u8;32] => 64 hex chars
		assert!(validate_type(&"ff".repeat(32), "[u8;32]", false).is_ok());
		assert!(validate_type(&format!("0x{}", "DE".repeat(32)), "[u8;32]", false).is_ok());
		// [u8;64] => 128 hex chars
		assert!(validate_type(&"01".repeat(64), "[u8;64]", false).is_ok());
		assert!(validate_type(&format!("0x{}", "Cd".repeat(64)), "[u8;64]", false).is_ok());
	}

	#[test]
	fn validate_type_fixed_u8_array_invalid_length() {
		// Too short
		let err = validate_type("00", "[u8;20]", false).unwrap_err();
		assert_eq!(
			err,
			"Invalid hex bytes length. Expected N bytes (2*N hex chars, optional 0x prefix)."
		);
		// Too long
		let err = validate_type(&"00".repeat(21), "[u8;20]", false).unwrap_err();
		assert_eq!(
			err,
			"Invalid hex bytes length. Expected N bytes (2*N hex chars, optional 0x prefix)."
		);
	}

	#[test]
	fn validate_type_fixed_u8_array_invalid_hex() {
		// Odd length after 0x
		let err = validate_type("0x0", "[u8;20]", false).unwrap_err();
		assert_eq!(
			err,
			"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
		);
		// Non-hex characters
		let err = validate_type("zz", "[u8;20]", false).unwrap_err();
		assert_eq!(
			err,
			"Invalid hex bytes. Provide an even-length hex string (optional 0x prefix)."
		);
	}

	#[test]
	fn validate_type_option_fixed_u8_array() {
		// Empty accepted for Option<[u8;N]>
		assert!(validate_type("", "Option<[u8;20]>", true).is_ok());
		// Correct length hex
		assert!(validate_type(&"aa".repeat(20), "Option<[u8;20]>", true).is_ok());
		// Wrong length
		let err = validate_type(&"aa".repeat(19), "Option<[u8;20]>", true).unwrap_err();
		assert_eq!(
			err,
			"Invalid hex bytes length. Expected N bytes (2*N hex chars, optional 0x prefix)."
		);
	}

	#[test]
	fn validate_type_option_numeric_non_empty_validated() {
		// When Option<T> has a non-empty value, it should validate against base type
		assert!(validate_type("5", "Option<u32>", true).is_ok());
		let err = validate_type("oops", "Option<u64>", true).unwrap_err();
		assert_eq!(err, "Invalid unsigned integer.");
	}

	#[test]
	fn has_contract_been_built_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Project-local target/ink lookup.
		let name = "hello_world";
		let contract_path = write_package(path, name)?;
		assert!(!has_contract_been_built(&contract_path));

		// Mock build directory
		fs::create_dir_all(contract_path.join("target/ink"))?;
		assert!(!has_contract_been_built(&contract_path));
		// Create a mocked .contract file inside the target directory
		File::create(contract_path.join(format!("target/ink/{}.contract", name)))?;
		assert!(has_contract_been_built(&contract_path));

		// Workspace target/ink lookup.
		let workspace_root = path.join("workspace");
		fs::create_dir(&workspace_root)?;
		fs::write(workspace_root.join("Cargo.toml"), "[workspace]\nmembers = [\"member\"]\n")?;
		let member_path = write_package(&workspace_root, "member")?;
		fs::create_dir_all(workspace_root.join("target/ink"))?;
		File::create(workspace_root.join("target/ink/member.contract"))?;
		assert!(has_contract_been_built(&member_path));

		Ok(())
	}

	#[test]
	fn build_contract_artifacts_reports_error() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let err = build_contract_artifacts(temp_dir.path(), true, Verbosity::Quiet, None)
			.expect_err("expected build to fail without a Cargo.toml");
		assert!(err.to_string().contains("Use `pop build` to retry with build output."));
		Ok(())
	}

	#[test]
	fn resolve_function_args_works() -> anyhow::Result<()> {
		let function = ContractFunction {
			label: "new".to_string(),
			payable: false,
			args: vec![Param { label: "init_value".into(), type_name: "bool".into() }],
			docs: String::new(),
			default: false,
			mutates: true,
		};
		let mut cli = MockCli::new()
			.expect_input("Enter the value for the parameter: init_value", "true".into());
		let mut args = Vec::new();
		resolve_function_args(&function, &mut cli, &mut args, false)?;
		assert_eq!(args, vec!["true"]);
		cli.verify()
	}

	#[test]
	fn resolve_function_args_respects_existing() -> anyhow::Result<()> {
		let mut cli =
			MockCli::new().expect_input("Enter the value for the parameter: number", "2".into());
		let function = ContractFunction {
			label: "specific_flip".to_string(),
			payable: true,
			args: vec![
				Param { label: "new_value".into(), type_name: "bool".into() },
				Param { label: "number".into(), type_name: "Option<u32>".into() },
			],
			docs: String::new(),
			default: false,
			mutates: true,
		};
		let mut args = vec!["true".to_string()];
		resolve_function_args(&function, &mut cli, &mut args, false)?;
		assert_eq!(args, vec!["true", "2"]);
		cli.verify()
	}

	#[test]
	fn resolve_function_args_preserves_preprovided_args() -> anyhow::Result<()> {
		let function = ContractFunction {
			label: "specific_flip".into(),
			payable: true,
			args: vec![
				Param { label: "new_value".into(), type_name: "bool".into() },
				Param { label: "number".into(), type_name: "Option<u32>".into() },
			],
			docs: String::new(),
			default: false,
			mutates: true,
		};

		let mut cli = MockCli::new();
		let mut args = vec!["true".to_string(), "Some(2)".to_string()];
		resolve_function_args(&function, &mut cli, &mut args, false)?;
		assert_eq!(args, vec!["true".to_string(), "Some(2)".to_string()]);
		cli.verify()
	}

	#[tokio::test]
	async fn check_contracts_node_and_prompt_works() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {CONTRACTS_NODE_BINARY} binary is not found."))
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning(format!("⚠️ The {CONTRACTS_NODE_BINARY} binary is not found."));

		let node_path =
			check_ink_node_and_prompt(&mut cli, &Spinner::Mock, cache_path.path(), false).await?;
		// Binary path is at least equal to the cache path + the contracts node binary.
		assert!(
			node_path
				.0
				.to_str()
				.unwrap()
				.starts_with(cache_path.path().join(CONTRACTS_NODE_BINARY).to_str().unwrap())
		);
		cli.verify()
	}

	#[tokio::test]
	async fn check_contracts_node_and_prompt_handles_skip_confirm() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {CONTRACTS_NODE_BINARY} binary is not found."));

		let node_path =
			check_ink_node_and_prompt(&mut cli, &Spinner::Mock, cache_path.path(), true).await?;
		// Binary path is at least equal to the cache path + the contracts node binary.
		assert!(
			node_path
				.0
				.to_str()
				.unwrap()
				.starts_with(cache_path.path().join(CONTRACTS_NODE_BINARY).to_str().unwrap())
		);
		cli.verify()
	}

	#[tokio::test]
	async fn node_is_terminated() -> anyhow::Result<()> {
		let cache = tempfile::tempdir().expect("Could not create temp dir");
		let binary_1 = ink_node_generator(PathBuf::from(cache.path()), None).await?;
		binary_1.source(false, &(), true).await?;
		set_executable_permission(binary_1.path())?;
		let binary_2 = eth_rpc_generator(PathBuf::from(cache.path()), None).await?;
		binary_2.source(false, &(), true).await?;
		set_executable_permission(binary_2.path())?;
		let process_1_port = resolve_port(None);
		let process_1 = run_ink_node(&binary_1.path(), None, process_1_port).await?;
		let process_2_port = resolve_port(None);
		let chain_url = format!("ws://127.0.0.1:{}", process_1_port);
		let process_2 =
			run_eth_rpc_node(&binary_2.path(), None, &chain_url, process_2_port).await?;
		let log_1 = NamedTempFile::new()?;
		let log_2 = NamedTempFile::new()?;
		// Terminate the process.
		let mut cli =
			MockCli::new().expect_confirm("Would you like to terminate the local node?", true);
		assert!(
			terminate_nodes(&mut cli, Some(((process_1, log_1), (process_2, log_2))), false)
				.await
				.is_ok()
		);
		assert!(!is_chain_alive(Url::parse(&chain_url)?).await?);
		cli.verify()
	}

	#[test]
	fn resolve_signing_method_with_explicit_suri_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let mut use_wallet = false;
		let mut suri = Some("//Bob".to_string());
		resolve_signer(true, &mut use_wallet, &mut suri, &mut cli)?;
		assert!(!use_wallet);
		assert_eq!(suri, Some("//Bob".to_string()));
		Ok(())
	}

	#[test]
	fn resolve_signing_method_with_use_wallet_flag_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_confirm(
			"Do you want to use your browser wallet to sign the extrinsic? (Selecting 'No' will prompt you to manually enter the secret key URI for signing, e.g., '//Alice')",
			true,
		);
		let mut use_wallet = true;
		let mut suri = None;
		// When skip_confirm is false, we prompt and choose wallet usage.
		resolve_signer(false, &mut use_wallet, &mut suri, &mut cli)?;
		assert!(use_wallet);
		assert_eq!(suri, None);
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn resolve_signing_method_prompts_and_chooses_wallet_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_confirm(
			"Do you want to use your browser wallet to sign the extrinsic? (Selecting 'No' will prompt you to manually enter the secret key URI for signing, e.g., '//Alice')",
			true,
		);
		let mut use_wallet = false;
		let mut suri = None;
		resolve_signer(false, &mut use_wallet, &mut suri, &mut cli)?;
		assert!(use_wallet);
		assert_eq!(suri, None);
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn resolve_signing_method_prompts_and_provides_suri_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_confirm(
				"Do you want to use your browser wallet to sign the extrinsic? (Selecting 'No' will prompt you to manually enter the secret key URI for signing, e.g., '//Alice')",
				false,
			)
			.expect_input("Specify the signer:", "//Charlie".to_string());
		let mut use_wallet = false;
		let mut suri = None;
		resolve_signer(false, &mut use_wallet, &mut suri, &mut cli)?;
		assert!(!use_wallet);
		assert_eq!(suri, Some("//Charlie".to_string()));
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn resolve_signer_bails_when_skip_confirm_and_no_signer() {
		let mut cli = MockCli::new();
		let mut use_wallet = false;
		let mut suri = None;
		let res = resolve_signer(true, &mut use_wallet, &mut suri, &mut cli);
		assert!(res.is_err());
		assert_eq!(
			res.unwrap_err().to_string(),
			"When skipping confirmation, a signer must be provided via --use-wallet or --suri."
		);
	}

	#[test]
	fn resolve_signer_with_skip_confirm_and_use_wallet_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let mut use_wallet = true;
		let mut suri = None;
		resolve_signer(true, &mut use_wallet, &mut suri, &mut cli)?;
		assert!(use_wallet);
		assert_eq!(suri, None);
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn normalize_call_args_works() {
		let function = ContractFunction {
			label: "test".to_string(),
			payable: false,
			args: vec![
				Param { label: "test_string".to_string(), type_name: "str".to_string() },
				Param { label: "test_none".to_string(), type_name: "Option<str>".to_string() },
			],
			docs: "test".to_string(),
			default: false,
			mutates: false,
		};
		let mut args = vec!["test".to_string(), "None".to_string()];
		normalize_call_args(&mut args, &function);
		assert_eq!(args, vec!["\"test\"", ""]);
	}

	#[test]
	fn ensure_double_quoted_works() {
		assert_eq!(ensure_double_quoted("hello"), "\"hello\"");
		assert_eq!(ensure_double_quoted("  hello  "), "\"hello\"");
		assert_eq!(ensure_double_quoted("\"hello\""), "\"hello\"");
	}

	#[test]
	fn resolve_function_args_requires_all_args_when_skip_confirm() -> anyhow::Result<()> {
		let function = ContractFunction {
			label: "test".into(),
			payable: false,
			args: vec![Param { label: "a".into(), type_name: "u32".into() }],
			docs: String::new(),
			default: false,
			mutates: true,
		};
		let mut cli = MockCli::new();
		let mut args: Vec<String> = vec![];
		let res = resolve_function_args(&function, &mut cli, &mut args, true);
		assert!(res.is_err());
		Ok(())
	}
}
