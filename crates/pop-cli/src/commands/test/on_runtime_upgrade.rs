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
use std::{collections::HashSet, env::current_dir, path::PathBuf, str::FromStr};
use strum::{EnumMessage, VariantArray};
use try_runtime_core::common::{
	shared_parameters::{Runtime, SharedParams},
	state::{LiveState, State},
};

const DEFAULT_BLOCK_TIME: &str = "6000";
const DEFAULT_BLOCK_HASH: &str = "0x1a2b3c4d5e6f7890";
const DEFAULT_LIVE_NODE_URL: &str = "ws://localhost:9944/";
const CUSTOM_ARGS: [&str; 5] = ["--profile", "--no-build", "-n", "--skip-confirm", "-y"];
const SHARED_PARAMS: [&str; 6] = [
	"--runtime",
	"--wasm-execution",
	"--wasm-instantiation-strategy",
	"--heap-pages",
	"--export-proof",
	"--overwrite-state-version",
];

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
		let user_provided_args = std::env::args().collect::<Vec<String>>();
		if self.profile.is_none() {
			match guide_user_to_select_profile(cli) {
				Ok(profile) => self.profile = Some(profile),
				Err(e) => return display_message(&e.to_string(), false, cli),
			}
		};
		if !argument_exists(&user_provided_args, "--runtime") {
			self.command.shared_params.runtime = Runtime::Path(ensure_runtime_binary_exists(
				cli,
				&current_dir().unwrap_or(PathBuf::from("./")),
				self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
				!self.no_build,
			)?);
		}

		if self.command().blocktime.is_none() {
			let block_time = cli
				.input("Enter the block time:")
				.required(true)
				.default_input(DEFAULT_BLOCK_TIME)
				.interact()?;
			self.command.command.blocktime = Some(block_time.parse()?);
		}

		match self.update_state(cli) {
			Ok(subcommand) => subcommand,
			Err(e) => return display_message(&e.to_string(), false, cli),
		};

		// If the `checks` argument is not provided, prompt the user to select the upgrade checks.
		if !argument_exists(&user_provided_args, "--checks") {
			match guide_user_to_select_upgrade_checks(cli) {
				Ok(checks) => self.command.command.checks = checks,
				Err(e) => return display_message(&e.to_string(), false, cli),
			}
		}

		let result = self.run(cli).await;

		// Display the `on-runtime-upgrade` command.
		cli.info(self.display()?)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Tested runtime migrations successfully!", true, cli)?;
		Ok(())
	}

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		let mut shared_args = vec![];
		self.collect_shared_arguments(&shared_params, &mut shared_args);

		let mut args = vec![];
		self.collect_arguments_before_subcommand(&command_arguments, &mut args);
		args.push(self.subcommand()?);
		self.collect_arguments_after_subcommand(&after_subcommand, &mut args);

		generate_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::OnRuntimeUpgrade,
			shared_args,
			args,
			&CUSTOM_ARGS,
		)?;
		Ok(())
	}

	fn collect_shared_arguments(&self, user_provided_args: &[String], args: &mut Vec<String>) {
		let mut seen_args: HashSet<String> = HashSet::new();

		let arg = "--runtime";
		if !argument_exists(user_provided_args, arg) {
			let runtime_arg = match self.shared_params().runtime {
				Runtime::Path(ref path) => format!("{}={}", arg, path.to_str().unwrap()),
				Runtime::Existing => format!("{}=existing", arg),
			};
			args.push(runtime_arg.clone());
			seen_args.insert(arg.to_string());
		}

		// Exclude arguments that are already included.
		for arg in user_provided_args.iter() {
			if !seen_args.contains(arg) {
				args.push(arg.clone());
				seen_args.insert(arg.clone());
			}
		}
	}

	// Handle arguments before the subcommand.
	fn collect_arguments_before_subcommand(
		&self,
		user_provided_args: &[String],
		args: &mut Vec<String>,
	) {
		let mut seen_args: HashSet<String> = HashSet::new();

		let command = "--blocktime";
		if !argument_exists(user_provided_args, command) {
			if let Some(blocktime) = self.command().blocktime {
				args.push(format!("{}={}", command, blocktime));
				seen_args.insert(command.to_string());
			}
		}
		let arg = "--checks";
		if !argument_exists(user_provided_args, arg) {
			let (value, _) = get_upgrade_checks_details(self.command().checks);
			args.push(format!("{}={}", arg, value));
			seen_args.insert(arg.to_string());
		}
		// These are custom arguments which not in `try-runtime-cli`.
		let arg = "--profile";
		if !argument_exists(user_provided_args, arg) {
			if let Some(ref profile) = self.profile {
				args.push(format!("{}={}", arg, profile));
				seen_args.insert(arg.to_string());
			}
		}
		let arg = "-n";
		if !argument_exists(user_provided_args, arg) &&
			!argument_exists(user_provided_args, "--no-build") &&
			self.no_build
		{
			args.push(arg.to_string());
			seen_args.insert(arg.to_string());
		}
		let arg = "-y";
		if !argument_exists(user_provided_args, arg) &&
			!argument_exists(user_provided_args, "--skip-confirm") &&
			self.skip_confirm
		{
			args.push(arg.to_string());
			seen_args.insert(arg.to_string());
		}
		// Exclude arguments that are already included.
		for arg in user_provided_args.iter() {
			if !seen_args.contains(arg) {
				args.push(arg.clone());
				seen_args.insert(arg.clone());
			}
		}
	}

	// Handle arguments after the subcommand.
	fn collect_arguments_after_subcommand(
		&self,
		user_provided_args: &[String],
		args: &mut Vec<String>,
	) {
		let mut seen_args: HashSet<String> = HashSet::new();
		match self.command().state.as_ref().unwrap() {
			State::Live(state) => {
				let arg = "--uri";
				if !argument_exists(user_provided_args, arg) {
					args.push(format!("{}={}", arg, state.uri));
					seen_args.insert(arg.to_string());
				}
				let arg = "--at";
				if !argument_exists(user_provided_args, arg) {
					if let Some(ref at) = state.at {
						args.push(format!("{}={}", arg, at));
						seen_args.insert(arg.to_string());
					}
				}
			},
			State::Snap { path } => {
				let arg = "--path";
				if !argument_exists(user_provided_args, arg) {
					if let Some(ref path) = path {
						args.push(format!("{}={}", arg, path.display()));
						seen_args.insert(arg.to_string());
					};
				}
			},
		}
		// Exclude arguments that are already included.
		for arg in user_provided_args.iter() {
			if !seen_args.contains(arg) {
				args.push(arg.clone());
				seen_args.insert(arg.clone());
			}
		}
	}

	fn update_state(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let mut subcommand: Option<OnRuntimeUpgradeSubcommand> = None;
		let mut path: Option<PathBuf> = None;
		let mut live_state = default_live_state();

		// Read from state subcommand.
		if let Some(ref state) = self.command().state {
			match state {
				State::Live(_state) => {
					live_state = _state.clone();
					subcommand = Some(OnRuntimeUpgradeSubcommand::Live);
				},
				State::Snap { path: _path } => {
					path = _path.clone();
					subcommand = Some(OnRuntimeUpgradeSubcommand::Snapshot);
				},
			}
		}
		// If there is no state, prompt the user to select one.
		if subcommand.is_none() {
			subcommand = Some(guide_user_to_select_chain_state(cli)?.clone());
		};
		match subcommand {
			Some(state) => match state {
				OnRuntimeUpgradeSubcommand::Live => self.update_live_state(cli, live_state)?,
				OnRuntimeUpgradeSubcommand::Snapshot => self.update_snapshot_state(cli, path)?,
			},
			None => return Err(anyhow::anyhow!("No state selected for testing migration.")),
		}
		Ok(())
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
						"Snapshot file can be generated using `pop test create-snapshot` command"
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
			if !block_hash.is_empty() {
				live_state.at = Some(check_block_hash(&block_hash)?);
			}
		}
		self.command.command.state = Some(State::Live(live_state.clone()));
		Ok(())
	}

	fn display(&self) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test on-runtime-upgrade".to_string()];
		let mut args = vec![];
		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();

		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		self.collect_shared_arguments(&shared_params, &mut args);
		self.collect_arguments_before_subcommand(&command_arguments, &mut args);
		args.push(subcommand);
		self.collect_arguments_after_subcommand(&after_subcommand, &mut args);
		cmd_args.extend(args);
		Ok(cmd_args.join(" "))
	}

	fn command(&self) -> &Command {
		&self.command.command
	}

	fn shared_params(&self) -> &SharedParams {
		&self.command.shared_params
	}

	fn subcommand(&self) -> anyhow::Result<String> {
		Ok(match self.command().state {
			Some(State::Live(..)) => "live",
			Some(State::Snap { .. }) => "snap",
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		}
		.to_string())
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

fn partition_arguments(
	args: Vec<String>,
	subcommand: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
	let mut command_parts = args.split(|arg| arg == subcommand);
	let (before_subcommand, after_subcommand) =
		(command_parts.next().unwrap_or_default(), command_parts.next().unwrap_or_default());
	let (mut command_arguments, mut shared_params): (Vec<String>, Vec<String>) = (vec![], vec![]);
	for arg in before_subcommand.iter().cloned() {
		if is_shared_params(&arg) {
			shared_params.push(arg);
		} else {
			command_arguments.push(arg);
		}
	}
	(command_arguments, shared_params, after_subcommand.to_vec())
}

fn is_shared_params(arg: &str) -> bool {
	SHARED_PARAMS.iter().any(|a| arg.starts_with(a))
}

fn argument_exists(args: &[String], arg: &str) -> bool {
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
		.select("Select upgrade checks to perform:")
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
