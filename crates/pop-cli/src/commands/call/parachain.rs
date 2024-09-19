// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{fetch_metadata, get_type_description, parse_chain_metadata, Metadata};

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The name of the pallet to call.
	#[clap(long, short)]
	pallet: Option<String>,
	/// The name of extrinsic to call.
	#[clap(long, short, conflicts_with = "query")]
	extrinsic: Option<String>,
	/// The name of storage to query.
	#[clap(long, short, conflicts_with = "extrinsic")]
	query: Option<String>,
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
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let metadata = self.query_metadata(&mut cli::Cli).await?;
		let call_config =
			if self.pallet.is_none() && (self.extrinsic.is_none() || self.query.is_none()) {
				guide_user_to_call_chain(&mut cli::Cli, metadata).await?
			} else {
				self.clone()
			};
		execute_extrinsic(call_config.clone(), &mut cli::Cli).await?;
		Ok(())
	}
	///Parse metadata.
	async fn query_metadata(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<Metadata> {
		cli.intro("Call a parachain")?;
		let url: String =
			if self.pallet.is_none() && (self.extrinsic.is_none() || self.query.is_none()) {
				// Prompt for contract location.
				cli.input("Which chain would you like to interact with?")
					.placeholder("wss://rpc1.paseo.popnetwork.xyz")
					.default_input("wss://rpc1.paseo.popnetwork.xyz")
					.interact()?
			} else {
				self.url.clone()
			};
		let metadata = fetch_metadata(&url).await?;
		Ok(metadata)
	}

	fn display(&self) -> String {
		let mut full_message = format!("pop call parachain");
		if let Some(pallet) = &self.pallet {
			full_message.push_str(&format!(" --pallet {}", pallet));
		}
		if let Some(extrinsic) = &self.extrinsic {
			full_message.push_str(&format!(" --extrinsic {}", extrinsic));
		}
		if let Some(query) = &self.query {
			full_message.push_str(&format!(" --query {}", query));
		}
		if !self.args.is_empty() {
			full_message.push_str(&format!(" --args {}", self.args.join(" ")));
		}
		full_message.push_str(&format!(" --url {} --suri {}", self.url, self.suri));
		full_message
	}
}

#[derive(Clone, Eq, PartialEq)]
enum Action {
	Extrinsic,
	Query,
}

/// Guide the user to call the contract.
async fn guide_user_to_call_chain(
	cli: &mut impl cli::traits::Cli,
	metadata: Metadata,
) -> anyhow::Result<CallParachainCommand> {
	let pallets = match parse_chain_metadata(metadata.clone()).await {
		Ok(pallets) => pallets,
		Err(e) => {
			cli.outro_cancel("Unable to fetch the chain metadata.")?;
			return Err(anyhow!(format!("{}", e.to_string())));
		},
	};
	let pallet = {
		let mut prompt = cli.select("Select the pallet to call:");
		for pallet_item in pallets {
			prompt = prompt.item(pallet_item.clone(), &pallet_item.label, &pallet_item.docs);
		}
		prompt.interact()?
	};
	let action = cli
		.select("What do you want to do?")
		.item(Action::Extrinsic, "Submit an extrinsic", "hint")
		.item(Action::Query, "Query storage", "hint")
		.interact()?;

	let mut args = Vec::new();
	if action == Action::Extrinsic {
		let extrinsic = {
			let mut prompt_extrinsic = cli.select("Select the extrinsic to call:");
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
			let value = cli
				.input(&format!(
					"Enter the value for the argument '{}':",
					argument.name.unwrap_or_default()
				))
				.interact()?;
			args.push(value);
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
		let keys_needed = get_type_description(query.ty.1, &metadata)?;
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
	call_config: CallParachainCommand,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	cli.info(call_config.display())?;
	// TODO: Check if exists?
	if call_config.extrinsic.is_some() {
		//self.execute_extrinsic(call_config.clone(), &mut cli::Cli).await?;
	} else {
		//self.execute_query(call_config.clone(), &mut cli::Cli).await?;
	}
	Ok(())
}
