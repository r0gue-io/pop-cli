// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::{
		builds::get_project_path,
		Project::{self, *},
	},
};
use clap::{Args, Subcommand};
use std::{
	fmt::{Display, Formatter, Result},
	path::PathBuf,
};

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
mod contract;
#[cfg(feature = "parachain")]
mod network;
#[cfg(feature = "parachain")]
mod rollup;

/// Arguments for launching or deploying a project.
#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	/// Path to the project directory.
	// TODO: Introduce the short option in v0.8.0 once deprecated parachain command is removed.
	#[arg(long, global = true)]
	pub path: Option<PathBuf>,

	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, global = true, conflicts_with = "path")]
	pub path_pos: Option<PathBuf>,

	#[command(flatten)]
	#[cfg(feature = "parachain")]
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
	#[cfg(feature = "parachain")]
	/// Launch a local network.
	#[clap(alias = "n")]
	Network(network::ZombienetCommand),
	#[cfg(feature = "parachain")]
	/// [DEPRECATED] Launch a local network (will be removed in v0.8.0).
	#[clap(alias = "p", hide = true)]
	#[deprecated(since = "0.7.0", note = "will be removed in v0.8.0")]
	#[allow(rustdoc::broken_intra_doc_links)]
	Parachain(network::ZombienetCommand),
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	/// [DEPRECATED] Deploy a smart contract (will be removed in v0.8.0).
	#[clap(alias = "c", hide = true)]
	#[deprecated(since = "0.7.0", note = "will be removed in v0.8.0")]
	#[allow(rustdoc::broken_intra_doc_links)]
	Contract(contract::UpContractCommand),
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
		// If only contract feature enabled, deploy a contract
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			cmd.valid = true; // To handle deprecated command, remove in v0.8.0.
			cmd.execute().await?;
			return Ok(Contract);
		}
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(project_path.as_deref())? {
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

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "parachain")]
			Command::Network(_) => write!(f, "network"),
			#[cfg(feature = "parachain")]
			#[allow(deprecated)]
			Command::Parachain(_) => write!(f, "chain"),
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			#[allow(deprecated)]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;
	use duct::cmd;
	use url::Url;
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	use {
		super::contract::UpContractCommand,
		pop_contracts::{mock_build_process, new_environment},
		std::env,
	};
	#[cfg(feature = "parachain")]
	use {
		crate::style::format_url,
		pop_parachains::{instantiate_template_dir, Config, DeploymentProvider, Parachain},
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
				url: Url::parse("wss://rpc2.paseo.popnetwork.xyz")?,
				suri: "//Alice".to_string(),
				use_wallet: false,
				dry_run: true,
				upload_only: true,
				skip_confirm: false,
				valid: false,
			},
			#[cfg(feature = "parachain")]
			rollup: rollup::UpCommand::default(),
			command: None,
		})
	}

	#[tokio::test]
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	async fn detects_contract_correctly() -> anyhow::Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;
		let args = create_up_args(temp_dir.path().join("testing"))?;
		let mut cli = MockCli::new();
		assert_eq!(Command::execute_project_deployment(args, &mut cli).await?, Project::Contract);
		cli.verify()
	}

	#[tokio::test]
	#[cfg(feature = "parachain")]
	async fn detects_rollup_correctly() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let name = "rollup";
		let project_path = temp_dir.path().join(name);
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_template_dir(&Parachain::Standard, &project_path, None, config)?;

		let mut args = create_up_args(project_path)?;
		args.rollup.relay_chain_url = Some(Url::parse("wss://polkadot-rpc.publicnode.com")?);
		args.rollup.id = Some(2000);
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
		#[cfg(feature = "parachain")]
		assert_eq!(Command::Network(Default::default()).to_string(), "network");
		#[cfg(feature = "parachain")]
		assert_eq!(Command::Parachain(Default::default()).to_string(), "chain");
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}
}
