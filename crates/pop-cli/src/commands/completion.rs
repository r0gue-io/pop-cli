// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::{Shell, generate};
use serde::Serialize;
use std::{io, io::Write};

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CompletionShell {
	#[value(rename = "bash")]
	Bash,
	#[value(rename = "zsh")]
	Zsh,
	#[value(rename = "fish")]
	Fish,
	#[value(rename = "powershell")]
	PowerShell,
	#[value(rename = "elvish")]
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
	pub(crate) shell: CompletionShell,
}

pub(crate) struct Command;

impl Command {
	pub(crate) fn execute(args: &CompletionArgs) -> Result<()> {
		generate_completion(args.shell, &mut io::stdout())
	}
}

fn generate_completion(shell: CompletionShell, writer: &mut dyn Write) -> Result<()> {
	let mut cmd = crate::Cli::command();
	generate(shell.into(), &mut cmd, "pop", writer);
	Ok(())
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
}
