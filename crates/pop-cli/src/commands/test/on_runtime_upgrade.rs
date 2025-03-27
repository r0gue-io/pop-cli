use crate::{
	cli::{
		self,
		traits::{Input, Select},
	},
	commands::test::TestTryRuntimeCommand,
	common::{
		builds::guide_user_to_select_profile, prompt::display_message,
		runtime::ensure_runtime_binary_exists, try_runtime::check_try_runtime_and_prompt,
	},
};
use clap::Args;
use frame_try_runtime::UpgradeCheckSelect;
use pop_common::Profile;
use pop_parachains::{
	generate_try_runtime, get_upgrade_checks_details, OnRuntimeUpgradeSubcommand,
	TryRuntimeCliCommand,
};
use std::{env::current_dir, path::PathBuf, str::FromStr};
use strum::{EnumMessage, VariantArray};
use try_runtime_core::common::{
	shared_parameters::Runtime,
	state::{LiveState, State},
};

const DEFAULT_BLOCK_TIME: &str = "6000";
const DEFAULT_BLOCK_HASH: &str = "0x1a2b3c4d5e6f7890";
const DEFAULT_LIVE_NODE_URL: &str = "ws://localhost:9944/";
const EXCLUDED_ARGS: [&str; 7] =
	["--profile", "--migration", "-m", "--no-build", "-n", "--skip-confirm", "-y"];

/// Configuration for [`run`].
#[derive(Debug, Clone, clap::Parser)]
pub struct Command {
	/// The state type to use.
	#[command(subcommand)]
	pub state: Option<State>,

	/// Select which optional checks to perform. Selects all when no value is given.
	///
	/// - `none`: Perform no checks.
	/// - `all`: Perform all checks (default when --checks is present with no value).
	/// - `pre-and-post`: Perform pre- and post-upgrade checks (default when the arg is not
	///   present).
	/// - `try-state`: Perform the try-state checks.
	///
	/// Performing any checks will potentially invalidate the measured PoV/Weight.
	// NOTE: The clap attributes make it backwards compatible with the previous `--checks` flag.
	#[clap(long,
			default_value = "pre-and-post",
			default_missing_value = "all",
			num_args = 0..=1,
			verbatim_doc_comment
    )]
	pub checks: UpgradeCheckSelect,

	/// Whether to disable weight warnings, useful if the runtime is for a relay chain.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	pub no_weight_warnings: bool,

	/// Whether to skip enforcing that the new runtime `spec_version` is greater or equal to the
	/// existing `spec_version`.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	pub disable_spec_version_check: bool,

	/// Whether to disable migration idempotency checks
	#[clap(long, default_value = "false", default_missing_value = "true")]
	pub disable_idempotency_checks: bool,

	/// When migrations are detected as not idempotent, enabling this will output a diff of the
	/// storage before and after running the same set of migrations the second time.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	pub print_storage_diff: bool,

	/// Whether or multi-block migrations should be executed to completion after single block
	/// migratons are completed.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	pub disable_mbm_checks: bool,

	/// The maximum duration we expect all MBMs combined to take.
	///
	/// This value is just here to ensure that the CLI won't run forever in case of a buggy MBM.
	#[clap(long, default_value = "600")]
	pub mbm_max_blocks: u32,

	/// The chain blocktime in milliseconds.
	#[arg(long)]
	pub blocktime: Option<u64>,
}

#[derive(Args)]
pub(crate) struct TestOnRuntimeUpgradeCommand {
	/// Command to test runtime migrations.
	#[clap(flatten)]
	command: TestTryRuntimeCommand<Command>,
	/// Build profile.
	#[clap(long, value_enum)]
	profile: Option<Profile>,
	/// Avoid rebuilding the runtime if there is an existing runtime binary.
	#[clap(short = 'n', long)]
	no_build: bool,
	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl TestOnRuntimeUpgradeCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Testing runtime migrations")?;
		if self.profile.is_none() {
			match guide_user_to_select_profile(cli) {
				Ok(profile) => self.profile = Some(profile),
				Err(e) => return display_message(&e.to_string(), false, cli),
			}
		};
		self.command.shared_params.runtime = Runtime::Path(ensure_runtime_binary_exists(
			cli,
			&current_dir().unwrap_or(PathBuf::from("./")),
			self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
			!self.no_build,
		)?);

		if let Err(e) = self.update_state(cli) {
			return display_message(&e.to_string(), false, cli);
		}

		// If the `checks` argument is not provided, prompt the user to select the upgrade checks.
		if !has_argument("checks") {
			match guide_user_to_select_upgrade_checks(cli) {
				Ok(checks) => self.command.command.checks = checks,
				Err(e) => return display_message(&e.to_string(), false, cli),
			}
		}

		let result = self.run(cli).await;

		// Display the `on-runtime-upgrade` command.
		cli.info(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Tested runtime migrations successfully!", true, cli)?;
		Ok(())
	}

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		generate_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::OnRuntimeUpgrade,
			|args| args,
			&EXCLUDED_ARGS,
		)?;
		Ok(())
	}

	fn update_state(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let mut subcommand: Option<OnRuntimeUpgradeSubcommand> = None;
		let mut path: Option<PathBuf> = None;
		let mut live_state = default_live_state();

		// Read from state subcommand.
		match self.command.command.state {
			Some(ref state) => match state {
				State::Live(_state) => {
					live_state = _state.clone();
					subcommand = Some(OnRuntimeUpgradeSubcommand::Live);
				},
				State::Snap { path: _path } => {
					path = _path.clone();
					subcommand = Some(OnRuntimeUpgradeSubcommand::Snapshot);
				},
			},
			None => {},
		}

		// If there is no state, prompt the user to select one.
		if subcommand.is_none() {
			subcommand = Some(guide_user_to_select_chain_state(cli)?.clone());
		};
		match subcommand {
			Some(state) => {
				match state {
					OnRuntimeUpgradeSubcommand::Live => self.update_live_state(cli, live_state)?,
					OnRuntimeUpgradeSubcommand::Snapshot =>
						self.update_snapshot_state(cli, path)?,
				}
				return Ok(());
			},
			None => return Err(anyhow::anyhow!("No chain state selected for migration.")),
		}
	}

	fn update_snapshot_state(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		mut path: Option<PathBuf>,
	) -> anyhow::Result<()> {
		if path.is_none() {
			let snapshot_file: PathBuf = cli
				.input(format!(
					"Enter path to your snapshot file?\n{}.",
					console::style(
						"Snapshot file can be generated using `pop test create-snapshot` command."
					)
					.dim()
				))
				.required(true)
				.interact()?
				.into();
			if !snapshot_file.is_file() {
				return Err(anyhow::anyhow!("Invalid path to the snapshot file."));
			}
			path = Some(snapshot_file);
		}
		if self.command.command.blocktime.is_none() {
			let block_time = cli
				.input("Enter the block time:")
				.required(true)
				.default_input(DEFAULT_BLOCK_TIME)
				.interact()?;
			self.command.command.blocktime = Some(block_time.parse()?);
		}
		self.command.command.state = Some(State::Snap { path });
		Ok(())
	}

	fn update_live_state(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		mut live_state: LiveState,
	) -> anyhow::Result<()> {
		if live_state.uri.is_empty() {
			live_state.uri = cli
				.input("Enter the live chain of your node:")
				.required(true)
				.placeholder(DEFAULT_LIVE_NODE_URL)
				.interact()?;
		}
		if live_state.at.is_none() {
			let block_hash = cli
				.input("Enter the block hash (optional):")
				.required(false)
				.placeholder(DEFAULT_BLOCK_HASH)
				.interact()?;
			live_state.at = Some(check_block_hash(&block_hash)?);
		}
		self.command.command.state = Some(State::Live(live_state.clone()));
		Ok(())
	}

	fn display(&self) -> String {
		let mut args = vec!["pop test on-runtime-upgrade".to_string()];
		let mut arguments: Vec<String> = std::env::args().skip(3).collect();

		if !has_argument("runtime") {
			arguments.push(format!(
				"--runtime={}",
				match self.command.shared_params.runtime {
					Runtime::Path(ref path) => path.to_str().unwrap().to_string(),
					Runtime::Existing => "existing".to_string(),
				}
			));
		}
		if let Some(ref profile) = self.profile {
			arguments.push(format!("--profile={}", profile));
		}
		if !has_argument("checks") {
			let (value, _) = get_upgrade_checks_details(self.command.command.checks);
			arguments.push(format!("--checks={}", value));
		}

		if self.no_build {
			arguments.push("-n".to_string());
		}
		if self.skip_confirm {
			arguments.push("-y".to_string());
		}
		args.extend(arguments);
		args.join(" ")
	}
}

fn check_block_hash(block_hash: &str) -> anyhow::Result<String> {
	let (block_hash, offset) = if let Some(block_hash) = block_hash.strip_prefix("0x") {
		(block_hash, 2)
	} else {
		(block_hash, 0)
	};

	if let Some(pos) = block_hash.chars().position(|c| !c.is_ascii_hexdigit()) {
		Err(anyhow::anyhow!(
			"Expected block hash, found illegal hex character at position: {}",
			offset + pos,
		))
	} else {
		Ok(block_hash.into())
	}
}

fn has_argument(arg: &str) -> bool {
	let args: Vec<String> = std::env::args().collect();
	args.iter().any(|a| a.contains(arg))
}

fn guide_user_to_select_chain_state(
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<&OnRuntimeUpgradeSubcommand> {
	let mut prompt = cli.select("Run the migrations:");
	for subcommand in OnRuntimeUpgradeSubcommand::VARIANTS.iter() {
		prompt = prompt.item(
			subcommand,
			subcommand.get_message().unwrap(),
			subcommand.get_detailed_message().unwrap(),
		);
	}
	prompt.interact().map_err(anyhow::Error::from)
}

fn guide_user_to_select_upgrade_checks(
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<UpgradeCheckSelect> {
	let default_upgrade_check = get_upgrade_checks_details(UpgradeCheckSelect::All);
	let mut prompt = cli
		.select("Select which optional checks to perform.")
		.initial_value(default_upgrade_check.0);
	for check in [
		UpgradeCheckSelect::None,
		UpgradeCheckSelect::All,
		UpgradeCheckSelect::TryState,
		UpgradeCheckSelect::PreAndPost,
	] {
		let (value, description) = get_upgrade_checks_details(check);
		prompt = prompt.item(value.clone(), value, description);
	}
	let input = prompt.interact()?;
	UpgradeCheckSelect::from_str(&input).map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn default_live_state() -> LiveState {
	LiveState {
		uri: String::default(),
		at: None,
		pallet: vec![],
		hashed_prefixes: vec![],
		child_tree: false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn check_block_hash_works() {
		assert!(check_block_hash("0x1234567890abcdef").is_ok());
		assert!(check_block_hash("1234567890abcdef").is_ok());
		assert!(check_block_hash("0x1234567890abcdefg").is_err());
		assert!(check_block_hash("1234567890abcdefg").is_err());
	}
}
