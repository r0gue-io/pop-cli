// SPDX-License-Identifier: GPL-3.0

use anyhow::Context;
use clap::Args;
use cliclack::{confirm, log, outro};
use duct::cmd;
use os_info::Type;
use tokio::{fs, process::Command};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
/// Setup user environment for substrate development
pub(crate) struct InstallArgs {
	/// Before install all the dependencies needed, do not ask the user for confirmation.
	#[clap(short('y'), long)]
	skip_confirm: bool,
}

impl InstallArgs {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
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
		log::success("✅ Installation complete.")?;
		Ok(())
	}
}

async fn install_mac(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/macos/")?;
	if !skip_confirm {
		prompt_for_confirmation("Homebrew, protobuf, openssl, rustup, and cmake")?
	}
	install_homebrew().await?;
	cmd("brew", vec!["update"]).run()?;
	cmd("brew", vec!["install", "protobuf", "openssl", "cmake"]).run()?;

	Ok(())
}

async fn install_arch(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation("cmake, gcc, openssl-1.0, pkgconf, git, clang")?
	}
	cmd(
		"sudo",
		vec![
			"pacman",
			"-Syu",
			"--needed",
			"--noconfirm",
			"cmake",
			"gcc",
			"openssl-1.0",
			"pkgconf",
			"git",
			"clang",
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
		prompt_for_confirmation("git, clang, curl, libssl-dev, protobuf-compiler")?
	}
	cmd("sudo", vec!["apt", "update"]).run()?;
	cmd(
		"sudo",
		vec!["apt", "install", "-y", "git", "clang", "curl", "libssl-dev", "protobuf-compiler"],
	)
	.run()?;

	Ok(())
}

async fn install_debian(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation("cmake, pkg-config, libssl-dev, git, gcc, build-essential, git, protobuf-compiler, clang, libclang-dev")?
	}
	cmd("sudo", vec!["apt", "update"]).run()?;
	cmd(
		"sudo",
		vec![
			"apt",
			"install",
			"-y",
			"cmake",
			"pkg-config",
			"libssl-dev",
			"git",
			"gcc",
			"build-essential",
			"git",
			"protobuf-compiler",
			"clang",
			"libclang-dev",
		],
	)
	.run()?;

	Ok(())
}

async fn install_redhat(skip_confirm: bool) -> anyhow::Result<()> {
	log::info("More information about the packages to be installed here: https://docs.substrate.io/install/linux/")?;
	if !skip_confirm {
		prompt_for_confirmation("cmake, openssl-devel, git, protobuf, protobuf-compiler, clang, clang-devel and srustup")?;
	}
	cmd("sudo", vec!["yum", "update", "-y"]).run()?;
	cmd("sudo", vec!["yum", "groupinstall", "-y", "'Development Tool"]).run()?;
	cmd(
		"sudo",
		vec![
			"yum",
			"install",
			"-y",
			"cmake",
			"openssl-devel",
			"git",
			"protobuf",
			"protobuf-compiler",
			"clang",
			"clang-devel",
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
			let mut spinner = cliclack::spinner();
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
