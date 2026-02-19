// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, Cli, traits::*},
	output::{CliResponse, OutputMode, PromptRequiredError},
};
use Dependencies::*;
use anyhow::Context;
use clap::Args;
use duct::cmd;
use os_info::Type;
use serde::Serialize;
use std::{collections::HashMap, fs::Permissions, os::unix::fs::PermissionsExt, path::Path};
use strum_macros::Display;
use tokio::fs;

/// Utilities for installing the needed libraries for frontend development.
pub mod frontend;

const DOCS_URL: &str = "https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/";

#[derive(Display)]
pub enum Dependencies {
	#[strum(serialize = "clang")]
	Clang,
	#[strum(serialize = "cmake")]
	Cmake,
	#[strum(serialize = "curl")]
	Curl,
	#[strum(serialize = "git")]
	Git,
	#[strum(serialize = "homebrew")]
	Homebrew,
	#[strum(serialize = "libclang-dev")]
	LibClang,
	#[strum(serialize = "libssl-dev")]
	Libssl,
	#[strum(serialize = "libudev-dev")]
	LibUdevDev,
	#[strum(serialize = "llvm")]
	Llvm,
	#[strum(serialize = "make")]
	Make,
	#[strum(serialize = "openssl")]
	Openssl,
	#[strum(serialize = "openssl-devel")]
	OpenSslDevel,
	#[strum(serialize = "protobuf")]
	Protobuf,
	#[strum(serialize = "protobuf-compiler")]
	ProtobufCompiler,
	#[strum(serialize = "rustup")]
	Rustup,
	#[strum(serialize = "unzip")]
	Unzip,
	#[strum(serialize = "lsof")]
	Lsof,
	#[strum(serialize = "pkg-config")]
	PkgConfig,
}

/// Arguments for installing.
#[derive(Args, Serialize)]
#[cfg_attr(test, derive(Default))]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct InstallArgs {
	/// Automatically install all dependencies required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
	/// Install frontend development dependencies.
	#[clap(short = 'f', long)]
	frontend: bool,
}

/// Structured output for JSON mode.
#[derive(Serialize)]
struct InstallOutput {
	os: String,
}

/// Setup user environment for development.
pub(crate) struct Command;

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(
		self,
		args: &InstallArgs,
		output_mode: OutputMode,
	) -> anyhow::Result<()> {
		match output_mode {
			OutputMode::Human => self.execute_inner(args, &mut Cli).await,
			OutputMode::Json => {
				if !args.skip_confirm {
					return Err(PromptRequiredError(
						"-y/--skip-confirm is required with --json".into(),
					)
					.into());
				}
				let mut cli = crate::cli::JsonCli;
				self.execute_inner(args, &mut cli).await?;
				let os = detect_os_label();
				CliResponse::ok(InstallOutput { os }).print_json();
				Ok(())
			},
		}
	}

	async fn execute_inner(
		self,
		args: &InstallArgs,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<()> {
		cli.intro("Install dependencies for development")?;
		if cfg!(target_os = "macos") {
			cli.info("ℹ️ Mac OS (Darwin) detected.")?;
			install_mac(args.skip_confirm, cli).await?;
		} else if cfg!(target_os = "linux") {
			let os_type = os_info::get().os_type();
			let distro_type = match os_type {
				Type::Arch | Type::Debian | Type::Redhat | Type::Ubuntu => Some(os_type),
				_ => get_compatible_distro(),
			};

			match distro_type {
				Some(Type::Arch) => {
					cli.info("ℹ️ Arch Linux (or compatible) detected.")?;
					install_arch(args.skip_confirm, args.frontend, cli).await?;
				},
				Some(Type::Debian) => {
					cli.info("ℹ️ Debian Linux (or compatible) detected.")?;
					install_debian(args.skip_confirm, args.frontend, cli).await?;
				},
				Some(Type::Redhat) => {
					cli.info("ℹ️ Redhat Linux (or compatible) detected.")?;
					install_redhat(args.skip_confirm, cli).await?;
				},
				Some(Type::Ubuntu) => {
					cli.info("ℹ️ Ubuntu (or compatible) detected.")?;
					install_ubuntu(args.skip_confirm, args.frontend, cli).await?;
				},
				_ => not_supported_message(cli)?,
			}
		} else {
			return not_supported_message(cli);
		};
		install_rustup(cli).await?;

		if args.frontend {
			frontend::install_frontend_dependencies(args.skip_confirm, cli).await?;
		}
		cli.outro("✅ Installation complete.")?;
		Ok(())
	}
}

fn detect_os_label() -> String {
	if cfg!(target_os = "macos") {
		"macos".into()
	} else if cfg!(target_os = "linux") {
		let os_type = os_info::get().os_type();
		match os_type {
			Type::Arch => "arch".into(),
			Type::Debian => "debian".into(),
			Type::Redhat => "redhat".into(),
			Type::Ubuntu => "ubuntu".into(),
			_ => match get_compatible_distro() {
				Some(Type::Arch) => "arch".into(),
				Some(Type::Debian) => "debian".into(),
				Some(Type::Redhat) => "redhat".into(),
				Some(Type::Ubuntu) => "ubuntu".into(),
				_ => "unsupported".into(),
			},
		}
	} else {
		"unsupported".into()
	}
}

fn wrap_sudo_required_error(error: std::io::Error) -> anyhow::Error {
	anyhow::anyhow!(
		"An error occurred while installing components. If superuser privileges are needed run `sudo $(which pop) install`\n\n {}",
		error
	)
}

/// Parse `os-release` to get distribution information.
// source: https://www.freedesktop.org/software/systemd/man/latest/os-release.html
fn parse_os_release() -> anyhow::Result<HashMap<String, String>> {
	// Find the first path that exists.
	let path = ["/etc/os-release", "/usr/lib/os-release"]
		.iter()
		.map(Path::new)
		.find(|p| p.exists());

	let Some(path) = path else {
		return Ok(HashMap::new());
	};

	let content = std::fs::read_to_string(path)?;
	let mut map = HashMap::new();

	for line in content.lines() {
		// Skip empty lines and comments.
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}

		// Parse KEY=VALUE or KEY="VALUE" format.
		if let Some((key, value)) = line.split_once('=') {
			let value = value.trim_matches('"').trim_matches('\'');
			map.insert(key.to_string(), value.to_string());
		}
	}

	Ok(map)
}

/// Check if the distribution is compatible with a known distribution based on ID_LIKE.
fn get_compatible_distro() -> Option<Type> {
	let os_release = parse_os_release().ok()?;
	let id_like = os_release.get("ID_LIKE")?;

	// Check ID_LIKE for compatible distributions.
	// Split by space as ID_LIKE can contain multiple values (e.g., "ubuntu debian").
	for distro in id_like.split_whitespace() {
		match distro.to_lowercase().as_str() {
			"ubuntu" => return Some(Type::Ubuntu),
			"debian" => return Some(Type::Debian),
			"arch" => return Some(Type::Arch),
			"rhel" | "fedora" | "redhat" => return Some(Type::Redhat),
			_ => continue,
		}
	}

	None
}

async fn install_mac(skip_confirm: bool, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
	cli.info(format!(
		"More information about the packages to be installed here: {DOCS_URL}#macos"
	))?;
	if !skip_confirm {
		prompt_for_confirmation(
			&format!("{}, {}, {}, {} and {}", Homebrew, Protobuf, Openssl, Rustup, Cmake),
			cli,
		)?
	}
	install_homebrew(cli).await?;
	cmd("brew", vec!["update"]).run().map_err(wrap_sudo_required_error)?;
	cmd("brew", vec!["install", &Protobuf.to_string(), &Openssl.to_string(), &Cmake.to_string()])
		.run()
		.map_err(wrap_sudo_required_error)?;

	Ok(())
}

async fn install_arch(
	skip_confirm: bool,
	install_frontend: bool,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	cli.info(format!(
		"More information about the packages to be installed here: {DOCS_URL}#linux"
	))?;

	let mut packages = vec![
		Curl.to_string(),
		Git.to_string(),
		Clang.to_string(),
		Make.to_string(),
		Protobuf.to_string(),
	];

	let mut package_names = format!("{}, {}, {}, {}, {}", Curl, Git, Clang, Make, Protobuf);

	if install_frontend {
		packages.push(Unzip.to_string());
		package_names.push_str(&format!(", {}", Unzip));
	}

	package_names.push_str(&format!(" and {}", Rustup));

	if !skip_confirm {
		prompt_for_confirmation(&package_names, cli)?
	}

	let mut args = vec!["-Syu", "--needed", "--noconfirm"];
	args.extend(packages.iter().map(|s| s.as_str()));

	cmd("pacman", args).run().map_err(wrap_sudo_required_error)?;

	Ok(())
}

async fn install_ubuntu(
	skip_confirm: bool,
	install_frontend: bool,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	cli.info(format!(
		"More information about the packages to be installed here: {DOCS_URL}#linux"
	))?;

	let mut packages = vec![
		Git.to_string(),
		Clang.to_string(),
		Curl.to_string(),
		Libssl.to_string(),
		ProtobufCompiler.to_string(),
		Lsof.to_string(),
		PkgConfig.to_string(),
	];

	let mut package_names = format!(
		"{}, {}, {}, {}, {}, {}, {}",
		Git, Clang, Curl, Libssl, ProtobufCompiler, Lsof, PkgConfig
	);

	if install_frontend {
		packages.push(Unzip.to_string());
		package_names.push_str(&format!(", {}", Unzip));
	}

	package_names.push_str(&format!(" and {}", Rustup));

	if !skip_confirm {
		prompt_for_confirmation(&package_names, cli)?
	}

	let mut args = vec!["install", "--assume-yes"];
	args.extend(packages.iter().map(|s| s.as_str()));

	cmd("apt", args)
		.env("DEBIAN_FRONTEND", "noninteractive")
		.run()
		.map_err(wrap_sudo_required_error)?;

	Ok(())
}

async fn install_debian(
	skip_confirm: bool,
	install_frontend: bool,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	cli.info(format!(
		"More information about the packages to be installed here: {DOCS_URL}#linux"
	))?;

	let mut packages = vec![
		Libssl.to_string(),
		Git.to_string(),
		ProtobufCompiler.to_string(),
		Lsof.to_string(),
		Clang.to_string(),
		LibClang.to_string(),
		Curl.to_string(),
		Llvm.to_string(),
		LibUdevDev.to_string(),
		Make.to_string(),
	];

	let mut package_names = format!(
		"{}, {}, {}, {}, {}, {}, {}, {}, {}",
		Git, Clang, Curl, Libssl, Llvm, LibUdevDev, Make, ProtobufCompiler, Lsof
	);

	if install_frontend {
		packages.push(Unzip.to_string());
		package_names.push_str(&format!(", {}", Unzip));
	}

	package_names.push_str(&format!(" and {}", Rustup));

	if !skip_confirm {
		prompt_for_confirmation(&package_names, cli)?
	}

	let mut args = vec!["install", "-y"];
	args.extend(packages.iter().map(|s| s.as_str()));

	cmd("apt", args)
		.env("DEBIAN_FRONTEND", "noninteractive")
		.run()
		.map_err(wrap_sudo_required_error)?;

	Ok(())
}

async fn install_redhat(skip_confirm: bool, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
	cli.info(format!(
		"More information about the packages to be installed here: {DOCS_URL}#linux"
	))?;
	if !skip_confirm {
		prompt_for_confirmation(
			&format!(
				"{}, {}, {}, {}, {}, {}, {} and {}",
				Clang, Curl, Git, OpenSslDevel, Make, ProtobufCompiler, Lsof, Rustup,
			),
			cli,
		)?
	}
	cmd("yum", vec!["update", "-y"]).run().map_err(wrap_sudo_required_error)?;
	// NOTE: in many RedHad distributions we cannot run `yum groupinstall -y "Development Tools"`.
	// We install here the most important packages from that group.
	cmd(
		"yum",
		vec!["install", "-y", "gcc", "gcc-c++", "make", "cmake", "pkgconf", "pkgconf-pkg-config"],
	)
	.run()
	.map_err(wrap_sudo_required_error)?;
	cmd(
		"yum",
		vec![
			"install",
			"-y",
			&Clang.to_string(),
			&Curl.to_string(),
			&Git.to_string(),
			&ProtobufCompiler.to_string(),
			&Lsof.to_string(),
			&OpenSslDevel.to_string(),
			&Make.to_string(),
		],
	)
	.run()
	.map_err(wrap_sudo_required_error)?;

	Ok(())
}

fn prompt_for_confirmation(message: &str, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
	if !cli
		.confirm(format!(
			"📦 Do you want to proceed with the installation of the following packages: {}?",
			message
		))
		.initial_value(true)
		.interact()?
	{
		return Err(anyhow::anyhow!("🚫 You have cancelled the installation process."));
	}
	Ok(())
}

fn not_supported_message(cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
	cli.error("This OS is not supported at present")?;
	cli.warning(format!("⚠️ Please refer to {} for setup information.", DOCS_URL))?;
	Ok(())
}

async fn install_rustup(cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
	let rustup = match cmd("which", vec!["rustup"]).read() {
		Ok(output) => {
			cli.info(format!("ℹ️ rustup installed already at {}.", output))?;
			cmd("rustup", vec!["update"]).run()?;
			"rustup".to_string()
		},
		Err(_) => {
			cli.info("Installing rustup...")?;
			run_external_script("https://sh.rustup.rs", &["-y"]).await?;
			cli.outro("rustup installed!")?;
			let home = std::env::var("HOME")?;
			format!("{home}/.cargo/bin/rustup")
		},
	};
	cmd(&rustup, vec!["default", "stable"]).run()?;
	cmd(&rustup, vec!["update"]).run()?;
	cmd(&rustup, vec!["target", "add", "wasm32-unknown-unknown"]).run()?;
	cmd(
		&rustup,
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

async fn install_homebrew(cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
	match cmd("which", vec!["brew"]).read() {
		Ok(output) => cli.info(format!("ℹ️ Homebrew installed already at {}.", output))?,
		Err(_) =>
			run_external_script(
				"https://raw.githubusercontent.com/Homebrew/install/master/install.sh",
				&[],
			)
			.await?,
	}
	Ok(())
}

pub(crate) async fn run_external_script(script_url: &str, args: &[&str]) -> anyhow::Result<()> {
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
	fs::set_permissions(scripts_path.as_path(), Permissions::from_mode(0o755)).await?;
	cmd(scripts_path, args).run()?;
	temp.close()?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::io::Write;

	#[tokio::test]
	async fn install_mac_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_info("More information about the packages to be installed here: https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/#macos").expect_confirm("📦 Do you want to proceed with the installation of the following packages: homebrew, protobuf, openssl, rustup and cmake?", false);
		assert!(matches!(
			install_mac(false, &mut cli)
				.await,
			anyhow::Result::Err(message) if message.to_string() == "🚫 You have cancelled the installation process."
		));
		cli.verify()
	}
	#[tokio::test]
	async fn install_arch_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_info("More information about the packages to be installed here: https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/#linux").expect_confirm("📦 Do you want to proceed with the installation of the following packages: curl, git, clang, make, protobuf and rustup?", false);
		assert!(matches!(
			install_arch(false, false, &mut cli)
				.await,
			anyhow::Result::Err(message) if message.to_string() == "🚫 You have cancelled the installation process."
		));
		cli.verify()
	}
	#[tokio::test]
	async fn install_ubuntu_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_info("More information about the packages to be installed here: https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/#linux").expect_confirm("📦 Do you want to proceed with the installation of the following packages: git, clang, curl, libssl-dev, protobuf-compiler, lsof, pkg-config and rustup?", false);
		assert!(matches!(
			install_ubuntu(false, false, &mut cli)
				.await,
			anyhow::Result::Err(message) if message.to_string() == "🚫 You have cancelled the installation process."
		));
		cli.verify()
	}
	#[tokio::test]
	async fn install_debian_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_info("More information about the packages to be installed here: https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/#linux").expect_confirm("📦 Do you want to proceed with the installation of the following packages: git, clang, curl, libssl-dev, llvm, libudev-dev, make, protobuf-compiler, lsof and rustup?", false);
		assert!(matches!(
			install_debian(false, false, &mut cli)
				.await,
			anyhow::Result::Err(message) if message.to_string() == "🚫 You have cancelled the installation process."
		));
		cli.verify()
	}
	#[tokio::test]
	async fn install_redhat_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_info("More information about the packages to be installed here: https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/#linux").expect_confirm("📦 Do you want to proceed with the installation of the following packages: clang, curl, git, openssl-devel, make, protobuf-compiler, lsof and rustup?", false);
		assert!(matches!(
			install_redhat(false, &mut cli)
				.await,
			anyhow::Result::Err(message) if message.to_string() == "🚫 You have cancelled the installation process."
		));
		cli.verify()
	}

	#[tokio::test]
	async fn prompt_for_confirmation_works() -> anyhow::Result<()> {
		let deps = "test1, test2";
		let mut cli = MockCli::new().expect_confirm(
			format!(
				"📦 Do you want to proceed with the installation of the following packages: {}?",
				deps
			),
			true,
		);
		prompt_for_confirmation(deps, &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn not_supported_message_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new()
			.expect_error("This OS is not supported at present")
			.expect_warning(
				"⚠️ Please refer to https://docs.polkadot.com/develop/parachains/install-polkadot-sdk/ for setup information.",
			);
		not_supported_message(&mut cli)?;
		cli.verify()
	}

	#[test]
	fn test_parse_os_release() -> anyhow::Result<()> {
		// Create a temporary os-release file.
		let temp_dir = tempfile::tempdir()?;
		let os_release_path = temp_dir.path().join("os-release");
		let mut file = std::fs::File::create(&os_release_path)?;
		writeln!(file, "ID=tuxedo")?;
		writeln!(file, "ID_LIKE=\"ubuntu debian\"")?;
		writeln!(file, "NAME=\"Tuxedo OS\"")?;
		writeln!(file, "# This is a comment")?;
		writeln!(file)?;
		writeln!(file, "VERSION_ID='22.04'")?;
		drop(file);

		// Read and parse the file.
		let content = std::fs::read_to_string(&os_release_path)?;
		let mut map = HashMap::new();

		for line in content.lines() {
			let line = line.trim();
			if line.is_empty() || line.starts_with('#') {
				continue;
			}
			if let Some((key, value)) = line.split_once('=') {
				let value = value.trim_matches('"').trim_matches('\'');
				map.insert(key.to_string(), value.to_string());
			}
		}

		assert_eq!(map.get("ID"), Some(&"tuxedo".to_string()));
		assert_eq!(map.get("ID_LIKE"), Some(&"ubuntu debian".to_string()));
		assert_eq!(map.get("NAME"), Some(&"Tuxedo OS".to_string()));
		assert_eq!(map.get("VERSION_ID"), Some(&"22.04".to_string()));
		assert_eq!(map.len(), 4);

		Ok(())
	}

	#[test]
	fn test_id_like_parsing() {
		// Test Ubuntu-like.
		let id_like = "ubuntu debian";
		let mut found = None;
		for distro in id_like.split_whitespace() {
			match distro.to_lowercase().as_str() {
				"ubuntu" => {
					found = Some(Type::Ubuntu);
					break;
				},
				"debian" => {
					found = Some(Type::Debian);
					break;
				},
				_ => continue,
			}
		}
		assert_eq!(found, Some(Type::Ubuntu));

		// Test Debian-like.
		let id_like = "debian";
		let mut found = None;
		for distro in id_like.split_whitespace() {
			match distro.to_lowercase().as_str() {
				"ubuntu" => {
					found = Some(Type::Ubuntu);
					break;
				},
				"debian" => {
					found = Some(Type::Debian);
					break;
				},
				_ => continue,
			}
		}
		assert_eq!(found, Some(Type::Debian));

		// Test RHEL-like.
		let id_like = "rhel fedora";
		let mut found = None;
		for distro in id_like.split_whitespace() {
			match distro.to_lowercase().as_str() {
				"rhel" | "fedora" | "redhat" => {
					found = Some(Type::Redhat);
					break;
				},
				_ => continue,
			}
		}
		assert_eq!(found, Some(Type::Redhat));
	}
}
