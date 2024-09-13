// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{fetch_metadata, parse_chain_metadata, query, storage_info, Metadata};

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The name of the pallet to call.
	#[clap(long, short)]
	pallet: Option<String>,
	/// The name of extrinsic to call.
	#[clap(long, short)]
	extrinsic: Option<String>,
	/// The constructor arguments, encoded as strings.
	#[clap(long, num_args = 0..)]
	args: Vec<String>,
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
	fn display(&self) -> String {
		let mut full_message = format!("pop call parachain");
		if let Some(pallet) = &self.pallet {
			full_message.push_str(&format!(" --pallet {}", pallet));
		}
		if let Some(extrinsic) = &self.extrinsic {
			full_message.push_str(&format!(" --extrinsic {}", extrinsic));
		}
		if !self.args.is_empty() {
			full_message.push_str(&format!(" --args {}", self.args.join(" ")));
		}
		full_message.push_str(&format!(" --url {} --suri {}", self.url, self.suri));
		full_message
	}
}

pub(crate) struct CallParachain<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// The args to call.
	pub(crate) args: CallParachainCommand,
}

impl<'a, CLI: Cli> CallParachain<'a, CLI> {
	/// Executes the command.
	pub(crate) async fn execute(mut self: Self) -> Result<()> {
		self.cli.intro("Call a parachain")?;
		let metadata = fetch_metadata("wss://rpc1.paseo.popnetwork.xyz").await?;
		let call_config = if self.args.pallet.is_none() && self.args.extrinsic.is_none() {
			guide_user_to_call_chain(&mut self, metadata).await?
		} else {
			self.args.clone()
		};
		match self.execute_extrinsic(call_config.clone()).await {
			Ok(_) => Ok(()),
			Err(e) => {
				self.cli.outro_cancel(format!("{}", e.to_string()))?;
				return Ok(());
			},
		}
	}
	/// Executes the extrinsic or query.
	async fn execute_extrinsic(&mut self, call_config: CallParachainCommand) -> Result<()> {
		let pallet = call_config
			.pallet
			.clone()
			.expect("pallet can not be none as fallback above is interactive input; qed");
		let extrinsic = call_config
			.extrinsic
			.clone()
			.expect("extrinsic can not be none as fallback above is interactive input; qed");
		// TODO: Check if exists?
		// println!("Extrinsic to submit: {:?} with args {:?}", extrinsic.name, args);
		// query(&pallet.label, &storage.name, vec![], "wss://rpc1.paseo.popnetwork.xyz").await?;
		Ok(())
	}
}

#[derive(Clone, Eq, PartialEq)]
enum Action {
	Extrinsic,
	Query,
}

/// Guide the user to call the contract.
async fn guide_user_to_call_chain<'a, CLI: Cli>(
	command: &mut CallParachain<'a, CLI>,
	metadata: Metadata,
) -> anyhow::Result<CallParachainCommand> {
	command.cli.intro("Call a contract")?;
	// Prompt for contract location.
	let url: String = command
		.cli
		.input("Which chain would you like to interact with?")
		.placeholder("wss://rpc1.paseo.popnetwork.xyz")
		.default_input("wss://rpc1.paseo.popnetwork.xyz")
		.interact()?;

	let pallets = match parse_chain_metadata(metadata.clone()).await {
		Ok(pallets) => pallets,
		Err(e) => {
			command.cli.outro_cancel("Unable to fetch the chain metadata.")?;
			return Err(anyhow!(format!("{}", e.to_string())));
		},
	};
	let pallet = {
		let mut prompt = command.cli.select("Select the pallet to call:");
		for pallet_item in pallets {
			prompt = prompt.item(pallet_item.clone(), &pallet_item.label, &pallet_item.docs);
		}
		prompt.interact()?
	};
	let action = command
		.cli
		.select("What do you want to do?")
		.item(Action::Extrinsic, "Submit an extrinsic", "hint")
		.item(Action::Query, "Query storage", "hint")
		.interact()?;

	let mut extrinsic;
	let mut args = Vec::new();
	if action == Action::Extrinsic {
		extrinsic = {
			let mut prompt_extrinsic = command.cli.select("Select the extrinsic to call:");
			for extrinsic in pallet.extrinsics {
				prompt_extrinsic = prompt_extrinsic.item(
					extrinsic.clone(),
					&extrinsic.name,
					&extrinsic.docs.concat(),
				);
			}
			prompt_extrinsic.interact()?
		};
		for argument in extrinsic.fields {
			let value = command
				.cli
				.input(&format!(
					"Enter the value for the argument '{}':",
					argument.name.unwrap_or_default()
				))
				.interact()?;
			args.push(value);
		}
	} else {
		let storage = {
			let mut prompt_storage = command.cli.select("Select the storage to query:");
			for storage in pallet.storage {
				prompt_storage = prompt_storage.item(storage.clone(), &storage.name, &storage.docs);
			}
			prompt_storage.interact()?
		};
		let a = storage_info(&pallet.label, &storage.name, &metadata)?;
		println!("STORAGE {:?}", a);
	}

	Ok(CallParachainCommand {
		pallet: Some(pallet.label),
		extrinsic: Some("extrinsic".to_string()),
		args: vec!["".to_string()],
		url: "wss://rpc2.paseo.popnetwork.xyz".to_string(),
		suri: "//Alice".to_string(),
	})
}
