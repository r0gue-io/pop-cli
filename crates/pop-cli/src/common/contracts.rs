// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	common::binary::{check_and_prompt, BinaryGenerator},
	impl_binary_generator,
};
use pop_common::{manifest::from_path, sourcing::Binary};
use pop_contracts::{contracts_node_generator, ContractFunction};
use std::{
	path::{Path, PathBuf},
	process::{Child, Command},
};
use tempfile::NamedTempFile;
#[cfg(feature = "polkavm-contracts")]
use {
	crate::style::style,
	pop_common::{DefaultConfig, Keypair},
	pop_contracts::{AccountMapper, DefaultEnvironment, ExtrinsicOpts},
};

impl_binary_generator!(ContractsNodeGenerator, contracts_node_generator);

#[cfg(feature = "wasm-contracts")]
const CONTRACTS_NODE_BINARY: &str = "substrate-contracts-node";
#[cfg(feature = "polkavm-contracts")]
const CONTRACTS_NODE_BINARY: &str = "ink-node";

///  Checks the status of the contracts node binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_contracts_node_and_prompt(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	check_and_prompt::<ContractsNodeGenerator>(cli, CONTRACTS_NODE_BINARY, cache_path, skip_confirm)
		.await
}

/// Handles the optional termination of a local running node.
/// # Arguments
/// * `cli`: Command line interface.
/// * `process`: Tuple identifying the child process to terminate and its log file.
pub async fn terminate_node(
	cli: &mut impl Cli,
	process: Option<(Child, NamedTempFile)>,
) -> anyhow::Result<()> {
	// Prompt to close any launched node
	let Some((process, log)) = process else {
		return Ok(());
	};
	if cli
		.confirm("Would you like to terminate the local node?")
		.initial_value(true)
		.interact()?
	{
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;
	} else {
		cli.warning("You can terminate the process by pressing Ctrl+C.")?;
		Command::new("tail").args(["-F", &log.path().to_string_lossy()]).spawn()?;
		tokio::signal::ctrl_c().await?;
		cli.plain("\n")?;
		cli.success("✅ Local node terminated.")?;
	}

	Ok(())
}

/// Checks if a contract has been built by verifying the existence of the build directory and the
/// {name}.contract file.
///
/// # Arguments
/// * `path` - An optional path to the project directory. If no path is provided, the current
///   directory is used.
pub fn has_contract_been_built(path: Option<&Path>) -> bool {
	let project_path = path.unwrap_or_else(|| Path::new("./"));
	let Ok(manifest) = from_path(Some(project_path)) else {
		return false;
	};
	manifest
		.package
		.map(|p| project_path.join(format!("target/ink/{}.contract", p.name())).exists())
		.unwrap_or_default()
}

/// Requests and collects function arguments from the user via CLI interaction.
///
/// # Arguments
/// * `function` - The contract function containing argument definitions.
/// * `cli` - Command line interface implementation for user interaction.
///
/// # Returns
/// A vector of strings containing the user-provided argument values.
pub fn request_contract_function_args(
	function: &ContractFunction,
	cli: &mut impl Cli,
) -> anyhow::Result<Vec<String>> {
	let mut user_provided_args = Vec::new();
	for arg in &function.args {
		let mut input = cli
			.input(format!("Enter the value for the parameter: {}", arg.label))
			.placeholder(&format!("Type required: {}", arg.type_name));

		// Set default input only if the parameter type is `Option` (Not mandatory)
		if arg.type_name.starts_with("Option<") {
			input = input.default_input("");
		}
		user_provided_args.push(input.interact()?);
	}
	Ok(user_provided_args)
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

#[cfg(feature = "polkavm-contracts")]
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use pop_common::{find_free_port, set_executable_permission};
	use pop_contracts::{
		extract_function, is_chain_alive, run_contracts_node, FunctionType, Param,
	};
	use std::{
		env,
		fs::{self, File},
	};
	use url::Url;

	#[test]
	fn has_contract_been_built_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		let contract_path = path.join(name);
		assert!(!has_contract_been_built(Some(&contract_path)));

		cmd("cargo", ["build"]).dir(&contract_path).run()?;
		// Mock build directory
		fs::create_dir(&contract_path.join("target/ink"))?;
		assert!(!has_contract_been_built(Some(&path.join(name))));
		// Create a mocked .contract file inside the target directory
		File::create(contract_path.join(format!("target/ink/{}.contract", name)))?;
		assert!(has_contract_been_built(Some(&path.join(name))));
		Ok(())
	}

	#[tokio::test]
	async fn request_contract_function_args_works() -> anyhow::Result<()> {
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		let mut cli = MockCli::new()
			.expect_input("Enter the value for the parameter: init_value", "true".into());
		let function = extract_function(
			current_dir.join("pop-contracts/tests/files/testing.json"),
			"new",
			FunctionType::Constructor,
		)?;
		assert_eq!(request_contract_function_args(&function, &mut cli)?, vec!["true"]);
		cli.verify()
	}

	#[tokio::test]
	async fn check_contracts_node_and_prompt_works() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {CONTRACTS_NODE_BINARY} binary is not found."))
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning(format!("⚠️ The {CONTRACTS_NODE_BINARY} binary is not found."));

		let node_path = check_contracts_node_and_prompt(&mut cli, cache_path.path(), false).await?;
		// Binary path is at least equal to the cache path + the contracts node binary.
		assert!(node_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join(CONTRACTS_NODE_BINARY).to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn check_contracts_node_and_prompt_handles_skip_confirm() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {CONTRACTS_NODE_BINARY} binary is not found."));

		let node_path = check_contracts_node_and_prompt(&mut cli, cache_path.path(), true).await?;
		// Binary path is at least equal to the cache path + the contracts node binary.
		assert!(node_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join(CONTRACTS_NODE_BINARY).to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn node_is_terminated() -> anyhow::Result<()> {
		let cache = tempfile::tempdir().expect("Could not create temp dir");
		let binary = contracts_node_generator(PathBuf::from(cache.path()), None).await?;
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;
		let port = find_free_port(None);
		let process = run_contracts_node(binary.path(), None, port).await?;
		let log = NamedTempFile::new()?;
		// Terminate the process.
		let mut cli =
			MockCli::new().expect_confirm("Would you like to terminate the local node?", true);
		assert!(terminate_node(&mut cli, Some((process, log))).await.is_ok());
		assert_eq!(is_chain_alive(Url::parse(&format!("ws://localhost:{}", port))?).await?, false);
		cli.verify()
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
}
