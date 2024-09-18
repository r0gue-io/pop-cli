// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	common::contracts::check_contracts_node_and_prompt,
	style::style,
};
use clap::Args;
use cliclack::{confirm, log, log::error, spinner};
use console::{Emoji, Style};
use pop_common::manifest::from_path;
use pop_contracts::{
	build_smart_contract, dry_run_gas_estimate_instantiate, dry_run_upload,
	instantiate_smart_contract, is_chain_alive, parse_hex_bytes, run_contracts_node,
	set_up_deployment, set_up_upload, upload_smart_contract, UpOpts, Verbosity,
};
use sp_core::Bytes;
use sp_weights::Weight;
use std::{
	path::{Path, PathBuf},
	process::{Child, Command},
};
use tempfile::NamedTempFile;
use url::Url;

const COMPLETE: &str = "🚀 Deployment complete";
const DEFAULT_URL: &str = "ws://localhost:9944/";
const FAILED: &str = "🚫 Deployment failed.";

#[derive(Args, Clone)]
pub struct UpContractCommand {
	/// Path to the contract build directory.
	#[arg(short = 'p', long)]
	path: Option<PathBuf>,
	/// The name of the contract constructor to call.
	#[clap(name = "constructor", long, default_value = "new")]
	constructor: String,
	/// The constructor arguments, encoded as strings.
	#[clap(long, num_args = 0..)]
	args: Vec<String>,
	/// Transfers an initial balance to the instantiated contract.
	#[clap(name = "value", long, default_value = "0")]
	value: String,
	/// Maximum amount of gas to be used for this command.
	/// If not specified it will perform a dry-run to estimate the gas consumed for the
	/// instantiation.
	#[clap(name = "gas", long)]
	gas_limit: Option<u64>,
	/// Maximum proof size for the instantiation.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[clap(long)]
	proof_size: Option<u64>,
	/// A salt used in the address derivation of the new contract. Use to create multiple
	/// instances of the same contract code from the same account.
	#[clap(long, value_parser = parse_hex_bytes)]
	salt: Option<Bytes>,
	/// Websocket endpoint of a chain.
	#[clap(name = "url", long, value_parser, default_value = DEFAULT_URL)]
	url: Url,
	/// Secret key URI for the account deploying the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short, default_value = "//Alice")]
	suri: String,
	/// Perform a dry-run via RPC to estimate the gas usage. This does not submit a transaction.
	#[clap(long)]
	dry_run: bool,
	/// Uploads the contract only, without instantiation.
	#[clap(short('u'), long)]
	upload_only: bool,
	/// Automatically source or update the needed binary required without prompting for
	/// confirmation.
	#[clap(short('y'), long)]
	skip_confirm: bool,
}

impl UpContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> anyhow::Result<()> {
		Cli.intro("Deploy a smart contract")?;

		// Check if build exists in the specified "Contract build directory"
		if !has_contract_been_built(self.path.as_deref()) {
			// Build the contract in release mode
			Cli.warning("NOTE: contract has not yet been built.")?;
			let spinner = spinner();
			spinner.start("Building contract in RELEASE mode...");
			let result = match build_smart_contract(self.path.as_deref(), true, Verbosity::Quiet) {
				Ok(result) => result,
				Err(e) => {
					Cli.outro_cancel(format!("🚫 An error occurred building your contract: {e}\nUse `pop build` to retry with build output."))?;
					return Ok(());
				},
			};
			spinner.stop(format!(
				"Your contract artifacts are ready. You can find them in: {}",
				result.target_directory.display()
			));
		}

		// Check if specified chain is accessible
		let process = if !is_chain_alive(self.url.clone()).await? {
			if !self.skip_confirm {
				let chain = if self.url.as_str() == DEFAULT_URL {
					"No endpoint was specified.".into()
				} else {
					format!("The specified endpoint of {} is inaccessible.", self.url)
				};

				if !confirm(format!(
					"{chain} Would you like to start a local node in the background for testing?",
				))
				.initial_value(true)
				.interact()?
				{
					Cli.outro_cancel(
						"🚫 You need to specify an accessible endpoint to deploy the contract.",
					)?;
					return Ok(());
				}
			}

			// Update url to that of the launched node
			self.url = Url::parse(DEFAULT_URL).expect("default url is valid");

			let log = NamedTempFile::new()?;

			// uses the cache location
			let binary_path = match check_contracts_node_and_prompt(self.skip_confirm).await {
				Ok(binary_path) => binary_path,
				Err(_) => {
					Cli.outro_cancel(
						"🚫 You need to specify an accessible endpoint to deploy the contract.",
					)?;
					return Ok(());
				},
			};

			let spinner = spinner();
			spinner.start("Starting local node...");

			let process = run_contracts_node(binary_path, Some(log.as_file())).await?;
			let bar = Style::new().magenta().dim().apply_to(Emoji("│", "|"));
			spinner.stop(format!(
				"Local node started successfully:{}",
				style(format!(
					"
{bar}  {}
{bar}  {}",
					style(format!(
						"portal: https://polkadot.js.org/apps/?rpc={}#/explorer",
						self.url
					))
					.dim(),
					style(format!("logs: tail -f {}", log.path().display())).dim(),
				))
				.dim()
			));
			Some((process, log))
		} else {
			None
		};

		// Check for upload only.
		if self.upload_only {
			let result = self.upload_contract().await;
			Self::terminate_node(process)?;
			match result {
				Ok(_) => {
					Cli.outro(COMPLETE)?;
				},
				Err(_) => {
					Cli.outro_cancel(FAILED)?;
				},
			}
			return Ok(());
		}

		// Otherwise instantiate.
		let instantiate_exec = match set_up_deployment(UpOpts {
			path: self.path.clone(),
			constructor: self.constructor.clone(),
			args: self.args.clone(),
			value: self.value.clone(),
			gas_limit: self.gas_limit,
			proof_size: self.proof_size,
			salt: self.salt.clone(),
			url: self.url.clone(),
			suri: self.suri.clone(),
		})
		.await
		{
			Ok(i) => i,
			Err(e) => {
				error(format!("An error occurred instantiating the contract: {e}"))?;
				Self::terminate_node(process)?;
				Cli.outro_cancel(FAILED)?;
				return Ok(());
			},
		};

		let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
			Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
		} else {
			let spinner = spinner();
			spinner.start("Doing a dry run to estimate the gas...");
			match dry_run_gas_estimate_instantiate(&instantiate_exec).await {
				Ok(w) => {
					spinner.stop(format!("Gas limit estimate: {:?}", w));
					w
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					Self::terminate_node(process)?;
					Cli.outro_cancel(FAILED)?;
					return Ok(());
				},
			}
		};

		// Finally upload and instantiate.
		if !self.dry_run {
			let spinner = spinner();
			spinner.start("Uploading and instantiating the contract...");
			let contract_address =
				instantiate_smart_contract(instantiate_exec, weight_limit).await?;
			spinner.stop(format!(
				"Contract deployed and instantiated: The Contract Address is {:?}",
				contract_address
			));
			Self::terminate_node(process)?;
			Cli.outro(COMPLETE)?;
		}

		Ok(())
	}

	/// Uploads the contract without instantiating it.
	async fn upload_contract(self) -> anyhow::Result<()> {
		let upload_exec = set_up_upload(self.clone().into()).await?;
		if self.dry_run {
			match dry_run_upload(&upload_exec).await {
				Ok(upload_result) => {
					let mut result = vec![format!("Code Hash: {:?}", upload_result.code_hash)];
					result.push(format!("Deposit: {:?}", upload_result.deposit));
					let result: Vec<_> = result
						.iter()
						.map(|s| style(format!("{} {s}", Emoji("●", ">"))).dim().to_string())
						.collect();
					Cli.success(format!("Dry run successful!\n{}", result.join("\n")))?;
				},
				Err(_) => {
					Cli.outro_cancel(FAILED)?;
					return Ok(());
				},
			};
		} else {
			let spinner = spinner();
			spinner.start("Uploading your contract...");
			let code_hash = match upload_smart_contract(&upload_exec).await {
				Ok(r) => r,
				Err(e) => {
					spinner.error(format!("An error occurred uploading your contract: {e}"));
					return Err(e.into());
				},
			};
			spinner.stop(format!("Contract uploaded: The code hash is {:?}", code_hash));
			log::warning("NOTE: The contract has not been instantiated.")?;
		}
		Ok(())
	}

	/// Handles the optional termination of a local running node.
	fn terminate_node(process: Option<(Child, NamedTempFile)>) -> anyhow::Result<()> {
		// Prompt to close any launched node
		let Some((process, log)) = process else {
			return Ok(());
		};
		if confirm("Would you like to terminate the local node?")
			.initial_value(true)
			.interact()?
		{
			// Stop the process contracts-node
			Command::new("kill")
				.args(["-s", "TERM", &process.id().to_string()])
				.spawn()?
				.wait()?;
		} else {
			log.keep()?;
			log::warning(format!("NOTE: The node is running in the background with process ID {}. Please terminate it manually when done.", process.id()))?;
		}

		Ok(())
	}
}

impl From<UpContractCommand> for UpOpts {
	fn from(cmd: UpContractCommand) -> Self {
		UpOpts {
			path: cmd.path,
			constructor: cmd.constructor,
			args: cmd.args,
			value: cmd.value,
			gas_limit: cmd.gas_limit,
			proof_size: cmd.proof_size,
			salt: cmd.salt,
			url: cmd.url,
			suri: cmd.suri,
		}
	}
}

/// Checks if a contract has been built by verifying the existence of the build directory and the
/// <name>.contract file.
///
/// # Arguments
/// * `path` - An optional path to the project directory. If no path is provided, the current
///   directory is used.
pub fn has_contract_been_built(path: Option<&Path>) -> bool {
	let project_path = path.unwrap_or_else(|| Path::new("./"));
	let manifest = match from_path(Some(project_path)) {
		Ok(manifest) => manifest,
		Err(_) => return false,
	};
	let contract_name = manifest.package().name();
	project_path.join("target/ink").exists() &&
		project_path.join(format!("target/ink/{}.contract", contract_name)).exists()
}

#[cfg(test)]
mod tests {
	use super::*;
	use duct::cmd;
	use std::fs::{self, File};
	use url::Url;

	#[test]
	fn conversion_up_contract_command_to_up_opts_works() -> anyhow::Result<()> {
		let command = UpContractCommand {
			path: None,
			constructor: "new".to_string(),
			args: vec!["false".to_string()].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse("ws://localhost:9944")?,
			suri: "//Alice".to_string(),
			dry_run: false,
			upload_only: false,
			skip_confirm: false,
		};
		let opts: UpOpts = command.into();
		assert_eq!(
			opts,
			UpOpts {
				path: None,
				constructor: "new".to_string(),
				args: vec!["false".to_string()].to_vec(),
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				salt: None,
				url: Url::parse("ws://localhost:9944")?,
				suri: "//Alice".to_string(),
			}
		);
		Ok(())
	}

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
}
