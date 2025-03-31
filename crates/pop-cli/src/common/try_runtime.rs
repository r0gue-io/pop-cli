// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	common::binary::{check_and_prompt, BinaryGenerator},
	impl_binary_generator,
};
use clap::{Args, Parser};
use duct::cmd;
use pop_common::sourcing::Binary;
use pop_parachains::{try_runtime_generator, Runtime, SharedParams};
use std::{
	collections::HashSet,
	fmt::Display,
	path::{Path, PathBuf},
};

const BINARY_NAME: &str = "try-runtime";

impl_binary_generator!(TryRuntimeGenerator, try_runtime_generator);

/// Construct a Try Runtime command with shared parameters.
#[derive(Args)]
pub(crate) struct TryRuntimeCommand<T>
where
	T: Parser + Args,
{
	/// Subcommand of try-runtime.
	#[clap(flatten)]
	pub command: T,
	/// Shared params of the try-runtime commands.
	#[clap(flatten)]
	pub shared_params: SharedParams,
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
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(match cmd("which", &[BINARY_NAME]).stdout_capture().run() {
		Ok(output) => {
			let path = String::from_utf8(output.stdout)?;
			PathBuf::from(path.trim())
		},
		Err(_) => source_try_runtime_binary(cli, &crate::cache()?, skip_confirm).await?,
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
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	check_and_prompt::<TryRuntimeGenerator>(cli, BINARY_NAME, cache_path, skip_confirm).await
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
		if !self.seen.contains(flag)
			&& condition_args.iter().all(|a| !self.seen.contains(*a))
			&& external_condition
		{
			if let Some(v) = value {
				if !v.is_empty() {
					self.args.push(format_arg(flag, v));
				} else {
					self.args.push(flag.to_string());
				}
				self.mark_added(flag);
			}
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

/// Partition arguments into command-specific arguments, shared arguments, and remaining arguments.
///
/// # Arguments
///
/// * `args` - A vector of arguments to be partitioned.
/// * `subcommand` - The name of the subcommand.
pub(crate) fn partition_arguments(
	args: Vec<String>,
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

#[cfg(test)]
mod tests {
	use super::*;
	use clap::Parser;

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
		collect_shared_arguments(&shared_params, &vec![], &mut args);
		assert_eq!(args, vec!["--runtime=path-to-runtime".to_string()]);
		Ok(())
	}

	#[test]
	fn partition_arguments_works() {
		let subcommand = "run";
		let (command_args, shared_params, after_subcommand) =
			partition_arguments(vec![subcommand.to_string()], subcommand);

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
		let (command_args, shared_params, after_subcommand) = partition_arguments(args, subcommand);
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
}
