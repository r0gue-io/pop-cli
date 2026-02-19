// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self,
		traits::{Confirm, Select},
	},
	commands::test::RuntimeTestOutput,
	common::{
		prompt::display_message,
		try_runtime::{
			ArgumentConstructor, BuildRuntimeParams, DEFAULT_BLOCK_TIME, argument_exists,
			check_try_runtime_and_prompt, collect_args, collect_shared_arguments,
			collect_state_arguments, partition_arguments, update_runtime_source,
			update_state_source,
		},
	},
	output::{OutputMode, build_error_with_details, invalid_input_error},
};
use clap::Args;
#[cfg(test)]
use clap::Parser;
use console::style;
use pop_chains::{
	SharedParams, TryRuntimeCliCommand, run_try_runtime,
	state::{State, StateCommand},
	try_runtime::UpgradeCheckSelect,
	upgrade_checks_details,
};
use serde::Serialize;
use std::{str::FromStr, time::Duration};

// Custom arguments which are not in `try-runtime on-runtime-upgrade`.
const CUSTOM_ARGS: [&str; 5] = ["--profile", "--no-build", "-n", "--skip-confirm", "-y"];
const DISABLE_SPEC_VERSION_CHECK: &str = "disable-spec-version-check";
const DISABLE_SPEC_NAME_CHECK: &str = "disable-spec-name-check";

#[derive(Debug, Clone, clap::Parser, Serialize)]
struct Command {
	/// The state to use.
	#[command(subcommand)]
	state: Option<State>,

	/// Select which optional checks to perform. Selects all when no value is given.
	///
	/// - `none`: Perform no checks.
	/// - `all`: Perform all checks (default when --checks is present with no value).
	/// - `pre-and-post`: Perform pre- and post-upgrade checks (default when the arg is not
	///   present).
	/// - `try-state`: Perform the try-state checks.
	///
	/// Performing any checks will potentially invalidate the measured PoV/Weight.
	#[serde(skip_serializing)]
	#[clap(long,
			default_value = "pre-and-post",
			default_missing_value = "all",
			num_args = 0..=1,
			verbatim_doc_comment
    )]
	checks: UpgradeCheckSelect,

	/// Whether to disable weight warnings, useful if the runtime is for a relay chain.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	no_weight_warnings: bool,

	/// Whether to skip enforcing that the new runtime `spec_version` is greater or equal to the
	/// existing `spec_version`.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	disable_spec_version_check: bool,

	/// Whether to disable migration idempotency checks.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	disable_idempotency_checks: bool,

	/// When migrations are detected as not idempotent, enabling this will output a diff of the
	/// storage before and after running the same set of migrations the second time.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	print_storage_diff: bool,

	/// Whether or multi-block migrations should be executed to completion after single block
	/// migrations are completed.
	#[clap(long, default_value = "false", default_missing_value = "true")]
	disable_mbm_checks: bool,

	/// The maximum duration we expect all MBMs combined to take.
	///
	/// This value is just here to ensure that the CLI won't run forever in case of a buggy MBM.
	#[clap(long, default_value = "600")]
	mbm_max_blocks: u32,

	/// The chain blocktime in milliseconds.
	#[clap(long, default_value = &DEFAULT_BLOCK_TIME.to_string())]
	blocktime: u64,
}

#[derive(Args, Serialize)]
pub(crate) struct TestOnRuntimeUpgradeCommand {
	/// Command to test migrations.
	#[clap(flatten)]
	command: Command,
	/// Shared params of the try-runtime commands.
	#[clap(flatten)]
	shared_params: SharedParams,
	/// Build parameters for the runtime binary.
	#[clap(flatten)]
	build_params: BuildRuntimeParams,
}

impl TestOnRuntimeUpgradeCommand {
	/// Executes the command.
	pub(crate) async fn execute(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		output_mode: OutputMode,
	) -> anyhow::Result<RuntimeTestOutput> {
		cli.intro("Testing migrations")?;
		let user_provided_args = std::env::args().collect::<Vec<String>>();
		if let Err(e) = update_runtime_source(
			cli,
			"Do you want to specify which runtime to run the migration on?",
			&user_provided_args,
			&mut self.shared_params.runtime,
			&mut self.build_params.profile,
			self.build_params.no_build,
		)
		.await
		{
			if output_mode == OutputMode::Json {
				return Err(invalid_input_error(e.to_string()));
			}
			display_message(&e.to_string(), false, cli)?;
			return Ok(RuntimeTestOutput::success("on-runtime-upgrade", None));
		}

		// Prompt the user to select the source of runtime state.
		if let Err(e) = update_state_source(cli, &mut self.command.state) {
			if output_mode == OutputMode::Json {
				return Err(invalid_input_error(e.to_string()));
			}
			display_message(&e.to_string(), false, cli)?;
			return Ok(RuntimeTestOutput::success("on-runtime-upgrade", None));
		};

		// If the `checks` argument is not provided, prompt the user to select the upgrade checks.
		if !argument_exists(&user_provided_args, "--checks") {
			match guide_user_to_select_upgrade_checks(cli) {
				Ok(checks) => self.command.checks = checks,
				Err(e) => {
					if output_mode == OutputMode::Json {
						return Err(invalid_input_error(e.to_string()));
					}
					display_message(&e.to_string(), false, cli)?;
					return Ok(RuntimeTestOutput::success("on-runtime-upgrade", None));
				},
			}
		}

		// Run migrations with `try-runtime-cli` binary.
		loop {
			let result = self.run(cli).await;
			// Display the `on-runtime-upgrade` command.
			if let Err(e) = result {
				if output_mode == OutputMode::Json {
					return Err(build_error_with_details(
						"Failed to test runtime upgrade",
						e.to_string(),
					));
				}
				match self.handle_check_errors(e.to_string(), cli) {
					Ok(()) => continue,
					Err(e) => {
						cli.info(self.display()?)?;
						display_message(&e.to_string(), false, cli)?;
						return Ok(RuntimeTestOutput::success("on-runtime-upgrade", None));
					},
				}
			}
			cli.info(self.display()?)?;
			display_message("Tested migrations successfully!", true, cli)?;
			return Ok(RuntimeTestOutput::success("on-runtime-upgrade", None));
		}
	}

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let spinner = cli.spinner();
		let binary_path =
			check_try_runtime_and_prompt(cli, &spinner, self.build_params.skip_confirm).await?;
		cli.warning("NOTE: this may take some time...")?;
		match self.command.state {
			Some(State::Live(ref live_state)) =>
				if let Some(ref uri) = live_state.uri {
					spinner.start(format!(
						"Running migrations against live state at {}...",
						style(&uri).magenta().underlined()
					));
				},
			Some(State::Snap { ref path }) =>
				if let Some(p) = path {
					spinner.start(format!(
						"Running migrations using a snapshot file at {}...",
						p.display()
					));
				},
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		}
		tokio::time::sleep(Duration::from_secs(1)).await;

		let subcommand = self.subcommand()?;
		let user_provided_args = collect_args(std::env::args().skip(3));
		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(&user_provided_args, &subcommand);

		let mut shared_args = vec![];
		collect_shared_arguments(&self.shared_params, &shared_params, &mut shared_args);

		let mut args = vec![];
		self.collect_arguments_before_subcommand(&command_arguments, &mut args);
		args.push(self.subcommand()?);
		collect_state_arguments(&self.command.state, &after_subcommand, &mut args)?;
		#[cfg(test)]
		{
			args.retain(|arg| {
				!matches!(arg.as_str(), "--show-output" | "--nocapture" | "--ignored")
			});
		}
		run_try_runtime(
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
		let mut c = ArgumentConstructor::new(args, user_provided_args);
		c.add(&[], true, "--blocktime", Some(self.command.blocktime.to_string()));
		c.add(&[], true, "--checks", Some(upgrade_checks_details(&self.command.checks).0));
		// For testing.
		c.add(
			&[],
			self.command.disable_spec_version_check,
			"--disable-spec-version-check",
			Some(String::default()),
		);
		self.build_params.add_arguments(&mut c);
		c.finalize(&[]);
	}

	fn display(&self) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test on-runtime-upgrade".to_string()];
		let mut args = vec![];
		let subcommand = self.subcommand()?;
		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(&collect_args(std::env::args().skip(3)), &subcommand);

		collect_shared_arguments(&self.shared_params, &shared_params, &mut args);
		self.collect_arguments_before_subcommand(&command_arguments, &mut args);
		args.push(subcommand);
		collect_state_arguments(&self.command.state, &after_subcommand, &mut args)?;
		cmd_args.extend(args);
		#[cfg(test)]
		{
			cmd_args.retain(|arg| {
				!matches!(arg.as_str(), "--show-output" | "--nocapture" | "--ignored")
			});
		}
		Ok(cmd_args.join(" "))
	}

	fn handle_check_errors(
		&mut self,
		error: String,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
		if error.contains(DISABLE_SPEC_VERSION_CHECK) {
			let disabled = cli
				.confirm(
					"⚠️ New runtime spec version must be greater than the on-chain runtime spec version. \
    				Do you want to disable the spec version check and try again?",
				)
				.interact()?;
			if !disabled {
				return Err(anyhow::anyhow!(format!(
					"Failed to run migrations: Invalid spec version. \
					You can disable the check manually by adding the `--{}` flag.",
					DISABLE_SPEC_VERSION_CHECK
				)));
			}
			self.command.disable_spec_version_check = disabled;
			return Ok(());
		}
		if error.contains(DISABLE_SPEC_NAME_CHECK) {
			let disabled = cli
				.confirm(
					"⚠️ Runtime spec names must match. \
       					Do you want to disable the spec name check and try again?",
				)
				.interact()?;
			if !disabled {
				return Err(anyhow::anyhow!(format!(
					"Failed to run migrations: Invalid spec name. \
					You can disable the check manually by adding the `--{}` flag.",
					DISABLE_SPEC_NAME_CHECK
				)));
			}
			self.shared_params.disable_spec_name_check = disabled;
			return Ok(());
		}
		Err(anyhow::anyhow!(error))
	}

	fn subcommand(&self) -> anyhow::Result<String> {
		Ok(match self.command.state {
			Some(ref state) => StateCommand::from(state).to_string(),
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		})
	}
}

#[cfg(test)]
impl Default for TestOnRuntimeUpgradeCommand {
	fn default() -> Self {
		TestOnRuntimeUpgradeCommand {
			command: Command::try_parse_from(vec![""]).unwrap(),
			shared_params: SharedParams::try_parse_from(vec![""]).unwrap(),
			build_params: BuildRuntimeParams::default(),
		}
	}
}

fn guide_user_to_select_upgrade_checks(
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<UpgradeCheckSelect> {
	let default_upgrade_check = upgrade_checks_details(&UpgradeCheckSelect::All);
	let mut prompt = cli
		.select("Select upgrade checks to perform:")
		.initial_value(default_upgrade_check.0);
	for check in [
		UpgradeCheckSelect::None,
		UpgradeCheckSelect::All,
		UpgradeCheckSelect::TryState,
		UpgradeCheckSelect::PreAndPost,
	] {
		let (value, description) = upgrade_checks_details(&check);
		prompt = prompt.item(value.clone(), value, description);
	}
	let input = prompt.interact()?;
	UpgradeCheckSelect::from_str(&input).map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::common::{
		runtime::{Feature::TryRuntime, get_mock_runtime},
		try_runtime::{
			DEFAULT_BLOCK_HASH, get_mock_snapshot, get_subcommands, source_try_runtime_binary,
		},
		urls,
	};
	use cli::MockCli;
	use pop_chains::{Runtime, state::LiveState};
	use pop_common::Profile;
	use std::path::PathBuf;

	#[tokio::test]
	async fn on_runtime_upgrade_live_state_works() -> anyhow::Result<()> {
		let mut command = TestOnRuntimeUpgradeCommand::default();
		command.build_params.no_build = true;

		source_try_runtime_binary(
			&mut MockCli::new(),
			&crate::cli::Spinner::Mock,
			&crate::cache()?,
			true,
		)
		.await?;
		let mut cli = MockCli::new()
			.expect_intro("Testing migrations")
			.expect_confirm(
				format!(
					"Do you want to specify which runtime to run the migration on?\n{}",
					style("If not provided, use the code of the remote node, or a snapshot.").dim()
				),
				true,
			)
			.expect_select(
				"Choose the build profile of the binary that should be used: ".to_string(),
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				std::env::current_dir()?.display()
			))
			.expect_input(
				"Please, specify the path to the runtime project or the runtime binary.",
				get_mock_runtime(Some(TryRuntime)).to_str().unwrap().to_string(),
			)
			.expect_info(format!(
				"Using runtime at {}",
				get_mock_runtime(Some(TryRuntime)).display()
			))
			.expect_select(
				"Select source of runtime state:",
				Some(true),
				true,
				Some(get_subcommands()),
				0, // live
				None,
			)
			.expect_input("Enter the live chain of your node:", urls::LOCAL.to_string())
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
			--checks=all --profile=debug -n live --uri={} --at={}",
				get_mock_runtime(Some(TryRuntime)).to_str().unwrap(),
				urls::LOCAL,
				DEFAULT_BLOCK_HASH.strip_prefix("0x").unwrap_or_default()
			));
		command.execute(&mut cli, OutputMode::Human).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn on_runtime_upgrade_requires_flags_in_json_mode() -> anyhow::Result<()> {
		let mut command = TestOnRuntimeUpgradeCommand::default();
		let error = command
			.execute(&mut crate::cli::JsonCli, OutputMode::Json)
			.await
			.unwrap_err()
			.to_string();
		assert!(error.contains("interactive prompt required but --json mode is active"));
		Ok(())
	}

	#[tokio::test]
	async fn on_runtime_upgrade_snapshot_works() -> anyhow::Result<()> {
		let mut command = TestOnRuntimeUpgradeCommand::default();
		command.build_params.no_build = true;

		source_try_runtime_binary(
			&mut MockCli::new(),
			&crate::cli::Spinner::Mock,
			&crate::cache()?,
			true,
		)
		.await?;
		let mut cli = MockCli::new()
			.expect_intro("Testing migrations")
			.expect_confirm(
				format!(
					"Do you want to specify which runtime to run the migration on?\n{}",
					style("If not provided, use the code of the remote node, or a snapshot.").dim()
				),
				true,
			)
			.expect_select(
				"Choose the build profile of the binary that should be used: ".to_string(),
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				std::env::current_dir()?.display()
			))
			.expect_input(
				"Please, specify the path to the runtime project or the runtime binary.",
				get_mock_runtime(Some(TryRuntime)).to_str().unwrap().to_string(),
			)
			.expect_info(format!(
				"Using runtime at {}",
				get_mock_runtime(Some(TryRuntime)).display()
			))
			.expect_select(
				"Select source of runtime state:",
				Some(true),
				true,
				Some(get_subcommands()),
				1, // snap
				None,
			)
			.expect_input(
				format!(
					"Enter path to your snapshot file?\n{}.",
					style(
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
				--checks=all --profile=debug -n snap --path={}",
				get_mock_runtime(Some(TryRuntime)).to_str().unwrap(),
				get_mock_snapshot().to_str().unwrap()
			));
		command.execute(&mut cli, OutputMode::Human).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn on_runtime_disable_checks_works() -> anyhow::Result<()> {
		let mut cmd = TestOnRuntimeUpgradeCommand::default();
		cmd.build_params.no_build = true;
		cmd.build_params.profile = Some(Profile::Release);
		cmd.command.state = Some(State::Snap { path: Some(get_mock_snapshot()) });

		source_try_runtime_binary(
			&mut MockCli::new(),
			&crate::cli::Spinner::Mock,
			&crate::cache()?,
			true,
		)
		.await?;
		let mut cli = MockCli::new()
			.expect_intro("Testing migrations")
			.expect_confirm(
				format!(
					"Do you want to specify which runtime to run the migration on?\n{}",
					style("If not provided, use the code of the remote node, or a snapshot.").dim()
				),
				true,
			)
			.expect_warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				std::env::current_dir()?.display()
			))
			.expect_input(
				"Please, specify the path to the runtime project or the runtime binary.",
				get_mock_runtime(None).to_str().unwrap().to_string(),
			)
			.expect_info(format!("Using runtime at {}", get_mock_runtime(None).display()))
			.expect_select(
				"Select upgrade checks to perform:",
				Some(true),
				true,
				Some(get_upgrade_checks_items()),
				1, // all
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_confirm(
				"⚠️ Runtime spec names must match. Do you want to disable the spec name check and try again?",
				true,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_confirm(
				"⚠️ New runtime spec version must be greater than the on-chain runtime spec version. \
				Do you want to disable the spec version check and try again?",
				true,
			);
		cmd.execute(&mut cli, OutputMode::Human).await?;
		cli.verify()
	}

	#[test]
	fn handle_check_errors_works() -> anyhow::Result<()> {
		let mut command = TestOnRuntimeUpgradeCommand::default();

		// --disable-spec-version-check.
		for (confirm, result) in [
			(true, Ok(())),
			(
				false,
				Err(anyhow::anyhow!(
					"Failed to run migrations: Invalid spec version. You can disable the check manually by adding the `--disable-spec-version-check` flag."
				)),
			),
		] {
			let mut cli = MockCli::new().expect_confirm(
				"⚠️ New runtime spec version must be greater than the on-chain runtime spec version. \
				Do you want to disable the spec version check and try again?",
				confirm,
			);
			let _result =
				command.handle_check_errors(DISABLE_SPEC_VERSION_CHECK.to_string(), &mut cli);
			if result.is_ok() {
				assert!(_result.is_ok());
			} else if let Err(error) = result {
				assert_eq!(_result.unwrap_err().to_string(), error.to_string());
			}
		}

		// --disable-spec-name-check.
		for (confirm, result) in [
			(true, Ok(())),
			(
				false,
				Err(anyhow::anyhow!(
					"Failed to run migrations: Invalid spec name. You can disable the check manually by adding the `--disable-spec-name-check` flag."
				)),
			),
		] {
			let mut cli = MockCli::new().expect_confirm(
				"⚠️ Runtime spec names must match. Do you want to disable the spec name check and try again?",
				confirm,
			);
			let _result =
				command.handle_check_errors(DISABLE_SPEC_NAME_CHECK.to_string(), &mut cli);
			if result.is_ok() {
				assert!(_result.is_ok());
			} else if let Err(error) = result {
				assert_eq!(_result.unwrap_err().to_string(), error.to_string());
			}
		}
		Ok(())
	}

	#[tokio::test]
	async fn test_on_runtime_upgrade_invalid_runtime_path() -> anyhow::Result<()> {
		source_try_runtime_binary(
			&mut MockCli::new(),
			&crate::cli::Spinner::Mock,
			&crate::cache()?,
			true,
		)
		.await?;
		let mut cmd = TestOnRuntimeUpgradeCommand::default();
		cmd.shared_params.runtime = Runtime::Path(PathBuf::from("./dummy-runtime-path"));
		cmd.command.state = Some(State::Snap { path: Some(get_mock_snapshot()) });
		let error = cmd.run(&mut MockCli::new()).await.unwrap_err().to_string();
		assert!(error.contains(
			r#"Input("error while reading runtime file from \"./dummy-runtime-path\": Os { code: 2, kind: NotFound, message: \"No such file or directory\" }")"#,
		));
		Ok(())
	}

	#[tokio::test]
	async fn test_on_runtime_upgrade_missing_try_runtime_feature() -> anyhow::Result<()> {
		source_try_runtime_binary(
			&mut MockCli::new(),
			&crate::cli::Spinner::Mock,
			&crate::cache()?,
			true,
		)
		.await?;
		let mut cmd = TestOnRuntimeUpgradeCommand::default();
		cmd.shared_params.runtime = Runtime::Path(get_mock_runtime(None));
		cmd.command.state = Some(State::Snap { path: Some(get_mock_snapshot()) });
		cmd.shared_params.disable_spec_name_check = true;
		cmd.command.disable_spec_version_check = true;
		let error = cmd.run(&mut MockCli::new()).await.unwrap_err().to_string();
		assert!(
			error.contains(
				r#"Input("Given runtime is not compiled with the try-runtime feature.")"#,
			)
		);
		Ok(())
	}

	#[tokio::test]
	async fn test_on_runtime_upgrade_invalid_live_uri() -> anyhow::Result<()> {
		source_try_runtime_binary(
			&mut MockCli::new(),
			&crate::cli::Spinner::Mock,
			&crate::cache()?,
			true,
		)
		.await?;
		let mut cmd = TestOnRuntimeUpgradeCommand::default();
		cmd.shared_params.runtime = Runtime::Path(PathBuf::from("./dummy-runtime-path"));
		cmd.command.state = Some(State::Live(LiveState {
			uri: Some("https://example.com".to_string()),
			..Default::default()
		}));
		let error = cmd.run(&mut MockCli::new()).await.unwrap_err().to_string();
		assert!(error.contains(
			r#"Failed to test with try-runtime: error: invalid value 'https://example.com' for '--uri <URI>': not a valid WS(S) url: must start with 'ws://' or 'wss://'"#,
		));
		Ok(())
	}

	#[test]
	fn collect_arguments_before_subcommand_works() -> anyhow::Result<()> {
		let test_cases: Vec<(&str, Box<dyn Fn(&mut TestOnRuntimeUpgradeCommand)>, &str)> = vec![
			(
				"--blocktime=20",
				Box::new(|cmd| {
					cmd.command.blocktime = 10;
				}),
				"--blocktime=10",
			),
			(
				"--checks=pre-and-post",
				Box::new(|cmd| {
					cmd.command.checks = UpgradeCheckSelect::All;
				}),
				"--checks=all",
			),
			(
				"--profile=release",
				Box::new(|cmd| {
					cmd.build_params.profile = Some(Profile::Debug);
				}),
				"--profile=debug",
			),
			(
				"--no-build",
				Box::new(|cmd| {
					cmd.build_params.no_build = true;
				}),
				"-n",
			),
			(
				"-y",
				Box::new(|cmd| {
					cmd.build_params.skip_confirm = true;
				}),
				"-y",
			),
			(
				"--skip-confirm",
				Box::new(|cmd| {
					cmd.build_params.skip_confirm = true;
				}),
				"-y",
			),
		];
		for (provided_arg, update_fn, expected_arg) in test_cases {
			let mut command = TestOnRuntimeUpgradeCommand::default();
			let mut args = vec![];
			// Keep the user-provided argument unchanged.
			command.collect_arguments_before_subcommand(&[provided_arg.to_string()], &mut args);
			assert_eq!(args.iter().filter(|a| a.contains(&provided_arg.to_string())).count(), 1);

			// If there exists an argument with the same name as the provided argument, skip it.
			command.collect_arguments_before_subcommand(&[], &mut args);
			assert_eq!(args.iter().filter(|a| a.contains(&provided_arg.to_string())).count(), 1);

			// If the user does not provide an argument, modify with the argument updated during
			// runtime.
			let mut args = vec![];
			update_fn(&mut command);
			command.collect_arguments_before_subcommand(&[], &mut args);
			assert_eq!(args.iter().filter(|a| a.contains(&expected_arg.to_string())).count(), 1);
		}
		Ok(())
	}

	#[test]
	fn subcommand_works() -> anyhow::Result<()> {
		let mut command = TestOnRuntimeUpgradeCommand::default();
		command.command.state = Some(State::Live(LiveState::default()));
		assert_eq!(command.subcommand()?, StateCommand::Live.to_string());
		command.command.state = Some(State::Snap { path: Some(PathBuf::default()) });
		assert_eq!(command.subcommand()?, StateCommand::Snap.to_string());
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

	fn get_upgrade_checks_items() -> Vec<(String, String)> {
		[
			UpgradeCheckSelect::None,
			UpgradeCheckSelect::All,
			UpgradeCheckSelect::TryState,
			UpgradeCheckSelect::PreAndPost,
		]
		.iter()
		.map(upgrade_checks_details)
		.collect::<Vec<_>>()
	}
}
