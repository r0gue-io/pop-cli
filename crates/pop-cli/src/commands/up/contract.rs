// SPDX-License-Identifier: GPL-3.0

use crate::style::style;
use clap::Args;
use cliclack::{clear_screen, confirm, intro, log, log::success, outro, outro_cancel};
use pop_contracts::{
	build_smart_contract, dry_run_gas_estimate_instantiate, dry_run_upload,
	instantiate_smart_contract, is_chain_alive, parse_hex_bytes, run_contracts_node,
	set_up_deployment, set_up_upload, upload_smart_contract, UpOpts,
};
use sp_core::Bytes;
use sp_weights::Weight;
use std::path::PathBuf;

#[derive(Args, Clone)]
pub struct UpContractCommand {
	/// Path to the contract build folder.
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
	/// Websocket endpoint of a node.
	#[clap(name = "url", long, value_parser, default_value = "ws://localhost:9944")]
	url: url::Url,
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
	/// Before start a local node, do not ask the user for confirmation.
	#[clap(short('y'), long)]
	skip_confirm: bool,
}

impl UpContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;

		// Check if build exists in the specified "Contract build folder"
		let build_path = PathBuf::from(
			self.path.clone().unwrap_or("/.".into()).to_string_lossy().to_string() + "/target/ink",
		);

		if !build_path.as_path().exists() {
			log::warning("NOTE: contract has not yet been built.")?;
			intro(format!("{}: Building a contract", style(" Pop CLI ").black().on_magenta()))?;
			// Build the contract in release mode
			let result = build_smart_contract(self.path.as_deref(), true)?;
			log::success(result.to_string())?;
		}

		if !is_chain_alive(self.url.clone()).await? {
			if !self.skip_confirm {
				if !confirm(format!(
				"The chain \"{}\" is not live. Would you like pop to start a local node in the background for testing?",
				self.url.to_string()
			))
			.interact()?
			{
				outro_cancel("You need to specify a live chain to deploy the contract.")?;
				return Ok(());
			}
			}
			let process = run_contracts_node(crate::cache()?).await?;
			log::success("Local node started successfully in the background.")?;
			log::warning(format!("NOTE: The contracts node is running in the background with process ID {}. Please close it manually when done testing.", process.id()))?;
		}

		// if build exists then proceed
		intro(format!("{}: Deploy a smart contract", style(" Pop CLI ").black().on_magenta()))?;

		if self.upload_only {
			return self.upload_contract().await;
		}

		let instantiate_exec = set_up_deployment(UpOpts {
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
		.await?;

		let weight_limit;
		if self.gas_limit.is_some() && self.proof_size.is_some() {
			weight_limit = Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap());
		} else {
			let spinner = cliclack::spinner();
			spinner.start("Doing a dry run to estimate the gas...");
			weight_limit = match dry_run_gas_estimate_instantiate(&instantiate_exec).await {
				Ok(w) => {
					log::info(format!("Gas limit: {:?}", w))?;
					w
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					outro_cancel("Deployment failed.")?;
					return Ok(());
				},
			};
		}
		if !self.dry_run {
			let spinner = cliclack::spinner();
			spinner.start("Uploading and instantiating the contract...");
			let contract_address =
				instantiate_smart_contract(instantiate_exec, weight_limit).await?;
			spinner.stop(format!(
				"Contract deployed and instantiated: The Contract Address is {:?}",
				contract_address
			));
			outro("Deployment complete")?;
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
						.map(|s| {
							style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string()
						})
						.collect();
					success(format!("Dry run successful!\n{}", result.join("\n")))?;
				},
				Err(_) => {
					outro_cancel("Deployment failed.")?;
					return Ok(());
				},
			};
		} else {
			let spinner = cliclack::spinner();
			spinner.start("Uploading the contract...");
			let code_hash = upload_smart_contract(&upload_exec).await?;
			spinner.stop(format!("Contract uploaded: The code hash is {:?}", code_hash));
			outro("Deployment complete")?;
			log::warning("NOTE: The contract has not been instantiated.")?;
		}
		return Ok(());
	}
}

impl From<UpContractCommand> for UpOpts {
	fn from(cmd: UpContractCommand) -> Self {
		return UpOpts {
			path: cmd.path,
			constructor: cmd.constructor,
			args: cmd.args,
			value: cmd.value,
			gas_limit: cmd.gas_limit,
			proof_size: cmd.proof_size,
			salt: cmd.salt,
			url: cmd.url,
			suri: cmd.suri,
		};
	}
}

#[cfg(test)]
mod tests {
	use url::Url;

	use super::*;

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
}
