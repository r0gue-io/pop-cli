// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::{
		builds::get_project_path,
		Project::{self, *},
	},
};
use clap::{Args, Subcommand};
use std::path::PathBuf;
#[cfg(feature = "chain")]
use {
	pop_chains::up::Relay,
	std::fmt::{Display, Formatter, Result},
};

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
mod contract;
#[cfg(feature = "chain")]
pub(super) mod network;
#[cfg(feature = "chain")]
mod rollup;

#[cfg(feature = "chain")]
const KUSAMA: u8 = Relay::Kusama as u8;
#[cfg(feature = "chain")]
const PASEO: u8 = Relay::Paseo as u8;
#[cfg(feature = "chain")]
const POLKADOT: u8 = Relay::Polkadot as u8;
#[cfg(feature = "chain")]
const WESTEND: u8 = Relay::Westend as u8;

/// Arguments for launching or deploying a project.
#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	/// Path to the project directory.
	#[arg(long)]
	pub path: Option<PathBuf>,

	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub path_pos: Option<PathBuf>,

	#[command(flatten)]
	#[cfg(feature = "chain")]
	pub(crate) rollup: rollup::UpCommand,

	#[command(flatten)]
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	pub(crate) contract: contract::UpContractCommand,

	#[command(subcommand)]
	pub(crate) command: Option<Command>,
}

/// Launch a local network or deploy a smart contract.
#[derive(Subcommand, Clone)]
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
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: UpArgs) -> anyhow::Result<Project> {
		Self::execute_project_deployment(args, &mut Cli).await
	}

	/// Identifies the project type and executes the appropriate deployment process.
	async fn execute_project_deployment(
		args: UpArgs,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<Project> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());
		#[cfg(feature = "chain")]
		if let Some(path) = project_path.as_deref() {
			if path.is_file() {
				let cmd =
					network::ConfigFileCommand { path: Some(path.into()), ..Default::default() };
				cmd.execute(cli).await?;
				return Ok(Network);
			}
		}
		// If only contract feature enabled, deploy a contract
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			cmd.execute().await?;
			return Ok(Contract);
		}
		#[cfg(feature = "chain")]
		if pop_chains::is_supported(project_path.as_deref())? {
			let mut cmd = args.rollup;
			cmd.path = project_path;
			cmd.execute(cli).await?;
			return Ok(Chain);
		}
		cli.warning(
			"No contract or rollup detected. Ensure you are in a valid project directory.",
		)?;
		Ok(Unknown)
	}
}

#[cfg(feature = "chain")]
impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			Command::Network(_) => write!(f, "network"),
			Command::Paseo(_) => write!(f, "paseo"),
			Command::Kusama(_) => write!(f, "kusama"),
			Command::Polkadot(_) => write!(f, "polkadot"),
			Command::Westend(_) => write!(f, "westend"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::common::urls;
	use cli::MockCli;
	use duct::cmd;
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	use {super::contract::UpContractCommand, url::Url};
	#[cfg(feature = "chain")]
	use {
		crate::style::format_url,
		pop_chains::{instantiate_template_dir, ChainTemplate, Config, DeploymentProvider},
		strum::VariantArray,
	};

	fn create_up_args(project_path: PathBuf) -> anyhow::Result<UpArgs> {
		Ok(UpArgs {
			path: Some(project_path),
			path_pos: None,
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			contract: UpContractCommand {
				path: None,
				constructor: "new".to_string(),
				args: vec!["false".to_string()],
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				salt: None,
				url: Url::parse(urls::LOCAL)?,
				suri: "//Alice".to_string(),
				use_wallet: false,
				dry_run: true,
				upload_only: true,
				skip_confirm: false,
			},
			#[cfg(feature = "chain")]
			rollup: rollup::UpCommand::default(),
			command: None,
		})
	}

	#[tokio::test]
	#[cfg(feature = "chain")]
	async fn detects_rollup_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "rollup";
		let project_path = temp_dir.path().join(name);
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_template_dir(&ChainTemplate::Standard, &project_path, None, config)?;

		let mut args = create_up_args(project_path)?;
		args.rollup.id = Some(2000);
		args.rollup.relay_chain_url = Some(Url::parse("ws://127.0.0.1:9944")?);
		args.rollup.genesis_code = Some(PathBuf::from("path/to/genesis"));
		args.rollup.genesis_state = Some(PathBuf::from("path/to/state"));
		let mut cli = MockCli::new().expect_select(
			"Select your deployment method:",
			Some(false),
			true,
			Some(
				DeploymentProvider::VARIANTS
					.into_iter()
					.map(|action| (action.name().to_string(), format_url(action.base_url())))
					.chain(std::iter::once((
						"Register".to_string(),
						"Register the rollup on the relay chain without deploying with a provider"
							.to_string(),
					)))
					.collect::<Vec<_>>(),
			),
			DeploymentProvider::VARIANTS.len(), // Register
			None,
		);
		assert_eq!(Command::execute_project_deployment(args, &mut cli).await?, Chain);
		cli.verify()
	}

	#[tokio::test]
	async fn detects_rust_project_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "hello_world";
		let path = temp_dir.path();
		let project_path = path.join(name);
		let args = create_up_args(project_path)?;

		cmd("cargo", ["new", name, "--bin"]).dir(&path).run()?;
		let mut cli = MockCli::new().expect_warning(
			"No contract or rollup detected. Ensure you are in a valid project directory.",
		);
		assert_eq!(Command::execute_project_deployment(args, &mut cli).await?, Unknown);
		cli.verify()
	}

	#[test]
	#[allow(deprecated)]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::Network(Default::default()).to_string(), "network");
	}
}
