// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli},
	common::builds::get_project_path,
};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "parachain")]
mod network;
#[cfg(feature = "parachain")]
mod rollup;

/// Arguments for launching or deploying a project.
#[derive(Args, Clone)]
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
	#[cfg(feature = "contract")]
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
	Parachain(network::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// [DEPRECATED] Deploy a smart contract (will be removed in v0.8.0).
	#[clap(alias = "c", hide = true)]
	Contract(contract::UpContractCommand),
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(args: UpArgs) -> anyhow::Result<&'static str> {
		Self::execute_project_deployment(args, &mut Cli).await
	}

	/// Identifies the project type and executes the appropriate deployment process.
	async fn execute_project_deployment(
		args: UpArgs,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<&'static str> {
		let project_path = get_project_path(args.path.clone(), args.path_pos.clone());
		// If only contract feature enabled, deploy a contract
		#[cfg(feature = "contract")]
		if pop_contracts::is_supported(project_path.as_deref())? {
			let mut cmd = args.contract;
			cmd.path = project_path;
			cmd.valid = true; // To handle deprecated command, remove in v0.8.0.
			cmd.execute().await?;
			return Ok("contract");
		}
		#[cfg(feature = "parachain")]
		if pop_parachains::is_supported(project_path.as_deref())? {
			let mut cmd = args.rollup;
			cmd.path = project_path;
			cmd.execute(cli).await?;
			return Ok("parachain");
		}
		cli.warning(
			"No contract or rollup detected. Ensure you are in a valid project directory.",
		)?;
		Ok("")
	}
}

#[cfg(test)]
mod tests {
	use super::{contract::UpContractCommand, *};

	use cli::MockCli;
	use duct::cmd;
	use pop_contracts::{mock_build_process, new_environment};
	use pop_parachains::{instantiate_template_dir, Config, Parachain};
	use std::env;
	use url::Url;

	fn create_up_args(project_path: PathBuf) -> anyhow::Result<UpArgs> {
		Ok(UpArgs {
			path: Some(project_path),
			path_pos: None,
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
			rollup: rollup::UpCommand::default(),
			command: None,
		})
	}

	#[tokio::test]
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
		assert_eq!(Command::execute_project_deployment(args, &mut cli).await?, "contract");
		cli.verify()
	}

	#[tokio::test]
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
		let mut cli = MockCli::new();
		assert_eq!(Command::execute_project_deployment(args, &mut cli).await?, "parachain");
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
		assert_eq!(Command::execute_project_deployment(args, &mut cli).await?, "");
		cli.verify()
	}
}
