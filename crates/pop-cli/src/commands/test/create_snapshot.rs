use std::collections::HashSet;

use crate::{
	cli::{self, traits::Input},
	common::{
		prompt::display_message,
		try_runtime::{argument_exists, check_try_runtime_and_prompt, format_arg},
	},
};
use clap::Args;
use cliclack::spinner;
use console::style;
use pop_parachains::{generate_try_runtime, LiveState, TryRuntimeCliCommand};

const CUSTOM_ARGS: [&str; 2] = ["--skip-confirm", "-y"];

#[derive(Args)]
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
		cli.warning("NOTE: `create-snapshot` only works with existing runtime.")?;

		if self.from.uri.is_none() {
			self.from.uri = Some(
				cli.input(format!(
    				"Enter the URI of the remote node:\n{}",
    				style(
    					"Ensures your remote node is built with the `try-runtime` feature enabled. \
    					If not, you can run `pop build --try-runtime` to rebuild your node."
    				)
    				.dim()
    			))
				.required(true)
				.interact()?,
			);
		}
		if self.snapshot_path.is_none() {
			let input = cli
				.input(format!(
         			"Enter the path to write the snapshot to (optional):\n{}",
         			style("If not provided `<spec-name>-<spec-version>@<block-hash>.snap` will be used.").dim()
          		))
				.required(false)
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
		display_message("Snapshot is created successfully!", true, cli)?;
		Ok(())
	}

	async fn run(&self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let spinner = spinner();
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		if let Some(ref uri) = self.from.uri {
			spinner.start(format!(
				"Creating a snapshot of a remote node at {}...",
				console::style(&uri).magenta().underlined()
			));
		}
		generate_try_runtime(
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
		let mut seen_args: HashSet<String> = HashSet::new();
		let mut args = vec![];

		let arg = "--uri";
		if !argument_exists(user_provided_args, arg) {
			if let Some(ref uri) = self.from.uri {
				args.push(format_arg(arg, uri));
				seen_args.insert(arg.to_string());
			}
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
		// If the last argument is a snapshot path, remove it.
		if let Some(arg) = args.last() {
			if !arg.starts_with("--") && arg.ends_with(".snap") {
				args.pop();
			}
			if let Some(ref path) = self.snapshot_path {
				args.push(path.to_string());
			}
		}
		args
	}
}

#[cfg(test)]
mod tests {
	#[tokio::test]
	async fn test_create_snapshot_works() -> anyhow::Result<()> {
		Ok(())
	}

	#[test]
	fn display_works() {}

	#[test]
	fn collect_arguments_works() {}
}
