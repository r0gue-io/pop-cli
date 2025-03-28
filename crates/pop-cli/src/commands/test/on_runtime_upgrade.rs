use crate::{
	cli::{
		self,
		traits::{Confirm, Input, Select},
	},
	commands::test::TestTryRuntimeCommand,
	common::{
		builds::guide_user_to_select_profile,
		prompt::display_message,
		runtime::ensure_runtime_binary_exists,
		try_runtime::{
			argument_exists, check_try_runtime_and_prompt, collect_shared_arguments,
			partition_arguments,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use frame_try_runtime::UpgradeCheckSelect;
use pop_common::Profile;
use pop_parachains::{
	generate_try_runtime, get_upgrade_checks_details, OnRuntimeUpgradeSubcommand,
	TryRuntimeCliCommand,
};
use std::{
	collections::HashSet, env::current_dir, path::PathBuf, str::FromStr, thread::sleep,
	time::Duration,
};
use strum::{EnumMessage, VariantArray};
use try_runtime_core::common::{
	shared_parameters::{Runtime, SharedParams},
	state::{LiveState, State},
};

const DEFAULT_BLOCK_TIME: &str = "6000";
const DEFAULT_BLOCK_HASH: &str =
	"0xa1b16c1efd889a9f17375ec4dd5c1b4351a2be17fa069564fced10d23b9b3836";
const DEFAULT_LIVE_NODE_URL: &str = "ws://127.0.0.1:9944";
const DEFAULT_SNAPSHOT_PATH: &str = "your-parachain.snap";
const CUSTOM_ARGS: [&str; 5] = ["--profile", "--no-build", "-n", "--skip-confirm", "-y"];

#[derive(Debug, Clone, clap::Parser)]
struct Command {
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
		if !argument_exists(&user_provided_args, "--runtime") &&
			cli.confirm(format!(
				"Do you want to run the migration on a runtime?\n{}",
				console::style(
					"If not provided, use the code of the remote node, or the snapshot."
				)
				.dim()
			))
			.initial_value(true)
			.interact()?
		{
			cli.warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")?;
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
		let spinner = spinner();
		match self.command().state {
			Some(State::Live(ref live_state)) => {
				spinner.start(format!(
					"Run the migrations of a given runtime on top of a live state at {}...",
					console::style(&live_state.uri).magenta().underlined()
				));
			},
			Some(State::Snap { ref path }) =>
				if let Some(p) = path {
					spinner.start(format!(
						"Run the migrations of a given runtime using a snapshot file at {}...",
						p.display()
					));
				},
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		}
		cli.warning("NOTE: this may take some time...")?;
		sleep(Duration::from_secs(2));

		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		let mut shared_args = vec![];
		collect_shared_arguments(self.shared_params(), &shared_params, &mut shared_args);

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
		spinner.clear();
		Ok(())
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
				if !argument_exists(user_provided_args, arg) && !state.uri.is_empty() {
					args.push(format!("{}={}", arg, state.uri));
					seen_args.insert(arg.to_string());
				}
				let arg = "--at";
				if let Some(ref at) = state.at {
					if !argument_exists(user_provided_args, arg) {
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
			if arg == "--at=" {
				continue;
			}
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
				.placeholder(DEFAULT_SNAPSHOT_PATH)
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

		collect_shared_arguments(self.shared_params(), &shared_params, &mut args);
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
			Some(State::Live(..)) => OnRuntimeUpgradeSubcommand::Live.command(),
			Some(State::Snap { .. }) => OnRuntimeUpgradeSubcommand::Snapshot.command(),
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

fn guide_user_to_select_chain_state(
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<&OnRuntimeUpgradeSubcommand> {
	let mut prompt = cli.select("Select source of runtime state to run the migration with:");
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
	use crate::common::{
		runtime::{get_mock_runtime, RuntimeFeature},
		try_runtime::source_try_runtime_binary,
	};

	use super::*;
	use clap::Parser;
	use cli::MockCli;

	#[tokio::test]
	async fn test_on_runtime_upgrade_live_state_works() -> anyhow::Result<()> {
		let command = default_command()?;
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut cli = MockCli::new()
			.expect_intro("Testing runtime migrations")
			.expect_select(
				"Choose the build profile of the binary that should be used: ".to_string(),
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_confirm(
				format!(
					"Do you want to run the migration on a runtime?\n{}",
					console::style(
						"If not provided, use the code of the remote node, or the snapshot."
					)
					.dim()
				),
				true,
			)
			.expect_warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")
			.expect_input(
				"Please specify the path to the runtime project or the runtime binary.",
				get_mock_runtime(Some(RuntimeFeature::TryRuntime)).to_str().unwrap().to_string(),
			)
			.expect_input("Enter the block time:", DEFAULT_BLOCK_TIME.to_string())
			.expect_select(
				"Select source of runtime state to run the migration with:",
				Some(true),
				true,
				Some(get_subcommands()),
				0, // live
				None,
			)
			.expect_input("Enter the live chain of your node:", DEFAULT_LIVE_NODE_URL.to_string())
			.expect_input("Enter the block hash (optional):", DEFAULT_BLOCK_HASH.to_string())
			.expect_select(
				"Select upgrade checks to perform:",
				Some(true),
				true,
				Some(get_upgrade_checks_items()),
				1, // all
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info(format!(
				"pop test on-runtime-upgrade --runtime={} --blocktime=6000 \
			--checks=all --profile=debug live --uri={} --at={}",
				get_mock_runtime(Some(RuntimeFeature::TryRuntime)).to_str().unwrap(),
				DEFAULT_LIVE_NODE_URL.to_string(),
				DEFAULT_BLOCK_HASH.strip_prefix("0x").unwrap_or_default().to_string()
			));
		command.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn test_on_runtime_upgrade_snapshot_works() -> anyhow::Result<()> {
		let command = default_command()?;
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut cli = MockCli::new()
			.expect_intro("Testing runtime migrations")
			.expect_select(
				"Choose the build profile of the binary that should be used: ".to_string(),
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_confirm(
				format!(
					"Do you want to run the migration on a runtime?\n{}",
					console::style(
						"If not provided, use the code of the remote node, or the snapshot."
					)
					.dim()
				),
				true,
			)
			.expect_warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")
			.expect_input(
				"Please specify the path to the runtime project or the runtime binary.",
				get_mock_runtime(Some(RuntimeFeature::TryRuntime)).to_str().unwrap().to_string(),
			)
			.expect_input("Enter the block time:", DEFAULT_BLOCK_TIME.to_string())
			.expect_select(
				"Select source of runtime state to run the migration with:",
				Some(true),
				true,
				Some(get_subcommands()),
				1, // snap
				None,
			)
			.expect_input(
				format!(
					"Enter path to your snapshot file?\n{}.",
					console::style(
						"Snapshot file can be generated using `pop test create-snapshot` command"
					)
					.dim()
				),
				get_mock_snapshot().to_str().unwrap().to_string(),
			)
			.expect_select(
				"Select upgrade checks to perform:",
				Some(true),
				true,
				Some(get_upgrade_checks_items()),
				1, // all
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info(format!(
				"pop test on-runtime-upgrade --runtime={} --blocktime=6000 \
				--checks=all --profile=debug snap --path={}",
				get_mock_runtime(Some(RuntimeFeature::TryRuntime)).to_str().unwrap(),
				get_mock_snapshot().to_str().unwrap()
			));
		command.execute(&mut cli).await?;
		cli.verify()
	}

	#[test]
	fn collect_arguments_before_subcommand_works() -> anyhow::Result<()> {
		let test_cases: Vec<(&str, Box<dyn Fn(&mut TestOnRuntimeUpgradeCommand)>, &str)> = vec![
			(
				"--blocktime=20",
				Box::new(|cmd: &mut TestOnRuntimeUpgradeCommand| {
					cmd.command.command.blocktime = Some(10);
				}),
				"--blocktime=10",
			),
			(
				"--checks=pre-and-post",
				Box::new(|cmd: &mut TestOnRuntimeUpgradeCommand| {
					cmd.command.command.checks = UpgradeCheckSelect::All;
				}),
				"--checks=all",
			),
			(
				"--profile=release",
				Box::new(|cmd: &mut TestOnRuntimeUpgradeCommand| {
					cmd.profile = Some(Profile::Debug);
				}),
				"--profile=debug",
			),
			(
				"--no-build",
				Box::new(|cmd: &mut TestOnRuntimeUpgradeCommand| {
					cmd.no_build = true;
				}),
				"-n",
			),
			(
				"-y",
				Box::new(|cmd: &mut TestOnRuntimeUpgradeCommand| {
					cmd.skip_confirm = true;
				}),
				"-y",
			),
			(
				"--skip-confirm",
				Box::new(|cmd: &mut TestOnRuntimeUpgradeCommand| {
					cmd.skip_confirm = true;
				}),
				"-y",
			),
		];
		for (provided_arg, update_fn, expected_arg) in test_cases {
			let mut command = default_command()?;
			let mut args = vec![];
			// Keep the user-provided argument unchanged.
			command.collect_arguments_before_subcommand(&[provided_arg.to_string()], &mut args);
			assert!(args.contains(&provided_arg.to_string()));

			// If the user does not provide an argument, modify with the argument updated during
			// runtime.
			update_fn(&mut command);
			command.collect_arguments_before_subcommand(&[], &mut args);
			assert!(args.contains(&expected_arg.to_string()));
		}
		Ok(())
	}

	#[test]
	fn collect_arguments_after_live_subcommand_works() -> anyhow::Result<()> {
		let mut command = default_command()?;
		command.command.command.state = Some(State::Live(default_live_state()));

		// No arguments.
		let mut args = vec![];
		command.collect_arguments_after_subcommand(&vec![], &mut args);
		assert!(args.is_empty());

		let mut live_state = default_live_state();
		live_state.uri = DEFAULT_LIVE_NODE_URL.to_string();
		command.command.command.state = Some(State::Live(live_state.clone()));
		// Keep the user-provided argument unchanged.
		let user_provided_args = &["--uri".to_string(), "http://localhost:9944".to_string()];
		let mut args = vec![];
		command.collect_arguments_after_subcommand(user_provided_args, &mut args);
		assert_eq!(args, user_provided_args);

		// If the user does not provide a `--uri` argument, modify with the argument updated during
		// runtime.
		let mut args = vec![];
		command.collect_arguments_after_subcommand(&vec![], &mut args);
		assert_eq!(args, vec![format!("--uri={}", live_state.uri)]);

		live_state.at = Some(DEFAULT_BLOCK_HASH.to_string());
		command.command.command.state = Some(State::Live(live_state.clone()));
		// Keep the user-provided argument unchanged.
		let user_provided_args =
			&[format!("--uri={}", live_state.uri), "--at".to_string(), "0x1234567890".to_string()];
		let mut args = vec![];
		command.collect_arguments_after_subcommand(user_provided_args, &mut args);
		assert_eq!(args, user_provided_args);

		// Not allow empty `--at`.
		let user_provided_args = &[format!("--uri={}", live_state.uri), "--at=".to_string()];
		let mut args = vec![];
		command.collect_arguments_after_subcommand(user_provided_args, &mut args);
		assert_eq!(args, vec![format!("--uri={}", live_state.uri)]);

		// If the user does not provide a block hash `--at` argument, modify with the argument
		// updated during runtime.
		let mut args = vec![];
		command.collect_arguments_after_subcommand(&vec![], &mut args);
		assert_eq!(
			args,
			vec![
				format!("--uri={}", live_state.uri),
				format!("--at={}", live_state.at.unwrap_or_default())
			]
		);
		Ok(())
	}

	#[test]
	fn collect_arguments_after_snap_subcommand_works() -> anyhow::Result<()> {
		let mut command = default_command()?;
		command.command.command.state = Some(State::Snap { path: Some(PathBuf::default()) });

		// No arguments.
		let mut args = vec![];
		command.collect_arguments_after_subcommand(&vec![], &mut args);
		assert!(args.is_empty());

		let state = State::Snap { path: Some(PathBuf::from("./existing-file")) };
		command.command.command.state = Some(state);
		// Keep the user-provided argument unchanged.
		let user_provided_args = &["--path".to_string(), "./path-to-file".to_string()];
		let mut args = vec![];
		command.collect_arguments_after_subcommand(user_provided_args, &mut args);
		assert_eq!(args, user_provided_args);

		// If the user does not provide a `--path` argument, modify with the argument updated during
		// runtime.
		let mut args = vec![];
		command.collect_arguments_after_subcommand(&vec![], &mut args);
		assert_eq!(args, vec!["--path=./existing-file"]);
		Ok(())
	}

	#[test]
	fn update_snapshot_state_works() -> anyhow::Result<()> {
		let snapshot_file = get_mock_snapshot();
		// Prompt for snapshot path if not provided
		let mut command = default_command()?;
		let mut cli = MockCli::new().expect_input(
			format!(
				"Enter path to your snapshot file?\n{}.",
				console::style(
					"Snapshot file can be generated using `pop test create-snapshot` command"
				)
				.dim()
			),
			snapshot_file.to_str().unwrap().to_string(),
		);
		command.update_snapshot_state(&mut cli, None)?;
		match command.command().state {
			Some(State::Snap { ref path }) => {
				assert_eq!(path.as_ref().unwrap(), snapshot_file.as_path());
			},
			_ => panic!("Expected snapshot state"),
		}
		cli.verify()?;

		// Use provided path without prompting.
		let mut command = default_command()?;
		let snapshot_path = Some(snapshot_file);
		let mut cli = MockCli::new(); // No prompt expected
		command.update_snapshot_state(&mut cli, snapshot_path.clone())?;
		match command.command().state {
			Some(State::Snap { ref path }) => {
				assert_eq!(path, &snapshot_path);
			},
			_ => panic!("Expected snapshot state"),
		}
		cli.verify()?;

		Ok(())
	}

	#[test]
	fn update_snapshot_state_invalid_file_fails() -> anyhow::Result<()> {
		let mut command = default_command()?;
		let mut cli = MockCli::new().expect_input(
			format!(
				"Enter path to your snapshot file?\n{}.",
				console::style(
					"Snapshot file can be generated using `pop test create-snapshot` command"
				)
				.dim()
			),
			"invalid-path-to-file".to_string(),
		);
		assert!(matches!(
			command.update_snapshot_state(&mut cli, None),
			Err(message) if message.to_string().contains("Invalid path to the snapshot file.")
		));
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn update_live_state_works() -> anyhow::Result<()> {
		// Prompt all inputs if not provided.
		let live_state = default_live_state();
		let mut command = default_command()?;
		let mut cli = MockCli::new()
			.expect_input("Enter the live chain of your node:", DEFAULT_LIVE_NODE_URL.to_string())
			.expect_input("Enter the block hash (optional):", DEFAULT_BLOCK_HASH.to_string());
		command.update_live_state(&mut cli, live_state)?;
		match command.command().state {
			Some(State::Live(ref live_state)) => {
				assert_eq!(live_state.uri, DEFAULT_LIVE_NODE_URL.to_string());
				assert_eq!(
					live_state.at,
					Some(DEFAULT_BLOCK_HASH.strip_prefix("0x").unwrap_or_default().to_string())
				);
			},
			_ => panic!("Expected live state"),
		}
		cli.verify()?;

		// Prompt for the URI if not provided.
		let mut live_state = default_live_state();
		live_state.at = Some("1234567890abcdef".to_string());
		let mut command = default_command()?;
		let mut cli = MockCli::new()
			.expect_input("Enter the live chain of your node:", DEFAULT_LIVE_NODE_URL.to_string());
		command.update_live_state(&mut cli, live_state)?;
		match command.command().state {
			Some(State::Live(ref live_state)) => {
				assert_eq!(live_state.uri, DEFAULT_LIVE_NODE_URL.to_string());
				assert_eq!(live_state.at, Some("1234567890abcdef".to_string()));
			},
			_ => panic!("Expected live state"),
		}
		cli.verify()?;

		// Prompt for the block hash if not provided.
		let mut live_state = default_live_state();
		live_state.uri = DEFAULT_LIVE_NODE_URL.to_string();
		let mut command = default_command()?;
		// Provide the empty block hash.
		let mut cli =
			MockCli::new().expect_input("Enter the block hash (optional):", String::default());
		command.update_live_state(&mut cli, live_state)?;
		match command.command().state {
			Some(State::Live(ref live_state)) => {
				assert_eq!(live_state.uri, DEFAULT_LIVE_NODE_URL.to_string());
				assert_eq!(live_state.at, None);
			},
			_ => panic!("Expected live state"),
		}
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn subcommand_works() -> anyhow::Result<()> {
		let mut command = default_command()?;
		command.command.command.state = Some(State::Live(default_live_state()));
		assert_eq!(command.subcommand()?, OnRuntimeUpgradeSubcommand::Live.command());
		command.command.command.state = Some(State::Snap { path: Some(PathBuf::default()) });
		assert_eq!(command.subcommand()?, OnRuntimeUpgradeSubcommand::Snapshot.command());
		Ok(())
	}

	#[test]
	fn check_block_hash_works() {
		assert!(check_block_hash("0x1234567890abcdef").is_ok());
		assert!(check_block_hash("1234567890abcdef").is_ok());
		assert!(check_block_hash("0x1234567890abcdefg").is_err());
		assert!(check_block_hash("1234567890abcdefg").is_err());
	}

	#[test]
	fn guide_user_to_select_chain_state_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_select(
			"Select source of runtime state to test with:",
			Some(true),
			true,
			Some(get_subcommands()),
			0,
			None,
		);
		assert_eq!(guide_user_to_select_chain_state(&mut cli)?, &OnRuntimeUpgradeSubcommand::Live);
		Ok(())
	}

	#[test]
	fn guide_user_to_select_upgrade_checks_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_select(
			"Select upgrade checks to perform:",
			Some(true),
			true,
			Some(get_upgrade_checks_items()),
			0,
			None,
		);
		assert_eq!(guide_user_to_select_upgrade_checks(&mut cli)?, UpgradeCheckSelect::None);
		cli.verify()
	}

	#[cfg(test)]
	fn default_command() -> anyhow::Result<TestOnRuntimeUpgradeCommand> {
		Ok(TestOnRuntimeUpgradeCommand {
			command: TestTryRuntimeCommand {
				command: Command::try_parse_from(vec![""])?,
				shared_params: SharedParams::try_parse_from(vec![""])?,
			},
			profile: None,
			no_build: false,
			skip_confirm: false,
		})
	}

	fn get_subcommands() -> Vec<(String, String)> {
		OnRuntimeUpgradeSubcommand::VARIANTS
			.iter()
			.map(|subcommand| {
				(
					subcommand.get_message().unwrap().to_string(),
					subcommand.get_detailed_message().unwrap().to_string(),
				)
			})
			.collect()
	}

	fn get_upgrade_checks_items() -> Vec<(String, String)> {
		[
			UpgradeCheckSelect::None,
			UpgradeCheckSelect::All,
			UpgradeCheckSelect::TryState,
			UpgradeCheckSelect::PreAndPost,
		]
		.iter()
		.map(|check| get_upgrade_checks_details(*check))
		.collect::<Vec<_>>()
	}

	fn get_mock_snapshot() -> PathBuf {
		std::env::current_dir()
			.unwrap()
			.join("../../tests/snapshots/base_parachain.snap")
			.canonicalize()
			.unwrap()
	}
}
