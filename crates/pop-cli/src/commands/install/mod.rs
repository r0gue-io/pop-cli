// SPDX-License-Identifier: GPL-3.0
use crate::style::{style, Theme};
use anyhow::Context;
use clap::Args;
use cliclack::{clear_screen, confirm, intro, log, outro, set_theme};
use duct::cmd;
use os_info::Type;
use strum::Display;
use tokio::{fs, process::Command};

#[derive(Display)]
pub enum DEPENDENCIES {
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
	#[strum(serialize = "openssl-1.0")]
	Openssl1,
	#[strum(serialize = "pkg-config")]
	PkgConfig,
	#[strum(serialize = "protobuf")]
	Protobuf,
	#[strum(serialize = "protobuf-compiler")]
	ProtobufCompiler,
	#[strum(serialize = "rustup")]
	Rustup,
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
/// Setup user environment for development
pub(crate) struct InstallArgs {
	/// Automatically install all dependencies required without prompting for confirmation.
	#[clap(short('y'), long)]
	skip_confirm: bool,
}

impl InstallArgs {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		set_theme(Theme);
		intro(format!(
			"{}: Install dependencies for development",
			style(" Pop CLI ").black().on_magenta()
		))?;
		if cfg!(target_os = "macos") {
			log::info("ℹ️ Mac OS (Darwin) detected.")?;
			install_mac(self.skip_confirm).await?;
		} else if cfg!(target_os = "linux") {
			match os_info::get().os_type() {
				Type::Arch => {
					log::info("ℹ️ Arch Linux detected.")?;
					install_arch(self.skip_confirm).await?;
				},
				Type::Debian => {
					log::info("ℹ️ Debian Linux detected.")?;
					install_debian(self.skip_confirm).await?;
				},
				Type::Redhat => {
					log::info("ℹ️ Redhat Linux detected.")?;
					install_redhat(self.skip_confirm).await?;
				},
				Type::Ubuntu => {
					log::info("ℹ️ Ubuntu detected.")?;
					install_ubuntu(self.skip_confirm).await?;
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
			DEPENDENCIES::Homebrew,
			DEPENDENCIES::Protobuf,
			DEPENDENCIES::Openssl,
			DEPENDENCIES::Rustup,
			DEPENDENCIES::Cmake,
		))?
	}
	install_homebrew().await?;
	cmd("brew", vec!["update"]).run()?;
	cmd(
		"brew",
		vec![
			"install",
			&DEPENDENCIES::Protobuf.to_string(),
			&DEPENDENCIES::Openssl.to_string(),
			&DEPENDENCIES::Cmake.to_string(),
		],
	)
	.run()?;

	Ok(())
}

async fn install_arch(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {}, {}, {} and {}",
			DEPENDENCIES::Curl,
			DEPENDENCIES::Git,
			DEPENDENCIES::Clang,
			DEPENDENCIES::Make,
			DEPENDENCIES::Protobuf,
			DEPENDENCIES::Openssl1,
			DEPENDENCIES::Rustup,
		))?
	}
	cmd(
		"pacman",
		vec![
			"-Syu",
			"--needed",
			"--noconfirm",
			&DEPENDENCIES::Curl.to_string(),
			&DEPENDENCIES::Git.to_string(),
			&DEPENDENCIES::Clang.to_string(),
			&DEPENDENCIES::Make.to_string(),
			&DEPENDENCIES::Protobuf.to_string(),
			&DEPENDENCIES::Openssl1.to_string(),
		],
	)
	.run()?;
	cmd("export", vec!["OPENSSL_LIB_DIR='/usr/lib/openssl-1.0'"]).run()?;
	cmd("export", vec!["OPENSSL_INCLUDE_DIR='/usr/include/openssl-1.0'"]).run()?;

	Ok(())
}

async fn install_ubuntu(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation(&format!(
			"{}, {}, {}, {}, {} and {}",
			DEPENDENCIES::Git,
			DEPENDENCIES::Clang,
			DEPENDENCIES::Curl,
			DEPENDENCIES::Libssl,
			DEPENDENCIES::ProtobufCompiler,
			DEPENDENCIES::Rustup,
		))?
	}
	cmd(
		"apt",
		vec![
			"install",
			"--assume-yes",
			&DEPENDENCIES::Git.to_string(),
			&DEPENDENCIES::Clang.to_string(),
			&DEPENDENCIES::Curl.to_string(),
			&DEPENDENCIES::Libssl.to_string(),
			&DEPENDENCIES::ProtobufCompiler.to_string(),
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
			DEPENDENCIES::Cmake,
			DEPENDENCIES::PkgConfig,
			DEPENDENCIES::Libssl,
			DEPENDENCIES::Git,
			DEPENDENCIES::Gcc,
			DEPENDENCIES::BuildEssential,
			DEPENDENCIES::ProtobufCompiler,
			DEPENDENCIES::Clang,
			DEPENDENCIES::LibClang,
			DEPENDENCIES::Rustup,
		))?
	}
	cmd(
		"apt",
		vec![
			"install",
			"-y",
			&DEPENDENCIES::Cmake.to_string(),
			&DEPENDENCIES::PkgConfig.to_string(),
			&DEPENDENCIES::Libssl.to_string(),
			&DEPENDENCIES::Git.to_string(),
			&DEPENDENCIES::Gcc.to_string(),
			&DEPENDENCIES::BuildEssential.to_string(),
			&DEPENDENCIES::ProtobufCompiler.to_string(),
			&DEPENDENCIES::Clang.to_string(),
			&DEPENDENCIES::LibClang.to_string(),
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
			DEPENDENCIES::Cmake,
			DEPENDENCIES::OpenSslDevel,
			DEPENDENCIES::Git,
			DEPENDENCIES::Protobuf,
			DEPENDENCIES::ProtobufCompiler,
			DEPENDENCIES::Clang,
			DEPENDENCIES::ClangDevel,
			DEPENDENCIES::Rustup,
		))?
	}
	cmd("yum", vec!["update", "-y"]).run()?;
	cmd("yum", vec!["groupinstall", "-y", "'Development Tool"]).run()?;
	cmd(
		"yum",
		vec![
			"install",
			"-y",
			&DEPENDENCIES::Cmake.to_string(),
			&DEPENDENCIES::OpenSslDevel.to_string(),
			&DEPENDENCIES::Git.to_string(),
			&DEPENDENCIES::Protobuf.to_string(),
			&DEPENDENCIES::ProtobufCompiler.to_string(),
			&DEPENDENCIES::Clang.to_string(),
			&DEPENDENCIES::ClangDevel.to_string(),
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
			cmd("rustup", vec!["default", "stable"]).run()?;
		},
		Err(_) => {
			let spinner = cliclack::spinner();
			spinner.start("Installing rustup ...");
			run_external_script("https://sh.rustup.rs").await?;
			outro("rustup installed!")?;
			cmd(
				"source",
				vec![
					"~/.cargo/env
",
				],
			)
			.run()?;
			cmd("rustup", vec!["default", "stable"]).run()?;
		},
	}
	cmd("rustup", vec!["update", "nightly"]).run()?;
	cmd("rustup", vec!["target", "add", "wasm32-unknown-unknown", "--toolchain", "nightly"])
		.run()?;

	Ok(())
}

async fn install_homebrew() -> anyhow::Result<()> {
	match cmd("which", vec!["brew"]).read() {
		Ok(output) => log::info(format!("ℹ️ Homebrew installed already at {}.", output))?,
		Err(_) => {
			run_external_script(
				"https://raw.githubusercontent.com/Homebrew/install/master/install.sh",
			)
			.await?
		},
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
		.text()
		.await?;
	fs::write(scripts_path.as_path(), script).await?;
	Command::new("bash").arg(scripts_path).status().await?;
	temp.close()?;
	Ok(())
}
