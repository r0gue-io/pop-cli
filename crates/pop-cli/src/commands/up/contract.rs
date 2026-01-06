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
			resolve_function_args, resolve_signer, terminate_nodes,
		},
		rpc::prompt_to_select_chain_rpc,
		urls,
		wallet::request_signature,
	},
	style::style,
};
use clap::Args;
use cliclack::spinner;
use console::Emoji;
use pop_common::resolve_port;
use pop_contracts::{
	FunctionType, UpOpts, Verbosity, Weight, build_smart_contract,
	dry_run_gas_estimate_instantiate, dry_run_upload, extract_function, get_contract_code,
	get_instantiate_payload, get_upload_payload, instantiate_contract_signed,
	instantiate_smart_contract, is_chain_alive, run_eth_rpc_node, run_ink_node, set_up_deployment,
	set_up_upload, upload_contract_signed, upload_smart_contract,
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

#[derive(Clone, Copy, Debug)]
struct ResolvedPorts {
	ink_node_port: u16,
	eth_rpc_port: u16,
}

fn resolve_ink_node_ports(ink_node_port: u16, eth_rpc_port: u16) -> anyhow::Result<ResolvedPorts> {
	let ink_explicit = ink_node_port != DEFAULT_PORT;
	let eth_explicit = eth_rpc_port != DEFAULT_ETH_RPC_PORT;

	// Fail fast if both same.
	if ink_node_port == eth_rpc_port {
		anyhow::bail!(
			"ink! node port and Ethereum RPC port cannot be the same ({})",
			ink_node_port
		);
	}

	// Resolve ink node port.
	let resolved_ink = resolve_port(Some(ink_node_port), &[]);
	if ink_explicit && resolved_ink != ink_node_port {
		anyhow::bail!("ink! node port {} is in use", ink_node_port);
	}

	// Resolve eth rpc port, always avoiding ink's resolved port.
	let resolved_eth = resolve_port(Some(eth_rpc_port), &[resolved_ink]);
	if eth_explicit && resolved_eth != eth_rpc_port {
		anyhow::bail!("Ethereum RPC port {} is in use", eth_rpc_port);
	}

	Ok(ResolvedPorts { ink_node_port: resolved_ink, eth_rpc_port: resolved_eth })
}

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
	/// Automatically detach from the terminal and run the node in the background.
	#[clap(short, long)]
	pub(crate) detach: bool,
}

impl InkNodeCommand {
	pub(crate) async fn execute(&self, cli: &mut Cli) -> anyhow::Result<()> {
		cli.intro("Launch a local Ink! node")?;
		let ports = match resolve_ink_node_ports(self.ink_node_port, self.eth_rpc_port) {
			Ok(ports) => ports,
			Err(e) => {
				cli.error(e.to_string())?;
				cli.outro_cancel("🚫 Unable to start local ink! node.")?;
				return Ok(());
			},
		};
		let url = Url::parse(&format!("ws://localhost:{}", ports.ink_node_port))?;
		let ((mut ink_node_process, ink_node_log), (mut eth_rpc_process, eth_rpc_log)) =
			start_ink_node(&url, self.skip_confirm, ports.ink_node_port, ports.eth_rpc_port)
				.await?;

		if !self.detach {
			// Wait for the process to terminate
			cli.info("Press Control+C to exit")?;
			tokio::signal::ctrl_c().await?;
			ink_node_process.kill()?;
			eth_rpc_process.kill()?;
			ink_node_process.wait()?;
			eth_rpc_process.wait()?;
			cli.plain("\n")?;
			cli.outro("✅ Ink! node terminated")?;
		} else {
			ink_node_log.keep()?;
			eth_rpc_log.keep()?;
			cli.outro(format!(
				"✅ Ink! node bootstrapped successfully. Run `kill -9 {} {}` to terminate it.",
				ink_node_process.id(),
				eth_rpc_process.id()
			))?;
		}
		Ok(())
	}
}

#[derive(Args, Clone, Serialize)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpContractCommand {
	/// Path to the contract build directory.
	#[serde(skip_serializing)]
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
	#[clap(name = "gas", short, long, requires = "proof_size")]
	pub(crate) gas_limit: Option<u64>,
	/// Maximum proof size for the instantiation.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[clap(short = 'P', long, requires = "gas")]
	pub(crate) proof_size: Option<u64>,
	/// Websocket endpoint of a chain.
	#[clap(short, long, value_parser)]
	pub(crate) url: Option<Url>,
	/// Secret key URI for the account deploying the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[serde(skip_serializing)]
	#[clap(short, long)]
	pub(crate) suri: Option<String>,
	/// Use a browser extension wallet to sign the extrinsic.
	#[clap(
		name = "use-wallet",
		long,
		default_value = "false",
		short('w'),
		conflicts_with = "suri"
	)]
	pub(crate) use_wallet: bool,
	/// Actually deploy the contract. Otherwise a dry-run is performed.
	#[clap(short = 'x', long)]
	pub(crate) execute: bool,
	/// Uploads the contract only, without instantiation.
	#[clap(short = 'U', long)]
	pub(crate) upload_only: bool,
	/// Automatically source or update the needed binary required without prompting for
	/// confirmation. When no `--url` is provided (falls back to the local default), this will
	/// automatically start the ink! node if no endpoint is reachable.
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
			let results = match build_smart_contract(&self.path, true, Verbosity::Quiet, None) {
				Ok(results) => results,
				Err(e) => {
					Cli.outro_cancel(format!("🚫 An error occurred building your contract: {e}\nUse `pop build` to retry with build output."))?;
					return Ok(());
				},
			};
			spinner.stop(format!(
				"Your contract artifacts are ready. You can find them in: {}",
				results
					.iter()
					.map(|r| r.target_directory.display().to_string())
					.collect::<Vec<_>>()
					.join("\n")
			));
		}

		// Resolve who is deploying the contract. If a `suri` was provided via the command line,
		// skip the prompt.
		if let Err(e) =
			resolve_signer(self.skip_confirm, &mut self.use_wallet, &mut self.suri, &mut Cli)
		{
			Cli.error(e.to_string())?;
			Cli.outro_cancel(FAILED)?;
			return Ok(());
		}

		let mut url = if let Some(url) = self.url.clone() {
			url
		} else if self.skip_confirm {
			Url::parse(urls::LOCAL).expect("default url is valid")
		} else {
			prompt_to_select_chain_rpc(
				"Where do you want to deploy your contract? (type to filter)",
				"Type the chain URL manually",
				urls::LOCAL,
				|n| n.supports_contracts,
				&mut Cli,
			)
			.await?
		};
		// Check if specified chain is accessible
		let processes = if !is_chain_alive(url.clone()).await? {
			let local_url = Url::parse(urls::LOCAL).expect("default url is valid");
			if url == local_url {
				if self.skip_confirm ||
					Cli.confirm(
						"No local ink! node detected. Would you like to start it node in the background for testing?",
					)
					.initial_value(true)
					.interact()?
				{
					Cli.info("Fetching and launching a local ink! node")?;
					let ports = resolve_ink_node_ports(DEFAULT_PORT, DEFAULT_ETH_RPC_PORT)?;
					url = Url::parse(&format!("ws://localhost:{}", ports.ink_node_port))?;
					Some(
						start_ink_node(
							&url,
							self.skip_confirm,
							ports.ink_node_port,
							ports.eth_rpc_port,
						)
						.await?,
					)
				} else {
					Cli.outro_cancel(
						"🚫 You need to specify an accessible endpoint to deploy the contract.",
					)?;
					return Ok(());
				}
			} else {
				Cli.outro_cancel(
					"🚫 You need to specify an accessible endpoint to deploy the contract.",
				)?;
				return Ok(());
			}
		} else {
			None
		};
		self.url = Some(url.clone());

		// Track the deployed contract address across both deployment flows.
		let mut deployed_contract_address: Option<String> = None;

		// Resolve constructor arguments
		if !self.upload_only {
			let function =
				extract_function(self.path.clone(), &self.constructor, FunctionType::Constructor)?;
			if !function.args.is_empty() {
				resolve_function_args(&function, &mut Cli, &mut self.args, self.skip_confirm)?;
			}
			normalize_call_args(&mut self.args, &function);
		}

		// Run steps for signing with wallet integration.
		if self.use_wallet {
			let (call_data, hash) = match self.get_contract_data().await {
				Ok(data) => data,
				Err(e) => {
					Cli.error(format!("An error occurred getting the call data: {e}"))?;
					terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
					Cli.outro_cancel(FAILED)?;
					return Ok(());
				},
			};

			let maybe_payload = request_signature(call_data, url.to_string()).await?;
			if let Some(payload) = maybe_payload {
				Cli.success("Signed payload received.")?;
				let spinner = spinner();
				spinner.start(
					"Uploading the contract and waiting for finalization, please be patient...",
				);

				if self.upload_only {
					#[allow(unused_variables)]
					let upload_result = match upload_contract_signed(url.as_str(), payload).await {
						Err(e) => {
							spinner
								.error(format!("An error occurred uploading your contract: {e}"));
							terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
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
					Cli.warning("NOTE: The contract has not been instantiated.")?;
				} else {
					let instantiate_exec = match set_up_deployment(self.clone().into()).await {
						Ok(i) => i,
						Err(e) => {
							Cli.error(format!(
								"An error occurred instantiating the contract: {e}"
							))?;
							terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
					};
					// Check if the account is already mapped, and prompt the user to perform the
					// mapping if it's required.
					map_account(instantiate_exec.opts(), &mut Cli).await?;
					let contract_info = match instantiate_contract_signed(url.as_str(), payload)
						.await
					{
						Err(e) => {
							spinner
								.error(format!("An error occurred uploading your contract: {e}"));
							terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
						Ok(result) => result,
					};

					let contract_address = format!("{:?}", contract_info.contract_address);
					let hash = contract_info.code_hash.map(|code_hash| format!("{:?}", code_hash));
					spinner.clear();
					display_contract_info(&mut Cli, contract_address.clone(), hash)?;
					// Store the contract address for later interaction prompt.
					deployed_contract_address = Some(contract_address);
				};
			} else {
				Cli.outro_cancel("Signed payload doesn't exist.")?;
				terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
				return Ok(());
			}
		} else {
			// Check for upload only.
			if self.upload_only {
				let result = self.upload_contract().await;
				match result {
					Ok(_) => {},
					Err(_) => {
						terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
						Cli.outro_cancel(FAILED)?;
						return Ok(());
					},
				}
			} else {
				// Otherwise instantiate.
				let instantiate_exec = match set_up_deployment(self.clone().into()).await {
					Ok(i) => i,
					Err(e) => {
						Cli.error(format!("An error occurred instantiating the contract: {e}"))?;
						terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
						Cli.outro_cancel(FAILED)?;
						return Ok(());
					},
				};
				// Check if the account is already mapped, and prompt the user to perform the
				// mapping if it's required.
				map_account(instantiate_exec.opts(), &mut Cli).await?;

				// Perform the dry run before attempting to execute the deployment, and since we are
				// on it also to calculate the weight.
				let spinner_1 = spinner();
				spinner_1.start("Doing a dry run...");
				let calculated_weight =
					match dry_run_gas_estimate_instantiate(&instantiate_exec).await {
						Ok(w) => {
							spinner_1.stop(format!("Gas limit estimate: {:?}", w));
							w
						},
						Err(e) => {
							spinner_1.error(format!("{e}"));
							terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
					};

				let weight_limit = if self.gas_limit.is_some() & self.proof_size.is_some() {
					Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
				} else {
					calculated_weight
				};

				// Confirm whether to execute if not specified via flag.
				if !self.execute {
					if self.skip_confirm {
						Cli.warning("NOTE: The contract has not been instantiated.")?;
					} else if Cli
						.confirm(
							"Do you want to deploy the contract? (Selecting 'No' will keep this as a dry run)",
						)
						.initial_value(true)
						.interact()?
					{
						self.execute = true;
					} else {
						Cli.warning("NOTE: The contract has not been instantiated.")?;
					}
				}

				// Finally upload and instantiate.
				if self.execute {
					let spinner_2 = spinner();
					spinner_2.start("Uploading and instantiating the contract...");
					let contract_info =
						instantiate_smart_contract(instantiate_exec, weight_limit).await?;
					let contract_address = contract_info.address.to_string();
					spinner_2.clear();
					display_contract_info(
						&mut Cli,
						contract_address.clone(),
						contract_info.code_hash,
					)?;
					// Store the contract address for later interaction prompt.
					deployed_contract_address = Some(contract_address);
				}
			}
		}

		// Prompt to keep interacting with the contract if one was deployed and skip_confirm is
		// false.
		if let Some(contract_address) = deployed_contract_address {
			if !self.skip_confirm {
				Cli.success(COMPLETE)?;
				self.keep_interacting_with_node(&mut Cli, contract_address).await?;
				terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
			} else {
				terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
				Cli.outro(COMPLETE)?;
			}
		} else {
			terminate_nodes(&mut Cli, processes, self.skip_confirm).await?;
			Cli.outro(COMPLETE)?;
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
			cmd.url = self.url.clone();
			cmd.deployed = true;
			cmd.execute = self.execute;
			cmd.use_wallet = self.use_wallet;
			cmd.suri = self.suri.clone();
			cmd.skip_confirm = self.skip_confirm;
			cmd.execute(cli).await?;
		}
		Ok(())
	}

	/// Uploads the contract without instantiating it.
	async fn upload_contract(&self) -> anyhow::Result<()> {
		let upload_exec = set_up_upload(self.clone().into()).await?;
		if !self.execute {
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
			let call_data = get_upload_payload(
				upload_exec,
				contract_code,
				self.url.as_ref().expect("url must be defined").as_str(),
			)
			.await?;
			Ok((call_data, hash))
		} else {
			let instantiate_exec = set_up_deployment(self.clone().into()).await?;

			let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
				Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
			} else {
				dry_run_gas_estimate_instantiate(&instantiate_exec)
					.await
					.unwrap_or_else(|_| Weight::zero())
			};
			// Skip storage deposit estimation when using wallet (UI will handle it)
			let storage_deposit_limit = if self.use_wallet { Some(0) } else { None };
			let call_data =
				get_instantiate_payload(instantiate_exec, weight_limit, storage_deposit_limit)
					.await?;
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
			url: cmd.url.expect("url must be set"),
			suri: cmd.suri.unwrap_or_else(|| "//Alice".to_string()),
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
			"\n{}\n{}\n{}",
			style(format!("portal: https://polkadot.js.org/apps/?rpc={}#/explorer", url)).dim(),
			style(format!("url: {}", url)).dim(),
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

fn display_contract_info(
	cli: &mut Cli,
	address: String,
	code_hash: Option<String>,
) -> anyhow::Result<()> {
	cli.info(format!(
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
	))?;
	Ok(())
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
			url: None,
			suri: Some("//Alice".to_string()),
			use_wallet: false,
			execute: true,
			upload_only: false,
			skip_confirm: false,
			skip_build: false,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use clap::Parser;
	use url::Url;

	#[test]
	fn conversion_up_contract_command_to_up_opts_works() -> anyhow::Result<()> {
		let command =
			UpContractCommand { url: Some(Url::parse(urls::LOCAL)?), ..Default::default() };
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
				url: Url::parse(urls::LOCAL)?,
				suri: "//Alice".to_string(),
			}
		);
		Ok(())
	}

	#[test]
	fn test_ink_node_command_clap_defaults() {
		// Build a tiny clap::Parser that flattens InkNodeCommand so we can parse args
		#[derive(clap::Parser)]
		struct TestParser {
			#[command(flatten)]
			cmd: InkNodeCommand,
		}

		let parsed = TestParser::parse_from(["pop-cli-test"]);
		let cmd = parsed.cmd;

		assert_eq!(cmd.ink_node_port, 9944);
		assert_eq!(cmd.eth_rpc_port, 8545);
		assert!(!cmd.skip_confirm);
	}

	#[test]
	fn test_ink_node_command_clap_overrides() {
		#[derive(clap::Parser, Debug)]
		struct TestParser {
			#[command(flatten)]
			cmd: InkNodeCommand,
		}

		let parsed = TestParser::parse_from([
			"pop-cli-test",
			"--ink-node-port",
			"12000",
			"--eth-rpc-port",
			"13000",
			"-y", // skip_confirm
		]);
		let cmd = parsed.cmd;

		assert_eq!(cmd.ink_node_port, 12000);
		assert_eq!(cmd.eth_rpc_port, 13000);
		assert!(cmd.skip_confirm);
	}

	#[test]
	fn resolve_ink_node_ports_uses_explicit_ports() {
		let ports = resolve_ink_node_ports(12001, 13001).expect("explicit ports should work");
		assert_eq!(ports.ink_node_port, 12001);
		assert_eq!(ports.eth_rpc_port, 13001);
	}

	#[test]
	fn resolve_ink_node_ports_errors_on_same_explicit_ports() {
		let result = resolve_ink_node_ports(12000, 12000);
		assert!(result.is_err());
	}

	#[test]
	fn resolve_ink_node_ports_errors_on_busy_explicit_port() {
		use std::net::TcpListener;
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let busy_port = listener.local_addr().unwrap().port();

		let result = resolve_ink_node_ports(busy_port, DEFAULT_ETH_RPC_PORT);
		assert!(result.is_err());
		let result = resolve_ink_node_ports(DEFAULT_PORT, busy_port);
		assert!(result.is_err());
	}

	#[test]
	fn resolve_ink_node_ports_resolves_defaults() {
		let ports = resolve_ink_node_ports(DEFAULT_PORT, DEFAULT_ETH_RPC_PORT)
			.expect("default ports should resolve");
		assert_eq!(ports.ink_node_port, DEFAULT_PORT);
		assert_eq!(ports.eth_rpc_port, DEFAULT_ETH_RPC_PORT);
	}
}
