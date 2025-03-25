use crate::{
	cli::{
		self,
		traits::{Input, Select},
	},
	commands::test::TestTryRuntimeCommand,
	common::{
		builds::guide_user_to_select_profile, runtime::ensure_runtime_binary_exists,
		try_runtime::check_try_runtime_and_prompt,
	},
};
use clap::Args;
use pop_common::Profile;
use pop_parachains::{generate_try_runtime, Migration, TryRuntimeCliCommand};
use std::{env::current_dir, path::PathBuf};
use strum::{EnumMessage, VariantArray};
use try_runtime_core::{
	commands::on_runtime_upgrade::Command,
	common::{
		shared_parameters::Runtime,
		state::{LiveState, State},
	},
};

#[derive(Args)]
pub(crate) struct TestOnRuntimeUpgradeCommand {
	/// Command to test runtime migrations.
	#[clap(flatten)]
	command: TestTryRuntimeCommand<Command>,
	/// Build profile.
	#[clap(long, value_enum)]
	profile: Option<Profile>,
	/// Migration mode to run with.
	#[clap(long, value_enum, alias = "m")]
	migration: Option<Migration>,
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
			self.profile = Some(guide_user_to_select_profile(cli)?);
		};
		self.command.shared_params.runtime = Runtime::Path(ensure_runtime_binary_exists(
			cli,
			&current_dir().unwrap_or(PathBuf::from("./")),
			self.profile.as_ref().ok_or_else(|| anyhow::anyhow!("No profile provided"))?,
			!self.no_build,
		)?);

		// Select the migration to run.
		if self.migration.is_none() {
			self.migration = Some(guide_user_to_select_migration_mode(cli)?.clone());
		}

		match self.migration()? {
			Migration::Live => {
				let uri =
					cli.input("Enter the live chain of your node").required(true).interact()?;
				self.command.command.state = State::Live(LiveState {
					uri,
					at: None,
					pallet: vec![],
					hashed_prefixes: vec![],
					child_tree: false,
				});
			},
			Migration::Snapshot => {
				let path = cli.input("Enter your snapshot file").required(true).interact()?;
				self.command.command.state = State::Snap { path: Some(path.into()) };
			},
		}

		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		generate_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::OnRuntimeUpgrade,
			|args| args,
			&[],
		);
		Ok(())
	}

	fn migration(&self) -> anyhow::Result<&Migration> {
		self.migration
			.as_ref()
			.ok_or_else(|| anyhow::anyhow!("No migration mode provided"))
	}
}

fn guide_user_to_select_migration_mode(
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<&Migration> {
	let mut prompt = cli.select("Run the migrations:");
	for migration in Migration::VARIANTS.iter() {
		prompt = prompt.item(
			migration,
			migration.get_message().unwrap(),
			migration.get_detailed_message().unwrap(),
		);
	}
	prompt.interact().map_err(anyhow::Error::from)
}
