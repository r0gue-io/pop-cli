// SPDX-License-Identifier: GPL-3.0

use super::{
	binary::which_version,
	builds::guide_user_to_select_profile,
	chain::get_pallets,
	runtime::{Feature, ensure_runtime_binary_exists},
};
use crate::{
	cli::traits::*,
	common::{
		binary::{BinaryGenerator, SemanticVersion, check_and_prompt},
		urls,
	},
	impl_binary_generator,
};
use clap::Args;
use cliclack::{ProgressBar, spinner};
use console::style;
use pop_chains::{
	Runtime, SharedParams, parse, set_up_client,
	state::{LiveState, State, StateCommand},
	try_runtime::TryStateSelect,
	try_runtime_generator, try_state_details, try_state_label,
};
use pop_common::Profile;
use serde::Serialize;
use std::{
	cmp::Ordering,
	collections::HashSet,
	env::current_dir,
	fmt::Display,
	path::{Path, PathBuf},
};
use strum::{EnumMessage, VariantArray};

const BINARY_NAME: &str = "try-runtime";
pub const DEFAULT_BLOCK_HASH: &str = "0x0000000000";
pub(crate) const DEFAULT_BLOCK_TIME: u64 = 6000;
pub(crate) const DEFAULT_SNAPSHOT_PATH: &str = "your-parachain.snap";
const TARGET_BINARY_VERSION: SemanticVersion = SemanticVersion(0, 8, 0);

impl_binary_generator!(TryRuntimeGenerator, try_runtime_generator);

/// Build parameters for the runtime binary.
#[derive(Args, Clone, Debug, Default, Serialize)]
pub(crate) struct BuildRuntimeParams {
	/// Build profile [default: release].
	#[clap(long, value_enum)]
	pub profile: Option<Profile>,

	/// Avoid rebuilding the runtime if there is an existing runtime binary.
	#[clap(short = 'n', long)]
	pub no_build: bool,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	pub skip_confirm: bool,
}

impl BuildRuntimeParams {
	/// Adds arguments to the argument constructor. These arguments are used by `try-runtime-cli`.
	pub(crate) fn add_arguments(&self, c: &mut ArgumentConstructor) {
		c.add(&[], true, "--profile", self.profile.map(|p| p.to_string()));
		c.add(&["--no-build"], self.no_build, "-n", Some(String::default()));
		c.add(&["--skip-confirm"], self.skip_confirm, "-y", Some(String::default()));
	}
}

/// Checks the status of the `try-runtime` binary, using the local version if available.
/// If the binary is missing, it is sourced as needed, and if an outdated version exists in cache,
/// the user is prompted to update to the latest release.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_try_runtime_and_prompt(
	cli: &mut impl Cli,
	spinner: &ProgressBar,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(if let Ok(path) = which_version(BINARY_NAME, &TARGET_BINARY_VERSION, &Ordering::Greater) {
		path
	} else {
		source_try_runtime_binary(cli, spinner, &crate::cache()?, skip_confirm).await?
	})
}

/// Prompt to source the `try-runtime` binary.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn source_try_runtime_binary(
	cli: &mut impl Cli,
	spinner: &ProgressBar,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	check_and_prompt::<TryRuntimeGenerator>(cli, spinner, BINARY_NAME, cache_path, skip_confirm)
		.await
}

/// Update the state source.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `state`: The state to update.
pub(crate) fn update_state_source(
	cli: &mut impl Cli,
	state: &mut Option<State>,
) -> anyhow::Result<()> {
	let (subcommand, path, mut live_state) = match state {
		Some(State::Live(state)) => (&StateCommand::Live, None, state.clone()),
		Some(State::Snap { path }) => (&StateCommand::Snap, path.clone(), LiveState::default()),
		None => (guide_user_to_select_state_source(cli)?, None, LiveState::default()),
	};
	match subcommand {
		StateCommand::Live => update_live_state(cli, &mut live_state, state)?,
		StateCommand::Snap => update_snapshot(cli, path, state)?,
	}
	Ok(())
}

/// Update the snapshot state.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `path`: The path to the snapshot file.
/// * `state`: The state to update.
pub(crate) fn update_snapshot(
	cli: &mut impl Cli,
	mut path: Option<PathBuf>,
	state: &mut Option<State>,
) -> anyhow::Result<()> {
	if path.is_none() {
		let snapshot_file: PathBuf = cli
			.input(format!(
				"Enter path to your snapshot file?\n{}.",
				style("Snapshot file can be generated using `pop test create-snapshot` command")
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
	*state = Some(State::Snap { path });
	Ok(())
}

/// Update the live state.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `live_state`: The live state to update.
/// * `state`: The state to update.
pub fn update_live_state(
	cli: &mut impl Cli,
	live_state: &mut LiveState,
	state: &mut Option<State>,
) -> anyhow::Result<()> {
	if live_state.uri.is_none() {
		let uri = cli
			.input("Enter the live chain of your node:")
			.required(true)
			.placeholder(urls::PASEO)
			.interact()?;
		live_state.uri = Some(parse::url(&uri)?);
	}
	if live_state.at.is_none() {
		let block_hash = cli
			.input("Enter the block hash (optional):")
			.required(false)
			.placeholder(DEFAULT_BLOCK_HASH)
			.interact()?;
		if !block_hash.is_empty() {
			live_state.at = Some(parse::hash(&block_hash)?);
		}
	}
	*state = Some(State::Live(live_state.clone()));
	Ok(())
}

/// Update the source of the runtime.
///
/// # Arguments
///
/// * `cli`: Command line interface.
/// * `user_provided_args`: The user provided arguments.
/// * `runtime`: The runtime to update.
/// * `profile`: The build profile.
/// * `no_build`: Whether to build the runtime.
pub(crate) async fn update_runtime_source(
	cli: &mut impl Cli,
	prompt: &str,
	user_provided_args: &[String],
	runtime: &mut Runtime,
	profile: &mut Option<Profile>,
	no_build: bool,
) -> anyhow::Result<()> {
	if !argument_exists(user_provided_args, "--runtime") &&
		cli.confirm(format!(
			"{}\n{}",
			prompt,
			style("If not provided, use the code of the remote node, or a snapshot.").dim()
		))
		.initial_value(true)
		.interact()?
	{
		if profile.is_none() {
			*profile = Some(guide_user_to_select_profile(cli)?);
		};
		if no_build {
			cli.warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")?;
		}
		let (binary_path, _) = ensure_runtime_binary_exists(
			cli,
			&current_dir().unwrap_or(PathBuf::from("./")),
			profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
			&[Feature::TryRuntime],
			!no_build,
			false,
			&None,
			None,
		)
		.await?;
		*runtime = Runtime::Path(binary_path);
	}
	Ok(())
}

fn guide_user_to_select_state_source(cli: &mut impl Cli) -> anyhow::Result<&StateCommand> {
	let mut prompt = cli.select("Select source of runtime state:");
	for subcommand in StateCommand::VARIANTS.iter() {
		prompt = prompt.item(
			subcommand,
			subcommand.get_message().unwrap(),
			subcommand.get_detailed_message().unwrap(),
		);
	}
	prompt.interact().map_err(anyhow::Error::from)
}

/// Guides the user to select the state tests.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `url`: URL of the live node.
pub async fn guide_user_to_select_try_state(
	cli: &mut impl Cli,
	url: Option<String>,
) -> anyhow::Result<TryStateSelect> {
	let default_try_state_select = try_state_details(&TryStateSelect::All);
	let input = {
		let mut prompt = cli
			.select("Select state tests to execute:")
			.initial_value(default_try_state_select.0);
		for option in [
			TryStateSelect::None,
			TryStateSelect::All,
			TryStateSelect::RoundRobin(0),
			TryStateSelect::Only(vec![]),
		] {
			let (value, description) = try_state_details(&option);
			prompt = prompt.item(value.clone(), value, description);
		}
		prompt.interact()?
	};
	Ok(match input.as_str() {
		s if s == try_state_label(&TryStateSelect::None) => TryStateSelect::None,
		s if s == try_state_label(&TryStateSelect::All) => TryStateSelect::All,
		s if s == try_state_label(&TryStateSelect::RoundRobin(0)) => {
			let input = cli
				.input("Enter the number of rounds:")
				.placeholder("10")
				.required(true)
				.interact()?;
			let rounds = input.parse::<u32>();
			if rounds.is_err() {
				return Err(anyhow::anyhow!("Must be a positive integer"));
			}
			TryStateSelect::RoundRobin(rounds?)
		},
		s if s == try_state_label(&TryStateSelect::Only(vec![])) => match url {
			Some(url) => {
				let spinner = spinner();
				spinner.start("Retrieving available pallets...");
				let client = set_up_client(&url).await?;
				let pallets = get_pallets(&client).await?;
				let mut prompt = cli
					.multiselect("Select pallets (select with SPACE):")
					.required(true)
					.filter_mode();
				for pallet in pallets {
					prompt = prompt.item(pallet.name.clone(), pallet.name, pallet.docs);
				}
				spinner.clear();
				let selected_pallets = prompt.interact()?;
				TryStateSelect::Only(
					selected_pallets
						.iter()
						.map(|pallet| pallet.trim().as_bytes().to_vec())
						.collect(),
				)
			},
			None => {
				let input = cli
					.input(format!(
						"Enter the pallet names separated by commas:\n{}",
						style(
							"Pallet names must be capitalized exactly as defined in the runtime."
						)
						.dim()
					))
					.placeholder("System, Balances, Proxy")
					.required(true)
					.interact()?;
				TryStateSelect::Only(
					input.split(",").map(|pallet| pallet.trim().as_bytes().to_vec()).collect(),
				)
			},
		},
		_ => TryStateSelect::All,
	})
}

/// Construct arguments based on provided conditions.
///
/// # Arguments
///
/// * `args` - A mutable reference to a vector of arguments.
/// * `seen` - A set of arguments already seen.
/// * `user_provided_args` - A slice of user-provided arguments.
pub(crate) struct ArgumentConstructor<'a> {
	args: &'a mut Vec<String>,
	user_provided_args: &'a [String],
	seen: HashSet<String>,
	added: HashSet<String>,
}

impl<'a> ArgumentConstructor<'a> {
	/// Creates a new instance of `ArgumentConstructor`.
	pub fn new(args: &'a mut Vec<String>, user_provided_args: &'a [String]) -> Self {
		let cloned_args = args.clone();
		let mut constructor =
			Self { args, user_provided_args, added: HashSet::default(), seen: HashSet::default() };
		for arg in cloned_args.iter() {
			constructor.mark_added(arg);
			constructor.mark_seen(arg);
		}
		for arg in user_provided_args {
			constructor.mark_seen(arg);
		}
		constructor
	}

	/// Adds an argument and mark it as seen.
	pub fn add(
		&mut self,
		condition_args: &[&str],
		external_condition: bool,
		flag: &str,
		value: Option<String>,
	) {
		if !self.seen.contains(flag) &&
			condition_args.iter().all(|a| !self.seen.contains(*a)) &&
			external_condition &&
			let Some(v) = value
		{
			if !v.is_empty() {
				self.args.push(format_arg(flag, v));
			} else {
				self.args.push(flag.to_string());
			}
			self.mark_added(flag);
		}
	}

	/// Finalizes the argument construction process.
	pub fn finalize(&mut self, skipped: &[&str]) -> Vec<String> {
		// Exclude arguments that are already included.
		for arg in self.user_provided_args.iter() {
			if skipped.iter().any(|a| a == arg) {
				continue;
			}
			if !self.added.contains(arg) {
				self.args.push(arg.clone());
				self.mark_added(arg);
			}
		}
		self.args.clone()
	}

	fn mark_seen(&mut self, arg: &str) {
		let parts = arg.split("=").collect::<Vec<&str>>();
		self.seen.insert(parts[0].to_string());
	}

	fn mark_added(&mut self, arg: &str) {
		let parts = arg.split("=").collect::<Vec<&str>>();
		self.added.insert(parts[0].to_string());
	}
}

/// Checks if an argument exists in the given list of arguments.
///
/// # Arguments
/// * `args`: The list of arguments.
/// * `arg`: The argument to check for.
pub(crate) fn argument_exists(args: &[String], arg: &str) -> bool {
	args.iter().any(|a| a.starts_with(arg))
}

/// Collect arguments shared across all `try-runtime-cli` commands.
///
/// # Arguments
/// * `shared_params` - The shared parameters.
/// * `user_provided_args` - The user-provided arguments.
/// * `args` - The vector of arguments to be collected.
pub(crate) fn collect_shared_arguments(
	shared_params: &SharedParams,
	user_provided_args: &[String],
	args: &mut Vec<String>,
) {
	let mut c = ArgumentConstructor::new(args, user_provided_args);
	c.add(
		&[],
		true,
		"--runtime",
		Some(
			match shared_params.runtime {
				Runtime::Path(ref path) => path.to_str().unwrap(),
				Runtime::Existing => "existing",
			}
			.to_string(),
		),
	);
	// For testing.
	c.add(
		&[],
		shared_params.disable_spec_name_check,
		"--disable-spec-name-check",
		Some(String::default()),
	);
	c.finalize(&[]);
}

/// Collect arguments for the `state` command.
///
/// # Arguments
///
/// * `state` - The state of the runtime.
/// * `user_provided_args` - The user-provided arguments.
/// * `args` - The vector of arguments to be collected.
pub(crate) fn collect_state_arguments(
	state: &Option<State>,
	user_provided_args: &[String],
	args: &mut Vec<String>,
) -> Result<(), anyhow::Error> {
	let mut c = ArgumentConstructor::new(args, user_provided_args);
	match state.as_ref().ok_or_else(|| anyhow::anyhow!("No state provided"))? {
		State::Live(state) => {
			c.add(&[], true, "--uri", state.uri.clone());
			c.add(&[], true, "--at", state.at.clone());
		},
		State::Snap { path } =>
			if let Some(path) = path {
				let path = path.to_str().unwrap().to_string();
				c.add(&[], !path.is_empty(), "--path", Some(path));
			},
	}
	c.finalize(&["--at="]);
	Ok(())
}

/// Partition arguments into command-specific arguments, shared arguments, and remaining arguments.
///
/// # Arguments
///
/// * `args` - A vector of arguments to be partitioned.
/// * `subcommand` - The name of the subcommand.
pub(crate) fn partition_arguments(
	args: &[String],
	subcommand: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
	let mut command_parts = args.split(|arg| arg == subcommand);
	let (before_subcommand, after_subcommand) =
		(command_parts.next().unwrap_or_default(), command_parts.next().unwrap_or_default());
	let (mut command_arguments, mut shared_arguments): (Vec<String>, Vec<String>) =
		(vec![], vec![]);
	for arg in before_subcommand.iter().cloned() {
		if SharedParams::has_argument(&arg) {
			shared_arguments.push(arg);
		} else {
			command_arguments.push(arg);
		}
	}
	(command_arguments, shared_arguments, after_subcommand.to_vec())
}

/// Formats an argument and its value into a string.
pub(crate) fn format_arg<A: Display, V: Display>(arg: A, value: V) -> String {
	format!("{}={}", arg, value)
}

/// Collects arguments and returns a vector of formatted arguments.
pub(crate) fn collect_args<A: Iterator<Item = String>>(args: A) -> Vec<String> {
	let mut format_args = Vec::new();
	let mut args = args.peekable();
	while let Some(arg) = args.next() {
		if (arg.starts_with("--") || arg.starts_with("-")) &&
			!arg.contains("=") &&
			let Some(value) = args.peek() &&
			!value.starts_with("--") &&
			!value.starts_with("-")
		{
			let next_value = args.next().unwrap();
			format_args.push(format_arg(arg, next_value));
			continue;
		}
		format_args.push(arg);
	}
	format_args
}

#[cfg(any(test, feature = "integration-tests"))]
#[allow(dead_code)]
pub fn get_mock_snapshot() -> PathBuf {
	std::env::current_dir()
		.unwrap()
		.join("../../tests/snapshots/base_parachain.snap")
		.canonicalize()
		.unwrap()
}

#[cfg(any(test, feature = "integration-tests"))]
#[allow(dead_code)]
pub fn get_subcommands() -> Vec<(String, String)> {
	StateCommand::VARIANTS
		.iter()
		.map(|subcommand| {
			(
				subcommand.get_message().unwrap().to_string(),
				subcommand.get_detailed_message().unwrap().to_string(),
			)
		})
		.collect()
}

#[cfg(any(test, feature = "integration-tests"))]
#[allow(dead_code)]
pub fn get_try_state_items() -> Vec<(String, String)> {
	[
		TryStateSelect::All,
		TryStateSelect::None,
		TryStateSelect::RoundRobin(0),
		TryStateSelect::Only(vec![]),
	]
	.iter()
	.map(try_state_details)
	.collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::MockCli,
		common::{binary::SemanticVersion, runtime::get_mock_runtime},
	};
	use clap::Parser;
	use tempfile::tempdir;

	#[derive(Default)]
	struct MockCommand {
		state: Option<State>,
	}

	#[test]
	fn update_snapshot_state_works() -> anyhow::Result<()> {
		let snapshot_file = get_mock_snapshot();
		// Prompt for snapshot path if not provided.
		let mut cmd = MockCommand::default();
		let mut cli = MockCli::new().expect_input(
			format!(
				"Enter path to your snapshot file?\n{}.",
				style("Snapshot file can be generated using `pop test create-snapshot` command")
					.dim()
			),
			snapshot_file.to_str().unwrap().to_string(),
		);
		update_snapshot(&mut cli, None, &mut cmd.state)?;
		match cmd.state {
			Some(State::Snap { ref path }) => {
				assert_eq!(path.as_ref().unwrap(), snapshot_file.as_path());
			},
			_ => panic!("Expected snapshot state"),
		}
		cli.verify()?;

		// Use provided path without prompting.
		let mut cmd = MockCommand::default();
		let snapshot_path = Some(snapshot_file);
		let mut cli = MockCli::new(); // No prompt expected
		update_snapshot(&mut cli, snapshot_path.clone(), &mut cmd.state)?;
		match cmd.state {
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
		let mut cmd = MockCommand::default();
		let mut cli = MockCli::new().expect_input(
			format!(
				"Enter path to your snapshot file?\n{}.",
				style("Snapshot file can be generated using `pop test create-snapshot` command")
					.dim()
			),
			"invalid-path-to-file".to_string(),
		);
		assert!(matches!(
			update_snapshot(&mut cli, None, &mut cmd.state),
			Err(message) if message.to_string().contains("Invalid path to the snapshot file.")
		));
		cli.verify()?;
		Ok(())
	}

	#[tokio::test]
	async fn update_runtime_source_works() -> anyhow::Result<()> {
		let mut runtime = Runtime::Existing;
		let mut profile = None;
		let mut cli = MockCli::new()
			.expect_confirm(
				format!(
					"Do you want to specify a runtime?\n{}",
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
				get_mock_runtime(Some(Feature::TryRuntime)).to_str().unwrap().to_string(),
			)
			.expect_info(format!(
				"Using runtime at {}",
				get_mock_runtime(Some(Feature::TryRuntime)).display()
			));
		update_runtime_source(
			&mut cli,
			"Do you want to specify a runtime?",
			&[],
			&mut runtime,
			&mut profile,
			true,
		)
		.await?;
		cli.verify()?;
		match runtime {
			Runtime::Existing => panic!("Unexpected runtime"),
			Runtime::Path(ref path) => {
				assert_eq!(path, &get_mock_runtime(Some(Feature::TryRuntime)))
			},
		}
		assert_eq!(profile, Some(Profile::Debug));

		// If `--runtime` is provided, don't prompt for runtime selection.
		let mut cli = MockCli::new();
		update_runtime_source(
			&mut cli,
			"",
			&["--runtime=dummy-runtime-path".to_string()],
			&mut runtime,
			&mut profile,
			true,
		)
		.await?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn guide_user_to_select_state_source_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_select(
			"Select source of runtime state:",
			Some(true),
			true,
			Some(get_subcommands()),
			0,
			None,
		);
		assert_eq!(guide_user_to_select_state_source(&mut cli)?, &StateCommand::Live);
		Ok(())
	}

	#[test]
	fn add_argument_without_value_works() {
		let mut args = vec![];
		let user_provided_args = vec![];
		let mut constructor = ArgumentConstructor::new(&mut args, &user_provided_args);
		constructor.add(&[], true, "--flag", Some(String::default()));
		assert_eq!(constructor.finalize(&[]), vec!["--flag".to_string()]);
		assert!(constructor.added.contains("--flag"));
	}

	#[test]
	fn skip_argument_when_already_seen_works() {
		let mut args = vec!["--existing".to_string()];
		let user_provided_args = vec![];
		ArgumentConstructor::new(&mut args, &user_provided_args).add(
			&[],
			true,
			"--existing",
			Some("ignored".to_string()),
		);
		// No duplicates added.
		assert_eq!(args, vec!["--existing".to_string()]);
	}

	#[test]
	fn skip_argument_based_on_condition_works() {
		let mut args = vec![];
		let user_provided_args = vec![];
		let mut constructor = ArgumentConstructor::new(&mut args, &user_provided_args);
		constructor.add(&["--skip"], false, "--flag", Some("value".to_string()));
		// Condition is false, so it should not add.
		assert!(args.is_empty());
	}

	#[test]
	fn finalize_adds_missing_user_arguments_works() {
		let mut args = vec![];
		let user_provided_args = vec!["--user-arg".to_string(), "--another-arg".to_string()];
		let mut constructor = ArgumentConstructor::new(&mut args, &user_provided_args);
		constructor.finalize(&[]);
		assert!(args.contains(&"--user-arg".to_string()));
		assert!(args.contains(&"--another-arg".to_string()));
	}

	#[test]
	fn finalize_skips_provided_arguments_works() {
		let mut args = vec![];
		let user_provided_args = vec!["--keep".to_string(), "--skip-me".to_string()];
		let mut constructor = ArgumentConstructor::new(&mut args, &user_provided_args);
		constructor.finalize(&["--skip-me"]);
		assert!(args.contains(&"--keep".to_string()));
		assert!(!args.contains(&"--skip-me".to_string())); // Skipped argument should not be added
	}

	#[test]
	fn argument_exists_works() {
		let args = vec![
			"--uri=http://example.com".to_string(),
			"--at=block123".to_string(),
			"--runtime".to_string(),
			"mock-runtime".to_string(),
		];
		assert!(argument_exists(&args, "--runtime"));
		assert!(argument_exists(&args, "--uri"));
		assert!(argument_exists(&args, "--at"));
		assert!(!argument_exists(&args, "--path"));
		assert!(!argument_exists(&args, "--custom-arg"));
	}

	#[test]
	fn collect_shared_arguments_works() -> anyhow::Result<()> {
		// Keep the user-provided argument unchanged.
		let user_provided_args = vec!["--runtime".to_string(), "dummy-runtime".to_string()];
		let mut args = vec![];
		collect_shared_arguments(
			&SharedParams::try_parse_from(vec![""])?,
			&user_provided_args,
			&mut args,
		);
		assert_eq!(args, user_provided_args);

		// If the user does not provide a URI argument, modify with the argument updated during
		// runtime.
		let shared_params = SharedParams::try_parse_from(vec!["", "--runtime=path-to-runtime"])?;
		let mut args = vec![];
		collect_shared_arguments(&shared_params, &[], &mut args);
		assert_eq!(args, vec!["--runtime=path-to-runtime".to_string()]);
		Ok(())
	}

	#[test]
	fn collect_live_state_arguments_works() -> anyhow::Result<()> {
		let mut cmd = MockCommand { state: Some(State::Live(LiveState::default())) };

		// No arguments.
		let mut args = vec![];
		collect_state_arguments(&cmd.state, &[], &mut args)?;
		assert!(args.is_empty());

		let mut live_state = LiveState { uri: Some(urls::LOCAL.to_string()), ..Default::default() };
		cmd.state = Some(State::Live(live_state.clone()));
		// Keep the user-provided argument unchanged.
		let user_provided_args = &["--uri".to_string(), urls::LOCAL.to_string()];
		let mut args = vec![];
		collect_state_arguments(&cmd.state, user_provided_args, &mut args)?;
		assert_eq!(args, user_provided_args);

		// If the user does not provide a `--uri` argument, modify with the argument updated during
		// runtime.
		let mut args = vec![];
		collect_state_arguments(&cmd.state, &[], &mut args)?;
		assert_eq!(args, vec![format!("--uri={}", live_state.uri.clone().unwrap_or_default())]);

		live_state.at = Some(DEFAULT_BLOCK_HASH.to_string());
		cmd.state = Some(State::Live(live_state.clone()));
		// Keep the user-provided argument unchanged.
		let user_provided_args = &[
			format!("--uri={}", live_state.uri.clone().unwrap_or_default()),
			"--at".to_string(),
			"0x1234567890".to_string(),
		];
		let mut args = vec![];
		collect_state_arguments(&cmd.state, user_provided_args, &mut args)?;
		assert_eq!(args, user_provided_args);

		// Not allow empty `--at`.
		let user_provided_args =
			&[format!("--uri={}", live_state.uri.clone().unwrap_or_default()), "--at=".to_string()];
		let mut args = vec![];
		collect_state_arguments(&cmd.state, user_provided_args, &mut args)?;
		assert_eq!(args, vec![format!("--uri={}", live_state.uri.clone().unwrap_or_default())]);

		// If the user does not provide a block hash `--at` argument, modify with the argument
		// updated during runtime.
		let mut args = vec![];
		collect_state_arguments(&cmd.state, &[], &mut args)?;
		assert_eq!(
			args,
			vec![
				format!("--uri={}", live_state.uri.unwrap_or_default()),
				format!("--at={}", live_state.at.unwrap_or_default())
			]
		);
		Ok(())
	}

	#[test]
	fn collect_snap_state_arguments_works() -> anyhow::Result<()> {
		let mut cmd = MockCommand { state: Some(State::Snap { path: Some(PathBuf::default()) }) };

		// No arguments.
		let mut args = vec![];
		collect_state_arguments(&cmd.state, &[], &mut args)?;
		assert!(args.is_empty());

		let state = State::Snap { path: Some(PathBuf::from("./existing-file")) };
		cmd.state = Some(state);
		// Keep the user-provided argument unchanged.
		let user_provided_args = &["--path".to_string(), "./path-to-file".to_string()];
		let mut args = vec![];
		collect_state_arguments(&cmd.state, user_provided_args, &mut args)?;
		assert_eq!(args, user_provided_args);

		// If the user does not provide a `--path` argument, modify with the argument updated during
		// runtime.
		let mut args = vec![];
		collect_state_arguments(&cmd.state, &[], &mut args)?;
		assert_eq!(args, vec!["--path=./existing-file"]);
		Ok(())
	}

	#[test]
	fn partition_arguments_works() {
		let subcommand = "run";
		let (command_args, shared_params, after_subcommand) =
			partition_arguments(&[subcommand.to_string()], subcommand);

		assert!(command_args.is_empty());
		assert!(shared_params.is_empty());
		assert!(after_subcommand.is_empty());

		let args = vec![
			"--runtime=runtime_name".to_string(),
			"--wasm-execution=instantiate".to_string(),
			"--command=command_name".to_string(),
			"run".to_string(),
			"--arg1".to_string(),
			"--arg2".to_string(),
		];
		let (command_args, shared_params, after_subcommand) =
			partition_arguments(&args, subcommand);
		assert_eq!(command_args, vec!["--command=command_name".to_string()]);
		assert_eq!(
			shared_params,
			vec!["--runtime=runtime_name".to_string(), "--wasm-execution=instantiate".to_string()]
		);
		assert_eq!(after_subcommand, vec!["--arg1".to_string(), "--arg2".to_string()]);
	}

	#[test]
	fn format_arg_works() {
		assert_eq!(format_arg("--number", 1), "--number=1");
		assert_eq!(format_arg("--string", "value"), "--string=value");
		assert_eq!(format_arg("--boolean", true), "--boolean=true");
		assert_eq!(format_arg("--path", PathBuf::new().display()), "--path=");
	}

	#[test]
	fn add_build_runtime_params_works() {
		for (user_provided_args, params, expected) in [
			(
				vec![],
				BuildRuntimeParams { no_build: true, profile: None, skip_confirm: true },
				vec!["-n", "-y"],
			),
			(
				vec!["--arg1", "--arg2"],
				BuildRuntimeParams {
					no_build: true,
					profile: Some(Profile::Debug),
					skip_confirm: true,
				},
				vec!["--profile=debug", "-n", "-y", "--arg1", "--arg2"],
			),
			(
				vec!["--no-build", "--skip-confirm", "--arg1", "--arg2"],
				BuildRuntimeParams {
					no_build: true,
					profile: Some(Profile::Debug),
					skip_confirm: true,
				},
				vec!["--profile=debug", "--no-build", "--skip-confirm", "--arg1", "--arg2"],
			),
		] {
			let args = &mut vec![];
			let user_provided_args: Vec<String> =
				user_provided_args.iter().map(|a| a.to_string()).collect();
			let mut c = ArgumentConstructor::new(args, &user_provided_args);
			params.add_arguments(&mut c);
			assert_eq!(c.finalize(&[]), expected);
		}
	}

	#[test]
	fn args_works() {
		// Empty input
		assert!(collect_args(vec![].into_iter()).is_empty());
		// Single standalone flag
		assert_eq!(
			collect_args(vec!["--flag".to_string()].into_iter()),
			vec!["--flag".to_string()],
		);
		// Combining key-value pairs
		assert_eq!(
			collect_args(vec!["--runtime".to_string(), "runtime-path".to_string()].into_iter()),
			vec!["--runtime=runtime-path".to_string()],
		);
		// Multiple standalone and key-value flags
		assert_eq!(
			collect_args(
				vec!["--flag".to_string(), "--runtime".to_string(), "runtime-path".to_string()]
					.into_iter()
			),
			vec!["--flag".to_string(), "--runtime=runtime-path".to_string()],
		);
		// Already formatted arguments remain unchanged
		assert_eq!(
			collect_args(
				vec!["--arg=123".to_string(), "--flag-1".to_string(), "--flag-2".to_string()]
					.into_iter()
			),
			vec!["--arg=123".to_string(), "--flag-1".to_string(), "--flag-2".to_string()],
		);
		// Short flag format (-r value -> -r=value)
		assert_eq!(
			collect_args(vec!["-r".to_string(), "123".to_string()].into_iter()),
			vec!["-r=123".to_string()],
		);
		// Mixing short and long flags
		assert_eq!(
			collect_args(
				vec!["-x".to_string(), "42".to_string(), "--long".to_string(), "foo".to_string(),]
					.into_iter()
			),
			vec!["-x=42".to_string(), "--long=foo".to_string()],
		);
		// Ensures flags with equal signs remain unchanged
		assert_eq!(
			collect_args(
				vec!["--key=value".to_string(), "-a=100".to_string(), "--other-flag".to_string()]
					.into_iter()
			),
			vec!["--key=value".to_string(), "-a=100".to_string(), "--other-flag".to_string()],
		);
		// Edge case: multiple short flags and values
		assert_eq!(
			collect_args(
				vec![
					"-a".to_string(),
					"1".to_string(),
					"-b=3".to_string(),
					"2".to_string(),
					"--verbose".to_string()
				]
				.into_iter()
			),
			vec!["-a=1".to_string(), "-b=3".to_string(), "2".to_string(), "--verbose".to_string()],
		);
		// Edge case: argument without value remains unchanged
		assert_eq!(
			collect_args(
				vec![
					"--debug".to_string(),
					"-v".to_string(),
					"--output".to_string(),
					"result.txt".to_string()
				]
				.into_iter()
			),
			vec!["--debug".to_string(), "-v".to_string(), "--output=result.txt".to_string()],
		);
	}

	#[tokio::test]
	async fn try_runtime_version_works() -> anyhow::Result<()> {
		let cache_path = tempdir().expect("Could create temp dir");
		let path =
			source_try_runtime_binary(&mut MockCli::new(), &spinner(), cache_path.path(), true)
				.await?;
		assert!(
			SemanticVersion::try_from(path.to_str().unwrap().to_string())? >= TARGET_BINARY_VERSION
		);
		Ok(())
	}
}
