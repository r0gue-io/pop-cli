// SPDX-License-Identifier: GPL-3.0

use anyhow::{Result, anyhow};
use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::{Shell, generate};
use serde::Serialize;
use std::{
	fs::{File, create_dir_all},
	io,
	io::Write,
	path::{Path, PathBuf},
};

use crate::cli::{
	Cli,
	traits::{Confirm as _, Input as _, Select as _},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CompletionShell {
	#[value(name = "bash")]
	Bash,
	#[value(name = "zsh")]
	Zsh,
	#[value(name = "fish")]
	Fish,
	#[value(name = "powershell")]
	PowerShell,
	#[value(name = "elvish")]
	Elvish,
}

impl From<CompletionShell> for Shell {
	fn from(shell: CompletionShell) -> Self {
		match shell {
			CompletionShell::Bash => Shell::Bash,
			CompletionShell::Zsh => Shell::Zsh,
			CompletionShell::Fish => Shell::Fish,
			CompletionShell::PowerShell => Shell::PowerShell,
			CompletionShell::Elvish => Shell::Elvish,
		}
	}
}

#[derive(Args, Serialize)]
pub(crate) struct CompletionArgs {
	/// Shell type to generate completions for.
	#[clap(value_enum)]
	pub(crate) shell: Option<CompletionShell>,
	/// Write completions to a file instead of stdout.
	#[clap(short, long)]
	pub(crate) output: Option<PathBuf>,
}

pub(crate) struct Command;

impl Command {
	pub(crate) fn execute(args: &CompletionArgs) -> Result<()> {
		match (args.shell, args.output.as_ref()) {
			(Some(shell), None) => generate_completion(shell, &mut io::stdout()),
			(Some(shell), Some(path)) => {
				write_completion_file(shell, path)?;
				print_post_install(shell, path);
				Ok(())
			},
			(None, _) => interactive_setup(&mut Cli),
		}
	}
}

fn generate_completion(shell: CompletionShell, writer: &mut dyn Write) -> Result<()> {
	let mut cmd = crate::Cli::command();
	let generator: Shell = shell.into();
	generate(generator, &mut cmd, "pop", writer);
	Ok(())
}

fn write_completion_file(shell: CompletionShell, path: &Path) -> Result<()> {
	if let Some(parent) = path.parent() {
		create_dir_all(parent)?;
	}
	let mut file = File::create(path)?;
	generate_completion(shell, &mut file)
}

fn default_completion_path(shell: CompletionShell, home: &Path) -> Option<PathBuf> {
	match shell {
		CompletionShell::Bash => Some(home.join(".local/share/bash-completion/completions/pop")),
		CompletionShell::Zsh => Some(home.join(".zsh/completions/_pop")),
		CompletionShell::Fish => Some(home.join(".config/fish/completions/pop.fish")),
		CompletionShell::PowerShell => Some(home.join(".config/powershell/Completions/pop.ps1")),
		CompletionShell::Elvish => Some(home.join(".config/elvish/lib/pop.elv")),
	}
}

fn interactive_setup(cli: &mut impl crate::cli::traits::Cli) -> Result<()> {
	cli.intro("Shell completion setup")?;

	let shell = {
		let mut prompt = cli.select("Select your shell:");
		prompt = prompt
			.item(CompletionShell::Zsh, "zsh", "Recommended for macOS")
			.item(CompletionShell::Bash, "bash", "Bash completions")
			.item(CompletionShell::Fish, "fish", "Fish completions")
			.item(CompletionShell::PowerShell, "powershell", "PowerShell completions")
			.item(CompletionShell::Elvish, "elvish", "Elvish completions");
		prompt.interact()?
	};

	let home = dirs::home_dir().ok_or_else(|| anyhow!("home directory not found"))?;
	let default_path = default_completion_path(shell, &home).unwrap_or_else(|| home.join(".pop"));
	let default_path = default_path.to_string_lossy().to_string();

	let path_input = cli
		.input("Where should I save the completion file?")
		.default_input(&default_path)
		.required(true)
		.interact()?;
	let path = PathBuf::from(path_input.trim());
	if path.as_os_str().is_empty() {
		return Err(anyhow!("completion output path cannot be empty"));
	}

	let confirmed = cli
		.confirm(format!("Write completion script to {}?", path.display()))
		.initial_value(true)
		.interact()?;
	if !confirmed {
		cli.outro_cancel("Aborted completion setup.")?;
		return Ok(());
	}

	write_completion_file(shell, &path)?;
	cli.success("Completion script written.")?;

	let steps = post_install_steps(shell, &path);
	cli.plain(steps)?;
	cli.outro("Completion setup complete.")?;
	Ok(())
}

fn print_post_install(shell: CompletionShell, path: &Path) {
	let steps = post_install_steps(shell, path);
	eprintln!("{steps}");
}

fn post_install_steps(shell: CompletionShell, path: &Path) -> String {
	let path_display = path.display();
	match shell {
		CompletionShell::Zsh => format!(
			"Next steps (commands):\n  fpath=({} $fpath)\n  autoload -Uz compinit && compinit\n\nNotes:\n  Restart your shell",
			path.parent().map(|p| p.display()).unwrap_or(path_display)
		),
		CompletionShell::Bash => format!(
			"Next steps (commands):\n  source {}\n\nNotes:\n  Add the line above to ~/.bashrc\n  Restart your shell",
			path_display
		),
		CompletionShell::Fish => format!(
			"Next steps:\n  Restart your shell\n\nNotes:\n  Fish will auto-load completions from {}",
			path_display
		),
		CompletionShell::PowerShell => format!(
			"Next steps:\n  Restart your shell\n\nNotes:\n  Ensure your profile loads {}",
			path_display
		),
		CompletionShell::Elvish => format!(
			"Next steps:\n  Restart your shell\n\nNotes:\n  Ensure your config loads {}",
			path_display
		),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn completion_output_is_generated() {
		let mut buffer = Vec::new();
		generate_completion(CompletionShell::Bash, &mut buffer).unwrap();
		assert!(!buffer.is_empty());
	}

	#[test]
	fn default_paths_match_expected_layouts() {
		let home = Path::new("/home/testuser");
		assert_eq!(
			default_completion_path(CompletionShell::Zsh, home).unwrap(),
			home.join(".zsh/completions/_pop")
		);
		assert_eq!(
			default_completion_path(CompletionShell::Bash, home).unwrap(),
			home.join(".local/share/bash-completion/completions/pop")
		);
		assert_eq!(
			default_completion_path(CompletionShell::Fish, home).unwrap(),
			home.join(".config/fish/completions/pop.fish")
		);
	}
}
