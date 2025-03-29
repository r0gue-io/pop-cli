// SPDX-License-Identifier: GPL-3.0

use crate::style::{style, Theme};
use anyhow::Context;
use clap::Args;
use cliclack::{clear_screen, confirm, intro, log, outro, set_theme};
use duct::cmd;
use os_info::Type;
use strum_macros::Display;
use tokio::fs;
use Dependencies::*;

#[derive(Display)]
pub enum Dependencies {
	#[strum(serialize = "build-essential")]
	BuildEssential,
	#[strum(serialize = "clang")]
	Clang,
	#[strum(serialize = "clang-devel")]
	ClangDevel,
	#[strum(serialize = "cmake")]
	Cmake,
	#[strum(serialize = "curl")]
	Curl,
	#[strum(serialize = "gcc")]
	Gcc,
	#[strum(serialize = "git")]
	Git,
	#[strum(serialize = "homebrew")]
	Homebrew,
	#[strum(serialize = "libclang-dev")]
	LibClang,
	#[strum(serialize = "libssl-dev")]
	Libssl,
	#[strum(serialize = "make")]
	Make,
	#[strum(serialize = "openssl")]
	Openssl,
	#[strum(serialize = "openssl-devel")]
	OpenSslDevel,
	#[strum(serialize = "pkg-config")]
	PkgConfig,
	#[strum(serialize = "protobuf")]
	Protobuf,
	#[strum(serialize = "protobuf-compiler")]
	ProtobufCompiler,
	#[strum(serialize = "rustup")]
	Rustup,
}

/// Arguments for installing.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct InstallArgs {
	/// Automatically install all dependencies required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

/// Setup user environment for development
pub(crate) struct Command;

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(self, args: InstallArgs) -> anyhow::Result<()> {
		clear_screen()?;
		set_theme(Theme);
		intro(format!(
			"{}: Install dependencies for development",
			style(" Pop CLI ").black().on_magenta()
		))?;
		if cfg!(target_os = "macos") {
			log::info("ℹ️ Mac OS (Darwin) detected.")?;
			install_mac(args.skip_confirm).await?;
		} else if cfg!(target_os = "linux") {
			match os_info::get().os_type() {
				Type::Arch => {
					log::info("ℹ️ Arch Linux detected.")?;
					install_arch(args.skip_confirm).await?;
				},
				Type::Debian => {
					log::info("ℹ️ Debian Linux detected.")?;
					install_debian(args.skip_confirm).await?;
				},
				Type::Redhat => {
					log::info("ℹ️ Redhat Linux detected.")?;
					install_redhat(args.skip_confirm).await?;
				},
				Type::Ubuntu => {
					log::info("ℹ️ Ubuntu detected.")?;
					install_ubuntu(args.skip_confirm).await?;
				},
				_ => return not_supported_message(),
			}
		} else {
			return not_supported_message();
		}
		install_rustup().await?;
		outro("✅ Installation complete.")?;
		Ok(())
	}
}

async fn install_mac(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/macos/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {} and {}",
			Dependencies::Homebrew,
			Dependencies::Protobuf,
			Dependencies::Openssl,
			Dependencies::Rustup,
			Dependencies::Cmake,
		))?
	}
	install_homebrew().await?;
	cmd("brew", vec!["update"]).run()?;
	cmd("brew", vec!["install", &Protobuf.to_string(), &Openssl.to_string(), &Cmake.to_string()])
		.run()?;

	Ok(())
}

async fn install_arch(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {}, {} and {}",
			Curl, Git, Clang, Make, Openssl, Rustup,
		))?
	}
	cmd(
		"pacman",
		vec![
			"-Syu",
			"--needed",
			"--noconfirm",
			&Curl.to_string(),
			&Git.to_string(),
			&Clang.to_string(),
			&Make.to_string(),
			&Openssl.to_string(),
		],
	)
	.run()?;

	Ok(())
}

async fn install_ubuntu(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {}, {} and {}",
			Git, Clang, Curl, Libssl, ProtobufCompiler, Rustup,
		))?
	}
	cmd(
		"apt",
		vec![
			"install",
			"--assume-yes",
			&Git.to_string(),
			&Clang.to_string(),
			&Curl.to_string(),
			&Libssl.to_string(),
			&ProtobufCompiler.to_string(),
		],
	)
	.run()?;

	Ok(())
}

async fn install_debian(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {}, {}, {}, {}, {}, {} and {}",
			Cmake,
			PkgConfig,
			Libssl,
			Git,
			Gcc,
			BuildEssential,
			ProtobufCompiler,
			Clang,
			LibClang,
			Rustup,
		))?
	}
	cmd(
		"apt",
		vec![
			"install",
			"-y",
			&Cmake.to_string(),
			&PkgConfig.to_string(),
			&Libssl.to_string(),
			&Git.to_string(),
			&Gcc.to_string(),
			&BuildEssential.to_string(),
			&ProtobufCompiler.to_string(),
			&Clang.to_string(),
			&LibClang.to_string(),
		],
	)
	.run()?;

	Ok(())
}

async fn install_redhat(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {}, {}, {}, {} and {}",
			Cmake, OpenSslDevel, Git, Protobuf, ProtobufCompiler, Clang, ClangDevel, Rustup,
		))?
	}
	cmd("yum", vec!["update", "-y"]).run()?;
	cmd("yum", vec!["groupinstall", "-y", "'Development Tool"]).run()?;
	cmd(
		"yum",
		vec![
			"install",
			"-y",
			&Cmake.to_string(),
			&OpenSslDevel.to_string(),
			&Git.to_string(),
			&Protobuf.to_string(),
			&ProtobufCompiler.to_string(),
			&Clang.to_string(),
			&ClangDevel.to_string(),
		],
	)
	.run()?;

	Ok(())
}

fn prompt_for_confirmation(message: &str) -> anyhow::Result<()> {
	if !confirm(format!(
		"📦 Do you want to proceed with the installation of the following packages: {} ?",
		message
	))
	.initial_value(true)
	.interact()?
	{
		return Err(anyhow::anyhow!("🚫 You have cancelled the installation process."));
	}
	Ok(())
}

fn not_supported_message() -> anyhow::Result<()> {
	log::error("This OS is not supported at present")?;
	log::warning("⚠️ Please refer to https://docs.substrate.io/install/ for setup information.")?;
	Ok(())
}

async fn install_rustup() -> anyhow::Result<()> {
	match cmd("which", vec!["rustup"]).read() {
		Ok(output) => {
			log::info(format!("ℹ️ rustup installed already at {}.", output))?;
			cmd("rustup", vec!["update"]).run()?;
		},
		Err(_) => {
			let spinner = cliclack::spinner();
			spinner.start("Installing rustup ...");
			run_external_script("https://sh.rustup.rs").await?;
			outro("rustup installed!")?;
			cmd("source", vec!["~/.cargo/env"]).run()?;
		},
	}
	cmd("rustup", vec!["default", "stable"]).run()?;
	cmd("rustup", vec!["target", "add", "wasm32-unknown-unknown"]).run()?;
	cmd("rustup", vec!["update", "nightly"]).run()?;
	cmd("rustup", vec!["target", "add", "wasm32-unknown-unknown", "--toolchain", "nightly"])
		.run()?;
	cmd(
		"rustup",
		vec![
			"component",
			"add",
			"cargo",
			"clippy",
			"rust-analyzer",
			"rust-src",
			"rust-std",
			"rustc",
			"rustfmt",
		],
	)
	.run()?;

	Ok(())
}

async fn install_homebrew() -> anyhow::Result<()> {
	match cmd("which", vec!["brew"]).read() {
		Ok(output) => log::info(format!("ℹ️ Homebrew installed already at {}.", output))?,
		Err(_) =>
			run_external_script(
				"https://raw.githubusercontent.com/Homebrew/install/master/install.sh",
			)
			.await?,
	}
	Ok(())
}

async fn run_external_script(script_url: &str) -> anyhow::Result<()> {
	let temp = tempfile::tempdir()?;
	let scripts_path = temp.path().join("install.sh");
	let client = reqwest::Client::new();
	let script = client
		.get(script_url)
		.send()
		.await
		.context("Network Error: Failed to fetch script from Github")?
		.error_for_status()?
		.text()
		.await?;
	fs::write(scripts_path.as_path(), script).await?;
	tokio::process::Command::new("bash").arg(scripts_path).status().await?;
	temp.close()?;
	Ok(())
}
