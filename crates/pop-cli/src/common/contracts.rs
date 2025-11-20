// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	common::{
		binary::{BinaryGenerator, check_and_prompt},
		wallet::prompt_to_use_wallet,
	},
	impl_binary_generator,
	style::style,
};
use cliclack::ProgressBar;
use pop_common::{AnySigner, DefaultConfig, manifest::from_path};
use pop_contracts::{
	AccountMapper, ContractFunction, DefaultEnvironment, ExtrinsicOpts, eth_rpc_generator,
	ink_node_generator,
};
use std::{
	path::{Path, PathBuf},
	process::{Child, Command},
};
use tempfile::NamedTempFile;

impl_binary_generator!(InkNodeGenerator, ink_node_generator);
impl_binary_generator!(EthRpcGenerator, eth_rpc_generator);

const CONTRACTS_NODE_BINARY: &str = "ink-node";
const ETH_RPC_BINARY: &str = "eth-rpc";

///  Checks the status of the contracts node binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_ink_node_and_prompt(
	cli: &mut impl Cli,
	spinner: &ProgressBar,
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

/// Checks if a contract has been built by verifying the existence of the build directory and the
/// {name}.contract file.
///
/// # Arguments
/// * `path` - An optional path to the project directory. If no path is provided, the current
///   directory is used.
pub fn has_contract_been_built(path: &Path) -> bool {
	let Ok(manifest) = from_path(path) else {
		return false;
	};
	manifest
		.package
		.map(|p| path.join(format!("target/ink/{}.contract", p.name())).exists())
		.unwrap_or_default()
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
		let mut input = cli
			.input(format!("Enter the value for the parameter: {}", arg.label))
			.placeholder(&format!("Type required: {}", arg.type_name));

		// Set default input only if the parameter type is `Option` (Not mandatory)
		if arg.type_name.starts_with("Option<") {
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
	extrinsic_opts: &ExtrinsicOpts<DefaultConfig, DefaultEnvironment, AnySigner>,
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
		if prompt_to_use_wallet(cli, skip_confirm)? {
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
	use crate::cli::MockCli;
	use cliclack::spinner;
	use duct::cmd;
	use pop_common::{find_free_port, set_executable_permission};
	use pop_contracts::{Param, is_chain_alive, run_eth_rpc_node, run_ink_node};
	use std::fs::{self, File};
	use url::Url;

	#[test]
	fn has_contract_been_built_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(path).run()?;
		let contract_path = path.join(name);
		assert!(!has_contract_been_built(&contract_path));

		cmd("cargo", ["build"]).dir(&contract_path).run()?;
		// Mock build directory
		fs::create_dir(contract_path.join("target/ink"))?;
		assert!(!has_contract_been_built(&path.join(name)));
		// Create a mocked .contract file inside the target directory
		File::create(contract_path.join(format!("target/ink/{}.contract", name)))?;
		assert!(has_contract_been_built(&path.join(name)));
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
			check_ink_node_and_prompt(&mut cli, &spinner(), cache_path.path(), false).await?;
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
			check_ink_node_and_prompt(&mut cli, &spinner(), cache_path.path(), true).await?;
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
		let process_1_port = find_free_port(None);
		let process_1 = run_ink_node(&binary_1.path(), None, process_1_port).await?;
		let process_2_port = find_free_port(None);
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
		use super::*;
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
		use super::*;
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
		use super::*;
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
		use super::*;
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
