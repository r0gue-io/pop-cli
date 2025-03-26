// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	common::{
		contracts::{check_contracts_node_and_prompt, has_contract_been_built, terminate_node},
		wallet::request_signature,
	},
	style::style,
};
use clap::Args;
use cliclack::{confirm, log, log::error, spinner, ProgressBar};
use console::{Emoji, Style};
use pop_contracts::{
	build_smart_contract, dry_run_gas_estimate_instantiate, dry_run_upload,
	get_code_hash_from_event, get_contract_code, get_instantiate_payload, get_upload_payload,
	instantiate_contract_signed, instantiate_smart_contract, is_chain_alive, parse_hex_bytes,
	run_contracts_node, set_up_deployment, set_up_upload, upload_contract_signed,
	upload_smart_contract, UpOpts, Verbosity,
};
use sp_core::Bytes;
use sp_weights::Weight;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use url::Url;

const COMPLETE: &str = "🚀 Deployment complete";
const DEFAULT_URL: &str = "ws://localhost:9944/";
const DEFAULT_PORT: u16 = 9944;
const FAILED: &str = "🚫 Deployment failed.";
const HELP_HEADER: &str = "Smart contract deployment options";

#[derive(Args, Clone)]
#[clap(next_help_heading = HELP_HEADER)]
pub struct UpContractCommand {
	/// Path to the contract build directory.
	#[clap(skip)]
	pub(crate) path: Option<PathBuf>,
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
	#[clap(short, long, value_parser, default_value = DEFAULT_URL)]
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
	// Deprecation flag, used to specify whether the deprecation warning is shown (will be removed
	// in v0.8.0).
	#[clap(skip)]
	pub(crate) valid: bool,
}

impl UpContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> anyhow::Result<()> {
		Cli.intro("Deploy a smart contract")?;
		// Show warning if specified as deprecated.
		if !self.valid {
			Cli.warning("DEPRECATION: Please use `pop up` (or simply `pop u`) in the future...")?;
		}
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
			let binary_path = match check_contracts_node_and_prompt(
				&mut Cli,
				&crate::cache()?,
				self.skip_confirm,
			)
			.await
			{
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

			let process =
				run_contracts_node(binary_path, Some(log.as_file()), DEFAULT_PORT).await?;
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

		// Run steps for signing with wallet integration. Returns early.
		if self.use_wallet {
			let (call_data, hash) = match self.get_contract_data().await {
				Ok(data) => data,
				Err(e) => {
					error(format!("An error occurred getting the call data: {e}"))?;
					terminate_node(&mut Cli, process)?;
					Cli.outro_cancel(FAILED)?;
					return Ok(());
				},
			};

			let maybe_payload = request_signature(call_data, self.url.to_string()).await?;
			if let Some(payload) = maybe_payload {
				log::success("Signed payload received.")?;
				let spinner = spinner();
				spinner.start(
					"Uploading the contract and waiting for finalization, please be patient...",
				);

				if self.upload_only {
					let upload_result = match upload_contract_signed(self.url.as_str(), payload)
						.await
					{
						Err(e) => {
							spinner
								.error(format!("An error occurred uploading your contract: {e}"));
							terminate_node(&mut Cli, process)?;
							Cli.outro_cancel(FAILED)?;
							return Ok(());
						},
						Ok(result) => result,
					};

					match get_code_hash_from_event(&upload_result, hash) {
						Ok(r) => {
							spinner.stop(format!("Contract uploaded: The code hash is {:?}", r));
						},
						Err(e) => {
							spinner
								.error(format!("An error occurred uploading your contract: {e}"));
						},
					};
				} else {
					let contract_info =
						match instantiate_contract_signed(self.url.as_str(), payload).await {
							Err(e) => {
								spinner.error(format!(
									"An error occurred uploading your contract: {e}"
								));
								terminate_node(&mut Cli, process)?;
								Cli.outro_cancel(FAILED)?;
								return Ok(());
							},
							Ok(result) => result,
						};

					let hash = contract_info.code_hash.map(|code_hash| format!("{:?}", code_hash));
					display_contract_info(
						&spinner,
						contract_info.contract_address.to_string(),
						hash,
					);
				};

				if self.upload_only {
					log::warning("NOTE: The contract has not been instantiated.")?;
				}
			} else {
				Cli.outro_cancel("Signed payload doesn't exist.")?;
				terminate_node(&mut Cli, process)?;
				return Ok(());
			}

			terminate_node(&mut Cli, process)?;
			Cli.outro(COMPLETE)?;
			return Ok(());
		}

		// Check for upload only.
		if self.upload_only {
			let result = self.upload_contract().await;
			terminate_node(&mut Cli, process)?;
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
		let instantiate_exec = match set_up_deployment(self.clone().into()).await {
			Ok(i) => i,
			Err(e) => {
				error(format!("An error occurred instantiating the contract: {e}"))?;
				terminate_node(&mut Cli, process)?;
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
					terminate_node(&mut Cli, process)?;
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
			display_contract_info(
				&spinner,
				contract_info.address.to_string(),
				contract_info.code_hash,
			);

			terminate_node(&mut Cli, process)?;
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

	// get the call data and contract code hash
	async fn get_contract_data(&self) -> anyhow::Result<(Vec<u8>, [u8; 32])> {
		let contract_code = get_contract_code(self.path.as_ref())?;
		let hash = contract_code.code_hash();
		if self.upload_only {
			let call_data = get_upload_payload(contract_code, self.url.as_str()).await?;
			Ok((call_data, hash))
		} else {
			let instantiate_exec = set_up_deployment(self.clone().into()).await?;

			let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
				Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
			} else {
				// Frontend will do dry run and update call data.
				Weight::zero()
			};
			let call_data = get_instantiate_payload(instantiate_exec, weight_limit)?;
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

#[cfg(test)]
mod tests {
	use super::*;
	use pop_common::{find_free_port, set_executable_permission};
	use pop_contracts::{contracts_node_generator, mock_build_process, new_environment};
	use std::{
		env,
		process::{Child, Command},
		time::Duration,
	};
	use subxt::{tx::Payload, SubstrateConfig};
	use tempfile::TempDir;
	use tokio::time::sleep;
	use url::Url;

	fn default_up_contract_command() -> UpContractCommand {
		UpContractCommand {
			path: None,
			constructor: "new".to_string(),
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse("ws://localhost:9944").expect("default url is valid"),
			suri: "//Alice".to_string(),
			dry_run: false,
			upload_only: false,
			skip_confirm: false,
			use_wallet: false,
			valid: true,
		}
	}

	async fn start_test_environment() -> anyhow::Result<(Child, u16, TempDir)> {
		let random_port = find_free_port(None);
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("../pop-contracts/tests/files/testing.contract"),
			current_dir.join("../pop-contracts/tests/files/testing.json"),
		)?;
		let cache = temp_dir.path().join("");
		let binary = contracts_node_generator(cache.clone(), None).await?;
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;
		let process = run_contracts_node(binary.path(), None, random_port).await?;
		Ok((process, random_port, temp_dir))
	}

	fn stop_test_environment(id: &str) -> anyhow::Result<()> {
		Command::new("kill").args(["-s", "TERM", id]).spawn()?.wait()?;
		Ok(())
	}

	#[test]
	fn conversion_up_contract_command_to_up_opts_works() -> anyhow::Result<()> {
		let command = default_up_contract_command();
		let opts: UpOpts = command.into();
		assert_eq!(
			opts,
			UpOpts {
				path: None,
				constructor: "new".to_string(),
				args: vec![].to_vec(),
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

	#[tokio::test]
	async fn get_upload_and_instantiate_call_data_works() -> anyhow::Result<()> {
		let (contracts_node_process, port, temp_dir) = start_test_environment().await?;
		sleep(Duration::from_secs(5)).await;

		get_upload_call_data_works(port, temp_dir.path().join("testing")).await?;
		get_instantiate_call_data_works(port, temp_dir.path().join("testing")).await?;

		// Stop running contracts-node
		stop_test_environment(&contracts_node_process.id().to_string())?;
		Ok(())
	}

	async fn get_upload_call_data_works(port: u16, temp_dir: PathBuf) -> anyhow::Result<()> {
		let localhost_url = format!("ws://127.0.0.1:{}", port);

		let up_contract_opts = UpContractCommand {
			path: Some(temp_dir),
			constructor: "new".to_string(),
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(&localhost_url).expect("given url is valid"),
			suri: "//Alice".to_string(),
			dry_run: false,
			upload_only: true,
			skip_confirm: true,
			use_wallet: true,
			valid: true,
		};

		let rpc_client = subxt::backend::rpc::RpcClient::from_url(&up_contract_opts.url).await?;
		let client = subxt::OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;

		// Retrieve call data based on the above command options.
		let (retrieved_call_data, _) = match up_contract_opts.get_contract_data().await {
			Ok(data) => data,
			Err(e) => {
				error(format!("An error occurred getting the call data: {e}"))?;
				return Err(e);
			},
		};
		// We have retrieved some payload.
		assert!(!retrieved_call_data.is_empty());

		// Craft encoded call data for an upload code call.
		let contract_code = get_contract_code(up_contract_opts.path.as_ref())?;
		let storage_deposit_limit: Option<u128> = None;
		let upload_code = contract_extrinsics::extrinsic_calls::UploadCode::new(
			contract_code,
			storage_deposit_limit,
			contract_extrinsics::upload::Determinism::Enforced,
		);
		let expected_call_data = upload_code.build();
		let mut encoded_expected_call_data = Vec::<u8>::new();
		expected_call_data
			.encode_call_data_to(&client.metadata(), &mut encoded_expected_call_data)?;

		// Retrieved call data and calculated match.
		assert_eq!(retrieved_call_data, encoded_expected_call_data);
		Ok(())
	}

	async fn get_instantiate_call_data_works(port: u16, temp_dir: PathBuf) -> anyhow::Result<()> {
		let localhost_url = format!("ws://127.0.0.1:{}", port);

		let up_contract_opts = UpContractCommand {
			path: Some(temp_dir),
			constructor: "new".to_string(),
			args: vec!["false".to_string()],
			value: "0".to_string(),
			gas_limit: Some(200_000_000),
			proof_size: Some(30_000),
			salt: None,
			url: Url::parse(&localhost_url).expect("given url is valid"),
			suri: "//Alice".to_string(),
			dry_run: false,
			upload_only: false,
			skip_confirm: true,
			use_wallet: true,
			valid: true,
		};

		// Retrieve call data based on the above command options.
		let (retrieved_call_data, _) = match up_contract_opts.get_contract_data().await {
			Ok(data) => data,
			Err(e) => {
				error(format!("An error occurred getting the call data: {e}"))?;
				return Err(e);
			},
		};
		// We have retrieved some payload.
		assert!(!retrieved_call_data.is_empty());

		// Craft instantiate call data.
		let weight = Weight::from_parts(200_000_000, 30_000);
		let expected_call_data =
			get_instantiate_payload(set_up_deployment(up_contract_opts.into()).await?, weight)?;
		// Retrieved call data matches the one crafted above.
		assert_eq!(retrieved_call_data, expected_call_data);

		Ok(())
	}
}
