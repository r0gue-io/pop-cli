// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Input},
	common::{
		prompt::display_message,
		try_runtime::{check_try_runtime_and_prompt, collect_args, ArgumentConstructor},
	},
};
use clap::Args;
use cliclack::spinner;
use console::style;
use pop_parachains::{parse::url, run_try_runtime, state::LiveState, TryRuntimeCliCommand};

// Custom arguments which are not in `try-runtime create-snapshot`.
const CUSTOM_ARGS: [&str; 2] = ["--skip-confirm", "-y"];
const DEFAULT_REMOTE_NODE_URL: &str = "wss://rpc1.paseo.popnetwork.xyz";
const DEFAULT_SNAPSHOT_PATH: &str = "example.snap";

#[derive(Args, Default)]
pub(crate) struct TestCreateSnapshotCommand {
	/// The source of the snapshot. Must be a remote node.
	#[clap(flatten)]
	from: LiveState,

	/// The snapshot path to write to.
	///
	/// If not provided `<spec-name>-<spec-version>@<block-hash>.snap` will be used.
	#[clap(index = 1)]
	snapshot_path: Option<String>,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl TestCreateSnapshotCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Creating a snapshot file")?;
		cli.warning(
			"NOTE: `create-snapshot` only works with the remote node. No runtime required.",
		)?;

		if self.from.uri.is_none() {
			let input = cli
				.input("Enter the URI of the remote node:")
				.placeholder(DEFAULT_REMOTE_NODE_URL)
				.required(true)
				.interact()?;
			self.from.uri = Some(url(input.trim())?);
		}
		if self.snapshot_path.is_none() {
			let input = cli
				.input(format!(
         			"Enter the path to write the snapshot to (optional):\n{}",
         			style("If not provided `<spec-name>-<spec-version>@<block-hash>.snap` will be used.").dim()
          		))
				.required(false).placeholder(DEFAULT_SNAPSHOT_PATH)
				.interact()?;
			if !input.is_empty() {
				self.snapshot_path = Some(input);
			}
		}

		// Create a snapshot with `try-runtime-cli` binary.
		let result = self.run(cli).await;

		// Display the `create-snapshot` command.
		cli.info(self.display())?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message(
			&format!(
				"Snapshot is created successfully!{}",
				if let Some(p) = self.snapshot_path {
					style(format!("\n{} Generated snapshot file: {}", console::Emoji("●", ">"), p))
						.dim()
						.to_string()
				} else {
					String::default()
				}
			),
			true,
			cli,
		)?;
		Ok(())
	}

	async fn run(&self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let spinner = spinner();
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		if let Some(ref uri) = self.from.uri {
			spinner.start(format!(
				"Creating a snapshot of a remote node at {}...\n{}",
				console::style(&uri).magenta().underlined(),
				style("Depends on the size of the network's state, this may take a while.").dim()
			));
		}
		run_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::CreateSnapshot,
			vec![],
			self.collect_arguments(&user_provided_args),
			&CUSTOM_ARGS,
		)?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut cmd_args = vec!["pop test create-snapshot".to_string()];
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		cmd_args.extend(self.collect_arguments(&user_provided_args));
		cmd_args.join(" ")
	}

	fn collect_arguments(&self, user_provided_args: &[String]) -> Vec<String> {
		// Remove snapshot path from the provided arguments.
		let mut provided_path = None;
		let mut user_provided_args = user_provided_args.to_vec();
		if let Some(arg) = user_provided_args.last() {
			if !arg.starts_with("--") && arg.ends_with(".snap") {
				provided_path = user_provided_args.pop();
			}
		}
		let collected_args = collect_args(user_provided_args.into_iter());
		let mut args = vec![];
		let mut c = ArgumentConstructor::new(&mut args, &collected_args);
		c.add(&[], true, "--uri", self.from.uri.clone());
		c.add(&["--skip-confirm"], self.skip_confirm, "-y", Some(String::default()));
		c.finalize(&[]);

		// If the last argument is a snapshot path, remove it.
		if let Some(path) = provided_path {
			args.push(path);
		} else if let Some(ref path) = self.snapshot_path {
			args.push(path.to_string());
		}
		args
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::common::try_runtime::source_try_runtime_binary;
	use cli::MockCli;
	use tempfile::tempdir;

	#[ignore = "Issue creating the snapshot in the provided URL."]
	#[tokio::test]
	async fn create_snapshot_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let temp_dir_path = temp_dir.path().join("example.snap");
		let command = TestCreateSnapshotCommand::default();
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;

		let mut cli = MockCli::new()
			.expect_intro("Creating a snapshot file")
			.expect_warning(
				"NOTE: `create-snapshot` only works with the remote node. No runtime required.",
			)
			.expect_input(
				"Enter the URI of the remote node:",
				DEFAULT_REMOTE_NODE_URL.to_string()
			).expect_input(
			    format!(
         			"Enter the path to write the snapshot to (optional):\n{}",
         			style("If not provided `<spec-name>-<spec-version>@<block-hash>.snap` will be used.").dim()
          		),
                temp_dir_path.to_str().unwrap().to_string()
			).expect_outro(
    			format!(
    				"Snapshot is created successfully!{}",
    				style(
                        format!(
                            "\n{} Generated snapshot file: {}",
                            console::Emoji("●", ">"),
                            temp_dir_path.to_str().unwrap().to_string()
                        )
                    ).dim().to_string()
    			)
			);
		command.execute(&mut cli).await?;
		assert!(temp_dir_path.exists());
		cli.verify()
	}

	#[tokio::test]
	async fn create_snapshot_invalid_uri() -> anyhow::Result<()> {
		let mut command = TestCreateSnapshotCommand::default();
		command.from.uri = Some("ws://127.0.0.1:9945".to_string());
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;

		let error = command.run(&mut MockCli::new()).await.unwrap_err().to_string();
		assert!(error.contains("Connection refused"));
		Ok(())
	}

	#[test]
	fn display_works() {
		let mut command = TestCreateSnapshotCommand::default();
		command.from.uri = Some(DEFAULT_REMOTE_NODE_URL.to_string());
		command.snapshot_path = Some(DEFAULT_SNAPSHOT_PATH.to_string());
		command.skip_confirm = true;
		assert_eq!(
			command.display(),
			format!(
				"pop test create-snapshot --uri={} -y {}",
				DEFAULT_REMOTE_NODE_URL, DEFAULT_SNAPSHOT_PATH
			)
		);
	}

	#[test]
	fn collect_arguments_works() {
		let expected_uri = &format!("--uri={}", DEFAULT_REMOTE_NODE_URL);
		let test_cases: Vec<(&str, Box<dyn Fn(&mut TestCreateSnapshotCommand)>, &str)> = vec![
			(
				"--uri=ws://localhost:8545",
				Box::new(|cmd| cmd.from.uri = Some(DEFAULT_REMOTE_NODE_URL.to_string())),
				expected_uri,
			),
			(
				"predefined-example.snap",
				Box::new(|cmd| cmd.snapshot_path = Some(DEFAULT_SNAPSHOT_PATH.to_string())),
				DEFAULT_SNAPSHOT_PATH,
			),
			("--skip-confirm", Box::new(|cmd| cmd.skip_confirm = true), "-y"),
			("-y", Box::new(|cmd| cmd.skip_confirm = true), "-y"),
		];
		for (provided_arg, update_fn, expected_arg) in test_cases {
			let mut command = TestCreateSnapshotCommand::default();
			// Keep the user-provided argument unchanged.
			let args = command.collect_arguments(&[provided_arg.to_string()]);
			assert_eq!(args.iter().filter(|a| a.contains(&provided_arg.to_string())).count(), 1);

			// If there exists an argument with the same name as the provided argument, skip it.
			let args = command.collect_arguments(&args);
			assert_eq!(args.iter().filter(|a| a.contains(&provided_arg.to_string())).count(), 1);

			// If the user does not provide an argument, modify with the argument updated during
			// runtime.
			update_fn(&mut command);
			let args = command.collect_arguments(&[]);
			assert_eq!(args.iter().filter(|a| a.contains(&expected_arg.to_string())).count(), 1);
		}

		let mut command = TestCreateSnapshotCommand::default();
		command.from.uri = Some(DEFAULT_REMOTE_NODE_URL.to_string());
		assert_eq!(
			command.collect_arguments(&["--skip-confirm".to_string(), "example.snap".to_string(),]),
			vec![&format!("--uri={}", DEFAULT_REMOTE_NODE_URL), "--skip-confirm", "example.snap"]
		);
		command.skip_confirm = true;
		assert_eq!(
			command.collect_arguments(&["example.snap".to_string(),]),
			vec![&format!("--uri={}", DEFAULT_REMOTE_NODE_URL), "-y", "example.snap"]
		);
		assert_eq!(
			command.collect_arguments(&[
				"--skip-confirm".to_string(),
				"--uri".to_string(),
				DEFAULT_REMOTE_NODE_URL.to_string(),
				"example.snap".to_string(),
			]),
			vec!["--skip-confirm", &format!("--uri={}", DEFAULT_REMOTE_NODE_URL), "example.snap"]
		);
		assert_eq!(
			command.collect_arguments(&[
				format!("--uri={}", DEFAULT_REMOTE_NODE_URL),
				"--skip-confirm".to_string(),
				"example.snap".to_string(),
			]),
			vec![&format!("--uri={}", DEFAULT_REMOTE_NODE_URL), "--skip-confirm", "example.snap"]
		);
		command.skip_confirm = true;
	}
}
