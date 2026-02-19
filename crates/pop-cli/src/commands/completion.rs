// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		Cli,
		traits::{Confirm as _, Input as _, Select as _},
	},
	output::{CliResponse, OutputMode, PromptRequiredError},
};
use anyhow::{Result, anyhow};
use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::{Shell, generate};
use serde::Serialize;
use std::{
	env,
	fs::{File, create_dir_all},
	io,
	io::{IsTerminal, Write},
	path::{Path, PathBuf},
};

/// Mirror of `clap_complete::Shell` with `Serialize` for telemetry.
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
	#[arg(value_enum, value_name = "SHELL", index = 1)]
	pub(crate) shell: Option<CompletionShell>,
	/// Shell type to generate completions for (flag form).
	#[arg(long = "shell", value_enum, value_name = "SHELL", conflicts_with = "shell")]
	pub(crate) shell_flag: Option<CompletionShell>,
	/// Write completions to a file instead of stdout.
	#[clap(short, long)]
	pub(crate) output: Option<PathBuf>,
}

/// Structured output for JSON mode.
#[derive(Serialize)]
struct CompletionOutput {
	shell: CompletionShell,
	path: Option<String>,
}

/// Entry point called from the command dispatcher.
pub(crate) fn execute(args: &CompletionArgs, output_mode: OutputMode) -> Result<()> {
	match output_mode {
		OutputMode::Human => Command::execute(args),
		OutputMode::Json => {
			let shell = args
				.shell
				.or(args.shell_flag)
				.ok_or_else(|| PromptRequiredError("--shell is required with --json".into()))?;
			match args.output.as_ref() {
				Some(path) => {
					write_completion_file(shell, path)?;
					CliResponse::ok(CompletionOutput {
						shell,
						path: Some(path.display().to_string()),
					})
					.print_json();
				},
				None => {
					// Generate to stdout would conflict with JSON envelope, so
					// we generate into a buffer but only report the shell used.
					let mut buf = Vec::new();
					generate_completion(shell, &mut buf)?;
					CliResponse::ok(CompletionOutput { shell, path: None }).print_json();
				},
			}
			Ok(())
		},
	}
}

pub(crate) struct Command;

impl Command {
	pub(crate) fn execute(args: &CompletionArgs) -> Result<()> {
		let shell = args.shell.or(args.shell_flag);
		match (shell, args.output.as_ref()) {
			(Some(shell), None) => generate_completion(shell, &mut io::stdout()),
			(Some(shell), Some(path)) => {
				write_completion_file(shell, path)?;
				print_post_install(shell, path);
				Ok(())
			},
			(None, Some(path)) =>
				if let Some((shell, shell_env)) = detect_shell_from_env() {
					eprintln!(
						"Detected shell from $SHELL ({}). If this is incorrect, rerun with --shell.",
						shell_env
					);
					write_completion_file(shell, path)?;
					print_post_install(shell, path);
					Ok(())
				} else if io::stdin().is_terminal() {
					interactive_setup(&mut Cli, Some(path))
				} else {
					Err(anyhow!("--output requires --shell when SHELL is not set"))
				},
			(None, None) => interactive_setup(&mut Cli, None),
		}
	}
}

fn generate_completion(shell: CompletionShell, writer: &mut dyn Write) -> Result<()> {
	let mut cmd = crate::Cli::command();
	let generator: Shell = shell.into();
	generate(generator, &mut cmd, "pop", writer);
	writer.flush()?;
	Ok(())
}

fn write_completion_file(shell: CompletionShell, path: &Path) -> Result<()> {
	if let Some(parent) = path.parent() {
		create_dir_all(parent)?;
	}
	let mut file = File::create(path)?;
	generate_completion(shell, &mut file)
}

fn shell_from_env_value(value: &str) -> Option<CompletionShell> {
	let value = value.to_ascii_lowercase();
	if value.contains("zsh") {
		Some(CompletionShell::Zsh)
	} else if value.contains("bash") {
		Some(CompletionShell::Bash)
	} else if value.contains("fish") {
		Some(CompletionShell::Fish)
	} else if value.contains("elvish") {
		Some(CompletionShell::Elvish)
	} else if value.contains("pwsh") || value.contains("powershell") {
		Some(CompletionShell::PowerShell)
	} else {
		None
	}
}

fn detect_shell_from_env() -> Option<(CompletionShell, String)> {
	let shell_env = env::var("SHELL").ok()?;
	let shell = shell_from_env_value(&shell_env)?;
	Some((shell, shell_env))
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

fn interactive_setup(
	cli: &mut impl crate::cli::traits::Cli,
	output_override: Option<&Path>,
) -> Result<()> {
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

	let path = if let Some(path) = output_override {
		path.to_path_buf()
	} else {
		let path_input = cli
			.input("Where should I save the completion file?")
			.default_input(&default_path)
			.required(true)
			.interact()?;
		PathBuf::from(path_input.trim())
	};
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

	#[test]
	fn json_mode_requires_shell_flag() {
		use crate::output::PromptRequiredError;
		let args = CompletionArgs { shell: None, shell_flag: None, output: None };
		let err = execute(&args, OutputMode::Json).unwrap_err();
		assert!(err.downcast_ref::<PromptRequiredError>().is_some());
		assert!(err.to_string().contains("--shell is required with --json"));
	}

	#[test]
	fn json_mode_produces_valid_envelope() {
		let args =
			CompletionArgs { shell: Some(CompletionShell::Bash), shell_flag: None, output: None };
		// Should succeed (generates to stdout via JSON envelope).
		execute(&args, OutputMode::Json).unwrap();

		// Verify envelope shape.
		let resp = CliResponse::ok(CompletionOutput { shell: CompletionShell::Bash, path: None });
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["schema_version"], 1);
		assert_eq!(json["success"], true);
		assert_eq!(json["data"]["shell"], "bash");
		assert!(json["data"]["path"].is_null());
		assert!(json.get("error").is_none());
	}

	#[test]
	fn json_mode_with_output_writes_file() {
		let tmp = tempfile::tempdir().unwrap();
		let output_path = tmp.path().join("pop.bash");
		let args = CompletionArgs {
			shell: Some(CompletionShell::Bash),
			shell_flag: None,
			output: Some(output_path.clone()),
		};
		execute(&args, OutputMode::Json).unwrap();
		assert!(output_path.exists());

		// Verify envelope includes the path.
		let resp = CliResponse::ok(CompletionOutput {
			shell: CompletionShell::Bash,
			path: Some(output_path.display().to_string()),
		});
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["data"]["shell"], "bash");
		assert!(json["data"]["path"].as_str().unwrap().contains("pop.bash"));
	}

	#[test]
	fn shell_from_env_value_detects_common_shells() {
		assert_eq!(shell_from_env_value("/bin/zsh"), Some(CompletionShell::Zsh));
		assert_eq!(shell_from_env_value("/usr/bin/bash"), Some(CompletionShell::Bash));
		assert_eq!(shell_from_env_value("fish"), Some(CompletionShell::Fish));
		assert_eq!(shell_from_env_value("pwsh.exe"), Some(CompletionShell::PowerShell));
		assert_eq!(shell_from_env_value("cmd.exe"), None);
	}
}
