use crate::{
	cli::{
		self,
		traits::{Input, Select},
	},
	commands::test::TestTryRuntimeCommand,
	common::{bench::ensure_runtime_binary_exists, builds::guide_user_to_select_profile},
};
use clap::Args;
use pop_common::Profile;
use pop_parachains::Migration;
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

		// TODO: run on runtime upgrade. Requires sourcing the binaries.
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
