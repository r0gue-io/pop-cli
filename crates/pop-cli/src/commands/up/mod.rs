// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::builds::ensure_project_path,
};
use clap::{Args, Subcommand};
#[cfg(feature = "chain")]
use pop_chains::up::Relay;
use serde::Serialize;
#[cfg(any(feature = "chain", feature = "contract"))]
use std::fmt::{Display, Formatter, Result};
use std::path::PathBuf;

#[cfg(feature = "chain")]
mod chain;
#[cfg(feature = "contract")]
mod contract;
/// Utilities for launching a frontend dev server.
mod frontend;
#[cfg(feature = "chain")]
pub(super) mod network;

#[cfg(feature = "chain")]
const KUSAMA: u8 = Relay::Kusama as u8;
#[cfg(feature = "chain")]
const PASEO: u8 = Relay::Paseo as u8;
#[cfg(feature = "chain")]
const POLKADOT: u8 = Relay::Polkadot as u8;
#[cfg(feature = "chain")]
const WESTEND: u8 = Relay::Westend as u8;

/// Arguments for launching or deploying a project.
#[derive(Args, Clone, Serialize)]
#[cfg_attr(test, derive(Default))]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	/// Path to the project directory.
	#[serde(skip_serializing)]
	#[arg(short, long)]
	pub path: Option<PathBuf>,

	/// Directory path without flag for your project [default: current directory]
	#[serde(skip_serializing)]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub path_pos: Option<PathBuf>,

	#[command(flatten)]
	#[cfg(feature = "chain")]
	pub(crate) chain: chain::UpCommand,

	#[command(flatten)]
	#[cfg(feature = "contract")]
	pub(crate) contract: contract::UpContractCommand,

	#[command(subcommand)]
	pub(crate) command: Option<Command>,
}

/// Launch a local network or deploy a smart contract.
#[derive(Subcommand, Clone, Serialize)]
pub(crate) enum Command {
	/// Launch a local network by specifying a network configuration file.
	#[cfg(feature = "chain")]
	#[clap(aliases = ["n", "chain"])]
	Network(network::ConfigFileCommand),
	/// Launch a local Paseo network.
	#[cfg(feature = "chain")]
	#[clap()]
	Paseo(network::BuildCommand<PASEO>),
	/// Launch a local Kusama network.
	#[cfg(feature = "chain")]
	#[clap()]
	Kusama(network::BuildCommand<KUSAMA>),
	/// Launch a local Polkadot network.
	#[cfg(feature = "chain")]
	#[clap()]
	Polkadot(network::BuildCommand<POLKADOT>),
	/// Launch a local Westend network.
	#[cfg(feature = "chain")]
	#[clap()]
	Westend(network::BuildCommand<WESTEND>),
	/// Launch a local Polkadot network with Asset Hub.
	#[cfg(feature = "chain")]
	#[clap(name = "asset-hub")]
	AssetHub(network::BuildCommand<POLKADOT>),
	/// Launch a frontend dev server.
	#[clap(alias = "f")]
	Frontend(frontend::FrontendCommand),
	#[cfg(feature = "contract")]
	/// Launch a local Ink! node.
	#[clap()]
	InkNode(contract::InkNodeCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: &mut UpArgs) -> anyhow::Result<()> {
		Self::execute_project_deployment(args, &mut Cli).await
	}

	/// Identifies the project type and executes the appropriate deployment process.
	async fn execute_project_deployment(
		args: &mut UpArgs,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
		let project_path = ensure_project_path(args.path.clone(), args.path_pos.clone());
		#[cfg(feature = "chain")]
		if project_path.is_file() {
			let cmd =
				network::ConfigFileCommand { path: project_path.clone(), ..Default::default() };
			cmd.execute(cli).await?;
			return Ok(());
		}

		// If only contract feature enabled, deploy a contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(&project_path)? {
			args.contract.path = project_path.clone();
			args.contract.execute().await?;
			return Ok(());
		}
		#[cfg(feature = "chain")]
		if pop_chains::is_supported(&project_path) {
			args.chain.path = project_path.clone();
			args.chain.execute(cli).await?;
			return Ok(());
		}
		cli.warning("No contract or chain detected. Ensure you are in a valid project directory.")?;
		Ok(())
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "chain")]
			Command::Network(_) => write!(f, "network"),
			#[cfg(feature = "chain")]
			Command::Paseo(_) => write!(f, "paseo"),
			#[cfg(feature = "chain")]
			Command::Kusama(_) => write!(f, "kusama"),
			#[cfg(feature = "chain")]
			Command::Polkadot(_) => write!(f, "polkadot"),
			#[cfg(feature = "chain")]
			Command::Westend(_) => write!(f, "westend"),
			#[cfg(feature = "chain")]
			Command::AssetHub(_) => write!(f, "asset-hub"),
			Command::Frontend(_) => write!(f, "frontend"),
			#[cfg(feature = "contract")]
			Command::InkNode(_) => write!(f, "ink-node"),
		}
	}
}

#[cfg(test)]
mod tests {
	#[cfg(feature = "contract")]
	use super::contract::UpContractCommand;
	use super::*;
	use cli::MockCli;
	use duct::cmd;
	#[cfg(feature = "chain")]
	use {
		crate::style::format_url,
		pop_chains::{ChainTemplate, Config, DeploymentProvider, instantiate_template_dir},
		strum::VariantArray,
		url::Url,
	};

	fn create_up_args(project_path: PathBuf) -> anyhow::Result<UpArgs> {
		Ok(UpArgs {
			path: Some(project_path.clone()),
			path_pos: None,
			#[cfg(feature = "contract")]
			contract: UpContractCommand {
				path: project_path,
				constructor: "new".to_string(),
				args: vec!["false".to_string()],
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				url: None,
				suri: Some("//Alice".to_string()),
				use_wallet: false,
				execute: false,
				upload_only: true,
				skip_confirm: false,
				skip_build: true,
			},
			#[cfg(feature = "chain")]
			chain: chain::UpCommand::default(),
			command: None,
		})
	}

	#[tokio::test]
	#[cfg(feature = "chain")]
	async fn detects_chain_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "chain";
		let project_path = temp_dir.path().join(name);
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_template_dir(&ChainTemplate::Standard, &project_path, None, config)?;

		let mut args = create_up_args(project_path)?;
		args.chain.id = Some(2000);
		args.chain.relay_chain_url = Some(Url::parse("ws://127.0.0.1:9944")?);
		args.chain.genesis_code = Some(PathBuf::from("path/to/genesis"));
		args.chain.genesis_state = Some(PathBuf::from("path/to/state"));
		let mut cli = MockCli::new().expect_select(
			"Select your deployment method:",
			Some(false),
			true,
			Some(
				DeploymentProvider::VARIANTS
					.iter()
					.map(|action| (action.name().to_string(), format_url(action.base_url())))
					.chain(std::iter::once((
						"Register".to_string(),
						"Register the chain on the relay chain without deploying with a provider"
							.to_string(),
					)))
					.collect::<Vec<_>>(),
			),
			DeploymentProvider::VARIANTS.len(), // Register
			None,
		);
		Command::execute_project_deployment(&mut args, &mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn detects_rust_project_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "hello_world";
		let path = temp_dir.path();
		let project_path = path.join(name);
		let mut args = create_up_args(project_path)?;

		cmd("cargo", ["new", name, "--bin"]).dir(path).run()?;
		let mut cli = MockCli::new().expect_warning(
			"No contract or chain detected. Ensure you are in a valid project directory.",
		);
		Command::execute_project_deployment(&mut args, &mut cli).await?;
		cli.verify()
	}

	#[test]
	#[allow(deprecated)]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::Network(Default::default()).to_string(), "network");
	}
}
