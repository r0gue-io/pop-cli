// SPDX-License-Identifier: GPL-3.0

use crate::cli::{traits::Cli as _, Cli};
use clap::{error::ErrorKind, Args, Command};
use cliclack;
use pop_common::{
	capitalize_str, find_workspace_toml, format_dir,
	rust_writer::{self, types::*},
};
use proc_macro2::Span;
use std::{fs, path::PathBuf};
use syn::{parse_str, Ident, Type};

#[cfg(test)]
mod tests;

#[derive(Args, Debug, Clone)]
pub struct AddConfigTypeCommand {
	#[arg(short, long, required = true, help = "Specify the path to the pallet crate.")]
	pub(crate) path: PathBuf,
	#[arg(short, long, required = true, help = "The name of the config type.")]
	pub(crate) name: String,
	#[arg(short, long, num_args(1..), required = true, help="Add trait bounds to your new config type.")]
	pub(crate) bounds: Vec<String>,
	#[command(flatten)]
	pub(crate) default_config: DefaultConfigTypeOptions,
	#[arg(
		long,
		help = "Define a default value for your new config type to use in the default config."
	)]
	pub(crate) default_value: Option<String>,
	#[arg(long, help = "Define a value for your new config type to use in your runtime.")]
	pub(crate) runtime_value: Option<String>,
	#[arg(
		long,
		help = "If your pallet is included in a runtime, Pop-Cli will look for the impl block for your pallet's Config trait inside configs/mod.rs or lib.rs in the runtime crate by default in order to add the new type to the runtime. If your impl block is in another path, use this option to specify it."
	)]
	pub(crate) runtime_impl_path: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
#[group(required = false, multiple = false)]
pub struct DefaultConfigTypeOptions {
	#[arg(
		long,
		help = "Ensures the trait item will not be used as a default with the #[derive_impl(..)] attribute macro."
	)]
	pub(crate) no_default: bool,
	#[arg(
		long,
		help = "Ensures that the trait DefaultConfig will not have any bounds for this trait item."
	)]
	pub(crate) no_default_bounds: bool,
}

fn validate_config_options(command: &AddConfigTypeCommand, lib_content: &str) {
	let mut cmd = Command::new("");

	if command.runtime_impl_path.is_some() && command.runtime_value.is_none() {
		cmd.error(
			ErrorKind::ArgumentConflict,
			"The use of --runtime-impl-path is forbidden if --runtime-value isn't used.",
		)
		.exit()
	}

	match command.default_config {
        DefaultConfigTypeOptions { no_default: true, .. } if !lib_content.contains("pub mod config_preludes") => cmd.error(
            ErrorKind::InvalidSubcommand,
            "Cannot specify --no-default if the affected pallet doesn't implement a default config. Pop-Cli follows the convention and looks for a module called 'config_preludes' (defined either inside the pallet's lib file or its own file 'config_preludes.rs'), if your pallet uses a default config under another name, be sure to rename it as 'config_preludes' if you want to use this feature of Pop-Cli."
        ).exit(),
		DefaultConfigTypeOptions { no_default: true, .. } if command.default_value.is_some() => cmd
			.error(
				ErrorKind::ArgumentConflict,
				"Cannot specify a default value for a no-default config type.",
			)
			.exit(),
		DefaultConfigTypeOptions { no_default: true, .. } if command.runtime_value.is_none() => cmd
			.error(
				ErrorKind::ArgumentConflict,
				"Types without a default value need a runtime value.",
			)
			.exit(),
        DefaultConfigTypeOptions { no_default_bounds: true, .. } if !lib_content.contains("pub mod config_preludes") => cmd.error(
                ErrorKind::InvalidSubcommand,
                "Cannot specify --no-default-bounds if the affected pallet doesn't implement a default config. Pop-Cli follows the convention and looks for a module called 'config_preludes' (defined either inside the pallet's lib file or its own file 'config_preludes.rs'), if your pallet uses a default config under another name, be sure to rename it as 'config_preludes' if you want to use this feature of Pop-Cli."
            ).exit(),
		DefaultConfigTypeOptions { no_default_bounds: true, .. }
			if command.default_value.is_none() && command.runtime_value.is_none() =>
			cmd.error(
				ErrorKind::ArgumentConflict,
				"The type needs at least a default value or a runtime value.",
			)
			.exit(),
        DefaultConfigTypeOptions { no_default: false, no_default_bounds: false } if command.default_value.is_some() && !lib_content.contains("pub mod config_preludes") => cmd.error(
                ErrorKind::InvalidSubcommand,
                "Cannot specify a --default-value if the affected pallet doesn't implement a default config. Pop-Cli follows the convention and looks for a module called 'config_preludes' (defined either inside the pallet's lib file or its own file 'config_preludes.rs'), if your pallet uses a default config under another name, be sure to rename it as 'config_preludes' if you want to use this feature of Pop-Cli."
            ).exit(),
		DefaultConfigTypeOptions { no_default: false, no_default_bounds: false }
			if command.default_value.is_none() && command.runtime_value.is_none() =>
			cmd.error(
				ErrorKind::ArgumentConflict,
				"The type needs at least a default value or a runtime value.",
			)
			.exit(),
		_ => (),
	}
}

impl AddConfigTypeCommand {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Add a new type to your pallet")?;
		let mut cmd = Command::new("");
		let src = &self.path.join("src");
		// Check that the path correspond to a pallet using that the file lib.rs always contains the
		// line #[pallet::pallet].
		let lib_path = src.join("lib.rs");
		if !lib_path.is_file() {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure that the used path correspond to a pallet crate.",
			)
			.exit();
		}
		let lib_content = fs::read_to_string(&lib_path)?;
		if !lib_content.contains("#[pallet::pallet]") {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure that the used path correspond to a pallet crate.",
			)
			.exit();
		}

		// Check that the command is correct
		validate_config_options(&self, &lib_content);

		let spinner = cliclack::spinner();
		spinner.start("Updating pallet's config trait...");
		let type_name_ident = Ident::new(&capitalize_str(&self.name), Span::call_site());
		// Update the config trait in lib.rs
		rust_writer::update_config_trait(
			&lib_path,
			type_name_ident.clone(),
			self.bounds.iter().map(|bound| Ident::new(&bound, Span::call_site())).collect(),
			match &self.default_config {
				DefaultConfigTypeOptions { no_default: true, .. } => DefaultConfigType::NoDefault,
				DefaultConfigTypeOptions { no_default_bounds: true, .. } =>
					DefaultConfigType::NoDefaultBounds,
				_ => DefaultConfigType::Default,
			},
		)?;

		match &self.default_config {
			// No_default only adds the runtime value to runtimes
			DefaultConfigTypeOptions { no_default: true, .. } => {
				spinner.set_message("Adding your type to pallet's related runtimes...");
				// Add the new type to the mock runtime
				rust_writer::add_type_to_runtimes(
                    &self.path,
                    type_name_ident.clone(),
                    parse_str::<Type>(&self.runtime_value.expect("validate options stops the execution from clap if runtime_value is none in this scenario; qed;"))?,
                    self.runtime_impl_path.as_deref()
                )?;
			},
			// Otherwise, the type is added at least to one: the runtimes or the default
			// config
			_ => {
				if let Some(runtime_value) = &self.runtime_value {
					spinner.set_message("Adding your type to pallet's related runtimes...");
					// Add the new type to the mock runtime
					rust_writer::add_type_to_runtimes(
						&self.path,
						type_name_ident.clone(),
						parse_str::<Type>(&runtime_value)?,
						self.runtime_impl_path.as_deref(),
					)?;
				}
				if let Some(default_value) = &self.default_value {
					spinner.set_message(
						"Adding your type's default value to the pallet's config preludes...",
					);
					// If config_preludes is defined in its own file, we pass it to
					// 'add_type_to_config_preludes", otherwise we pass lib.rs
					let config_preludes_path = src.join("config_preludes.rs");
					let file_path = if config_preludes_path.is_file() {
						&config_preludes_path
					} else {
						&lib_path
					};

					rust_writer::add_type_to_config_preludes(
						file_path,
						type_name_ident,
						parse_str::<Type>(&default_value)?,
					)?;
				}
			},
		};

		if let Some(mut workspace_toml) = find_workspace_toml(&self.path) {
			workspace_toml.pop();
			format_dir(&workspace_toml)?;
		} else {
			format_dir(&self.path)?;
		}
		spinner.stop("Your type is ready to be used in your pallet 🚀");
		Ok(())
	}
}
