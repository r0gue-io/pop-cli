// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		Cli,
		traits::{Cli as _, Confirm},
	},
	commands::call::contract::CallContractCommand,
	common::{
		contracts::{
			check_ink_node_and_prompt, has_contract_been_built, map_account, normalize_call_args,
			resolve_function_args, terminate_nodes,
		},
		rpc::prompt_to_select_chain_rpc,
		urls,
		wallet::request_signature,
	},
	style::style,
};
use clap::Args;
use cliclack::{ProgressBar, spinner};
use console::Emoji;
use pop_contracts::{
	Bytes, FunctionType, UpOpts, Verbosity, Weight, build_smart_contract,
	dry_run_gas_estimate_instantiate, dry_run_upload, extract_function, get_contract_code,
	get_instantiate_payload, get_upload_payload, instantiate_contract_signed,
	instantiate_smart_contract, is_chain_alive, parse_hex_bytes, run_eth_rpc_node, run_ink_node,
	set_up_deployment, set_up_upload, upload_contract_signed, upload_smart_contract,
};
use serde::Serialize;
use sp_core::bytes::to_hex;
use std::{path::PathBuf, process::Child};
use tempfile::NamedTempFile;
use url::Url;

const COMPLETE: &str = "🚀 Deployment complete";
const DEFAULT_PORT: u16 = 9944;
const DEFAULT_ETH_RPC_PORT: u16 = 8545;
const FAILED: &str = "🚫 Deployment failed.";
const HELP_HEADER: &str = "Smart contract deployment options";

/// Launch a local ink! node.
#[derive(Args, Clone, Serialize, Debug)]
pub(crate) struct InkNodeCommand {
	/// The port to be used for the ink! node.
	#[clap(short, long, default_value = "9944")]
	pub(crate) ink_node_port: u16,
	/// The port to be used for the Ethereum RPC node.
	#[clap(short, long, default_value = "8545")]
	pub(crate) eth_rpc_port: u16,
	/// Automatically source all necessary binaries required without prompting for confirmation.
	#[clap(short = 'y', long)]
	pub(crate) skip_confirm: bool,
}

impl InkNodeCommand {
	pub(crate) async fn execute(&self, cli: &mut Cli) -> anyhow::Result<()> {
		cli.intro("Launch a local Ink! node")?;
		let url = Url::parse(&format!("ws://localhost:{}", self.ink_node_port))?;
		let ((mut ink_node_process, ink_node_log), (mut eth_rpc_process, _)) =
			start_ink_node(&url, self.skip_confirm, self.ink_node_port, self.eth_rpc_port).await?;
		std::process::Command::new("tail")
			.args(["-F", &ink_node_log.path().to_string_lossy()])
			.spawn()?;

		// Wait for the process to terminate
		tokio::signal::ctrl_c().await?;
		ink_node_process.kill()?;
		eth_rpc_process.kill()?;
		ink_node_process.wait()?;
		eth_rpc_process.wait()?;
		cli.plain("\n")?;
		cli.outro("✅ Ink! node terminated")?;
		Ok(())
	}
}

#[derive(Args, Clone, Serialize)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpContractCommand {
	/// Path to the contract build directory.
	#[clap(skip)]
	pub(crate) path: PathBuf,
	/// The name of the contract constructor to call.
	#[clap(short, long, default_value = "new")]
	pub(crate) constructor: String,
	/// The constructor arguments, encoded as strings.
	#[clap(short, long, num_args = 0..,)]
	pub(crate) args: Vec<String>,
	/// Transfers an initial balance to the instantiated contract.
	#[clap(short, long, default_value = "0")]
	pub(crate) value: String,
	/// Maximum amount of gas to be used for this command.
	/// If not specified it will perform a dry-run to estimate the gas consumed for the
	/// instantiation.
	#[clap(name = "gas", short, long)]
	pub(crate) gas_limit: Option<u64>,
	/// Maximum proof size for the instantiation.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[clap(short = 'P', long)]
	pub(crate) proof_size: Option<u64>,
	/// A salt used in the address derivation of the new contract. Use to create multiple
	/// instances of the same contract code from the same account.
	#[clap(short = 'S', long, value_parser = parse_hex_bytes)]
	pub(crate) salt: Option<Bytes>,
	/// Websocket endpoint of a chain.
	#[clap(short, long, value_parser, default_value = urls::LOCAL)]
	pub(crate) url: Url,
	/// Secret key URI for the account deploying the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(short, long, default_value = "//Alice")]
	pub(crate) suri: String,
	/// Use a browser extension wallet to sign the extrinsic.
	#[clap(
		name = "use-wallet",
		long,
		default_value = "false",
		short('w'),
		conflicts_with = "suri"
	)]
	pub(crate) use_wallet: bool,
	/// Perform a dry-run via RPC to estimate the gas usage. This does not submit a transaction.
	#[clap(short = 'D', long)]
	pub(crate) dry_run: bool,
	/// Uploads the contract only, without instantiation.
	#[clap(short = 'U', long)]
	pub(crate) upload_only: bool,
	/// Automatically source or update the needed binary required without prompting for
	/// confirmation.
	#[clap(short = 'y', long)]
	pub(crate) skip_confirm: bool,
	/// Skip building the contract before deployment.
	/// If the contract is not built, it will be built regardless.
	#[clap(long)]
	pub(crate) skip_build: bool,
}

impl UpContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(&mut self) -> anyhow::Result<()> {
		Cli.intro("Deploy a smart contract")?;
		// Check if build exists in the specified "Contract build directory"
		let contract_already_built = has_contract_been_built(&self.path);
		if !self.skip_build || !contract_already_built {
			// Build the contract in release mode
			if !contract_already_built {
				Cli.warning("NOTE: contract has not yet been built.")?;
			}
			let spinner = spinner();
			spinner.start("Building contract in RELEASE mode...");
			let result = match build_smart_contract(&self.path, true, Verbosity::Quiet, None) {
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
		let processes = if !is_chain_alive(self.url.clone()).await? {
			let local_url = Url::parse(urls::LOCAL).expect("default url is valid");
			let start_local_node = if !self.skip_confirm {
				let msg = if self.url.as_str() == urls::LOCAL {
					"No endpoint was specified.".into()
				} else {
					format!("The specified endpoint of {} is inaccessible.", self.url)
				};

				if !Cli
					.confirm(format!(
						"{msg} Would you like to start a local node in the background for testing?",
					))
					.initial_value(true)
					.interact()?
				{
					self.url = prompt_to_select_chain_rpc(
						"Where is your contract deployed? (type to filter)",
						"Type the chain URL manually",
						urls::LOCAL,
						|n| n.supports_contracts,
						&mut Cli,
					)
					.await?;
					self.url == local_url
				} else {
					true
				}
			} else {
				Cli.outro_cancel(
					"🚫 You need to specify an accessible endpoint to deploy the contract.",
				)?;
				return Ok(());
			};

			if start_local_node {
				// Update url to that of the launched node
				self.url = local_url;

				Some(
					start_ink_node(
						&self.url,
						self.skip_confirm,
						DEFAULT_PORT,
						DEFAULT_ETH_RPC_PORT,
					)
					.await?,
				)
			} else {
				None
			}
		} else {
			None
		};

		// Run steps for signing with wallet integration. Returns early.
		if self.use_wallet {
			let (call_data, hash) = match self.get_contract_data().await {
				Ok(data) => data,
				Err(e) => {
					Cli.error(format!("An error occurred getting the call data: {e}"))?;
					terminate_nodes(&mut Cli, processes).await?;
					Cli.outro_cancel(FAILED)?;
					return Ok(());
				},
			};

			let maybe_signature_request =
				request_signature(call_data, self.url.to_string()).await?;
			if let Some(payload) = maybe_signature_request.signed_payload {
				Cli.success("Signed payload received.")?;
				let spinner = spinner();
				spinner.start(
					"Uploading the contract and waiting for finalization, please be patient...",
				);

				if self.upload_only {
					#[allow(unused_variables)]
					let upload_result = match upload_contract_signed(self.url.as_str(), payload).await {
						Err(e) => {
							spinner
								.error(format!("An error occurred uploading your contract: {e}"));
							terminate_nodes(&mut Cli, processes).await?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
						Ok(result) => {
							spinner.stop(format!(
								"Contract uploaded: The code hash is {:?}",
								to_hex(&hash, false)
							));
							result
						},
					};
				} else {
					let instantiate_exec = match set_up_deployment(self.clone().into()).await {
						Ok(i) => i,
						Err(e) => {
							Cli.error(format!(
								"An error occurred instantiating the contract: {e}"
							))?;
							terminate_nodes(&mut Cli, processes).await?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
					};
					// Check if the account is already mapped, and prompt the user to perform the
					// mapping if it's required.
					map_account(instantiate_exec.opts(), &mut Cli).await?;
					let contract_info = match instantiate_contract_signed(
						maybe_signature_request.contract_address,
						self.url.as_str(),
						payload,
					)
					.await
					{
						Err(e) => {
							spinner
								.error(format!("An error occurred uploading your contract: {e}"));
							terminate_nodes(&mut Cli, processes).await?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
						Ok(result) => result,
					};

					let hash = contract_info.code_hash.map(|code_hash| format!("{:?}", code_hash));
					display_contract_info(
						&spinner,
						format!("{:?}", contract_info.contract_address),
						hash,
					);
				};

				if self.upload_only {
					Cli.warning("NOTE: The contract has not been instantiated.")?;
				}
			} else {
				Cli.outro_cancel("Signed payload doesn't exist.")?;
				terminate_nodes(&mut Cli, processes).await?;
				return Ok(());
			}

			Cli.outro(COMPLETE)?;
			terminate_nodes(&mut Cli, processes).await?;
			return Ok(());
		}

		// Check for upload only.
		if self.upload_only {
			let result = self.upload_contract().await;
			terminate_nodes(&mut Cli, processes).await?;
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

		let function =
			extract_function(self.path.clone(), &self.constructor, FunctionType::Constructor)?;
		if !function.args.is_empty() {
			resolve_function_args(&function, &mut Cli, &mut self.args)?;
		}
		normalize_call_args(&mut self.args, &function);
		// Otherwise instantiate.
		let instantiate_exec = match set_up_deployment(self.clone().into()).await {
			Ok(i) => i,
			Err(e) => {
				Cli.error(format!("An error occurred instantiating the contract: {e}"))?;
				terminate_nodes(&mut Cli, processes).await?;
				Cli.outro_cancel(FAILED)?;
				return Ok(());
			},
		};
		// Check if the account is already mapped, and prompt the user to perform the mapping if
		// it's required.
		map_account(instantiate_exec.opts(), &mut Cli).await?;
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
					terminate_nodes(&mut Cli, processes).await?;
					Cli.outro_cancel(FAILED)?;
					return Ok(());
				},
			}
		};

		// Finally upload and instantiate.
		if !self.dry_run {
			let spinner = spinner();
			spinner.start("Uploading and instantiating the contract...");
			let contract_info = instantiate_smart_contract(instantiate_exec, weight_limit).await?;
			let contract_address = contract_info.address.to_string();
			display_contract_info(&spinner, contract_address.clone(), contract_info.code_hash);

			Cli.success(COMPLETE)?;
			self.keep_interacting_with_node(&mut Cli, contract_address).await?;
			terminate_nodes(&mut Cli, processes).await?;
		}

		Ok(())
	}

	async fn keep_interacting_with_node(
		&self,
		cli: &mut Cli,
		address: String,
	) -> anyhow::Result<()> {
		if cli
			.confirm("Do you want to keep making calls to the contract?")
			.initial_value(false)
			.interact()?
		{
			let mut cmd = CallContractCommand::default();
			cmd.path_pos = Some(self.path.clone());
			cmd.contract = Some(address);
			cmd.url = Some(self.url.clone());
			cmd.deployed = true;
			cmd.execute(cli).await?;
		}
		Ok(())
	}

	/// Uploads the contract without instantiating it.
	async fn upload_contract(&self) -> anyhow::Result<()> {
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
			Cli.warning("NOTE: The contract has not been instantiated.")?;
		}
		Ok(())
	}

	// get the call data and contract code hash
	async fn get_contract_data(&self) -> anyhow::Result<(Vec<u8>, [u8; 32])> {
		let contract_code = get_contract_code(&self.path)?;
		let hash = contract_code.code_hash();
		if self.upload_only {
			let upload_exec = set_up_upload(self.clone().into()).await?;
			let call_data =
				get_upload_payload(upload_exec, contract_code, self.url.as_str()).await?;
			Ok((call_data, hash))
		} else {
			let instantiate_exec = set_up_deployment(self.clone().into()).await?;

			let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
				Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
			} else {
				// Frontend will do dry run and update call data.
				Weight::zero()
			};
			let call_data = get_instantiate_payload(instantiate_exec, weight_limit).await?;
			Ok((call_data, hash))
		}
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

pub(crate) async fn start_ink_node(
	url: &Url,
	skip_confirm: bool,
	ink_node_port: u16,
	eth_rpc_port: u16,
) -> anyhow::Result<((Child, NamedTempFile), (Child, NamedTempFile))> {
	let log_ink_node = NamedTempFile::new()?;
	let log_eth_rpc = NamedTempFile::new()?;
	let spinner = spinner();

	// uses the cache location
	let (ink_node_binary_path, eth_rpc_binary_path) =
		match check_ink_node_and_prompt(&mut Cli, &spinner, &crate::cache()?, skip_confirm).await {
			Ok(binary_path) => binary_path,
			Err(_) => {
				Cli.outro_cancel(
					"🚫 You need to specify an accessible endpoint to deploy the contract.",
				)?;
				anyhow::bail!("Failed to start the local ink! node");
			},
		};

	spinner.start("Starting local node...");

	let ink_node_process =
		run_ink_node(&ink_node_binary_path, Some(log_ink_node.as_file()), ink_node_port).await?;
	let eth_rpc_node_process = run_eth_rpc_node(
		&eth_rpc_binary_path,
		Some(log_eth_rpc.as_file()),
		&format!("ws://localhost:{}", ink_node_port),
		eth_rpc_port,
	)
	.await?;
	spinner.clear();
	Cli.info(format!(
		"Local node started successfully:{}",
		style(format!(
			"\n{}\n{}",
			style(format!("portal: https://polkadot.js.org/apps/?rpc={}#/explorer", url)).dim(),
			style(format!("logs: tail -f {}", log_ink_node.path().display())).dim(),
		))
		.dim()
	))?;
	Cli.info(format!(
		"Ethereum RPC node started successfully:{}",
		style(format!(
			"\n{}\n{}",
			style(format!("url: ws://localhost:{}", eth_rpc_port)).dim(),
			style(format!("logs: tail -f {}", log_eth_rpc.path().display())).dim(),
		))
		.dim()
	))?;
	Ok(((ink_node_process, log_ink_node), (eth_rpc_node_process, log_eth_rpc)))
}

fn display_contract_info(spinner: &ProgressBar, address: String, code_hash: Option<String>) {
	spinner.stop(format!(
		"Contract deployed and instantiated:\n{}",
		style(format!(
			"{}\n{}",
			style(format!("{} The contract address is {:?}", console::Emoji("●", ">"), address))
				.dim(),
			code_hash
				.map(|hash| style(format!(
					"{} The contract code hash is {:?}",
					console::Emoji("●", ">"),
					hash
				))
				.dim()
				.to_string())
				.unwrap_or_default(),
		))
		.dim()
	));
}

impl Default for UpContractCommand {
	fn default() -> Self {
		Self {
			path: PathBuf::from("./"),
			constructor: "new".to_string(),
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(urls::LOCAL).expect("default url is valid"),
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: false,
			upload_only: false,
			skip_confirm: false,
			skip_build: false,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use url::Url;

	#[test]
	fn conversion_up_contract_command_to_up_opts_works() -> anyhow::Result<()> {
		let command = UpContractCommand::default();
		let opts: UpOpts = command.into();
		assert_eq!(
			opts,
			UpOpts {
				path: PathBuf::from("./"),
				constructor: "new".to_string(),
				args: vec![],
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				salt: None,
				url: Url::parse(urls::LOCAL)?,
				suri: "//Alice".to_string(),
			}
		);
		Ok(())
	}
}
