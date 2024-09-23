// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::Result;
use clap::Args;
use pop_parachains::{
	prepare_extrinsic, set_up_api, submit_extrinsic, OnlineClient, Pallet, SubstrateConfig,
};
use strum::VariantArray;

use super::use_cases::prompt_arguments;

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The signed extrinsic to call.
	#[clap(long, short)]
	extrinsic: Option<String>,
	/// Websocket endpoint of a node.
	#[clap(name = "url", long, value_parser, default_value = "ws://localhost:9944")]
	url: String,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short, default_value = "//Alice")]
	suri: String,
}

impl CallParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let (api, url) = self.set_up_api(&mut cli::Cli).await?;
		let call_config = if self.extrinsic.is_none() {
			guide_user_to_call_chain(&api, url, &mut cli::Cli).await?
		} else {
			self.clone()
		};
		execute_extrinsic(api, call_config, self.extrinsic.is_none(), &mut cli::Cli).await?;
		Ok(())
	}
	/// Prompt the user for the chain to use if not indicated and fetch the metadata.
	async fn set_up_api(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<(OnlineClient<SubstrateConfig>, String)> {
		cli.intro("Call a parachain")?;
		let url: String = if self.extrinsic.is_none() {
			// Prompt for contract location.
			cli.input("Which chain would you like to interact with?")
				.placeholder("wss://rpc1.paseo.popnetwork.xyz")
				.default_input("wss://rpc1.paseo.popnetwork.xyz")
				.interact()?
		} else {
			self.url.clone()
		};
		let api = set_up_api(&url).await?;
		Ok((api, url))
	}

	fn display(&self) -> String {
		let mut full_message = "pop call parachain".to_string();
		if let Some(extrinsic) = &self.extrinsic {
			full_message.push_str(&format!(" --extrinsic {}", extrinsic));
		}
		full_message.push_str(&format!("--url {}", self.url));
		if !self.suri.is_empty() {
			full_message.push_str(&format!(" --suri {}", self.suri));
		}
		full_message
	}
}

/// Guide the user to call the contract.
async fn guide_user_to_call_chain(
	api: &OnlineClient<SubstrateConfig>,
	url: String,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<CallParachainCommand> {
	let pallets = Pallet::VARIANTS;
	let pallet = {
		let mut prompt = cli.select("Select the pallet to call:");
		for pallet_item in pallets {
			prompt = prompt.item(pallet_item.clone(), pallet_item.as_ref(), "");
		}
		prompt.interact()?
	};

	let extrinsic = {
		let mut prompt_extrinsic = cli.select("Select the extrinsic to call:");
		for extrinsic in pallet.extrinsics() {
			prompt_extrinsic =
				prompt_extrinsic.item(extrinsic.clone(), format!("{}\n", &extrinsic.as_ref()), "");
		}
		Ok(CallParachainCommand {
			pallet: Some(pallet.label),
			extrinsic: Some(extrinsic.name),
			query: None,
			args,
			url: "wss://rpc2.paseo.popnetwork.xyz".to_string(),
			suri: "//Alice".to_string(),
		})
	} else {
		let query = {
			let mut prompt_storage = cli.select("Select the storage to query:");
			for storage in pallet.storage {
				prompt_storage = prompt_storage.item(storage.clone(), &storage.name, &storage.docs);
			}
			prompt_storage.interact()?
		};
		let keys_needed = get_type_description(query.ty.1, metadata)?;
		for key in keys_needed {
			let value = cli.input(&format!("Enter the key '{}':", key)).interact()?;
			args.push(value);
		}
		Ok(CallParachainCommand {
			pallet: Some(pallet.label),
			extrinsic: None,
			query: Some(query.name),
			args,
			url: "wss://rpc2.paseo.popnetwork.xyz".to_string(),
			suri: "//Alice".to_string(),
		})
	}
}

/// Executes the extrinsic or query.
async fn execute_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	call_config: CallParachainCommand,
	prompt_to_repeat_call: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	cli.info(call_config.display())?;
	let extrinsic = call_config
		.extrinsic
		.expect("extrinsic can not be none as fallback above is interactive input; qed");
	if !cli.confirm("Do you want to sign and submit the call?").interact()? {
		display_message(&format!("Extrinsic: {} not submitted", extrinsic), true, cli)?;
		return Ok(());
	}
	// TODO: Handle error
	let result = submit_extrinsic(api.clone(), extrinsic).await?;
	// Repeat call.
	if prompt_to_repeat_call {
		let another_call: bool = cli
			.confirm("Do you want to do another call to the same chain?")
			.initial_value(false)
			.interact()?;
		if another_call {
			// Remove only the prompt asking for another call.
			console::Term::stderr().clear_last_lines(2)?;
			let new_call_config = guide_user_to_call_chain(&api, call_config.url, cli).await?;
			Box::pin(execute_extrinsic(api, new_call_config, prompt_to_repeat_call, cli)).await?;
		} else {
			display_message(
				&format!("Extrinsic submitted successfully with hash: {}", result),
				true,
				cli,
			)?;
		}
	} else {
		display_message(
			&format!("Extrinsic submitted successfully with hash: {}", result),
			true,
			cli,
		)?;
	}
	Ok(())
}

fn display_message(message: &str, success: bool, cli: &mut impl cli::traits::Cli) -> Result<()> {
	if success {
		cli.outro(message)?;
	} else {
		cli.outro_cancel(message)?;
	}
	Ok(())
}
