// SPDX-License-Identifier: GPL-3.0

use crate::{cache, cli::Cli};
use clap::Subcommand;
use pop_common::templates::Template;
use serde_json::{json, Value};

pub(crate) mod build;
pub(crate) mod call;
pub(crate) mod clean;
pub(crate) mod install;
pub(crate) mod new;
pub(crate) mod test;
pub(crate) mod up;

#[derive(Subcommand)]
#[command(subcommand_required = true)]
pub(crate) enum Command {
	/// Set up the environment for development by installing required packages.
	#[clap(alias = "i")]
	Install(install::InstallArgs),
	/// Generate a new parachain, pallet or smart contract.
	#[clap(alias = "n")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	New(new::NewArgs),
	#[clap(alias = "b", about = about_build())]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Build(build::BuildArgs),
	/// Call a parachain or a smart contract.
	#[clap(alias = "c")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Call(call::CallArgs),
	/// Launch a local network or deploy a smart contract.
	#[clap(alias = "u")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Up(up::UpArgs),
	/// Test a smart contract.
	#[clap(alias = "t")]
	#[cfg(feature = "contract")]
	Test(test::TestArgs),
	/// Remove generated/cached artifacts.
	#[clap(alias = "C")]
	Clean(clean::CleanArgs),
}

/// Help message for the build command.
fn about_build() -> &'static str {
	#[cfg(all(feature = "parachain", feature = "contract"))]
	return "Build a parachain, smart contract or Rust package.";
	#[cfg(all(feature = "parachain", not(feature = "contract")))]
	return "Build a parachain or Rust package.";
	#[cfg(all(feature = "contract", not(feature = "parachain")))]
	return "Build a smart contract or Rust package.";
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<Value> {
		match self {
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Install(args) => install::Command.execute(args).await.map(|_| Value::Null),
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::New(args) => match args.command {
				#[cfg(feature = "parachain")]
				new::Command::Parachain(cmd) => match cmd.execute().await {
					Ok(template) => {
						// telemetry should never cause a panic or early exit
						Ok(
							json!({template.template_type().unwrap_or("provider-missing"): template.name()}),
						)
					},
					Err(e) => Err(e),
				},
				#[cfg(feature = "parachain")]
				new::Command::Pallet(cmd) => {
					// When more contract selections are added the tel data will likely need to go
					// deeper in the stack
					cmd.execute().await.map(|_| json!("template"))
				},
				#[cfg(feature = "contract")]
				new::Command::Contract(cmd) => {
					// When more contract selections are added, the tel data will likely need to go
					// deeper in the stack
					cmd.execute().await.map(|_| json!("default"))
				},
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Build(args) => match args.command {
				None => build::Command::execute(args).map(|t| json!(t)),
				Some(cmd) => match cmd {
					#[cfg(feature = "parachain")]
					build::Command::Parachain(cmd) => cmd.execute().map(|_| Value::Null),
					#[cfg(feature = "contract")]
					build::Command::Contract(cmd) => cmd.execute().map(|_| Value::Null),
					#[cfg(feature = "parachain")]
					build::Command::Spec(cmd) => cmd.execute().await.map(|_| Value::Null),
				},
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Call(args) => match args.command {
				#[cfg(feature = "parachain")]
				call::Command::Parachain(cmd) => cmd.execute().await.map(|_| Value::Null),
				#[cfg(feature = "contract")]
				call::Command::Contract(cmd) => cmd.execute().await.map(|_| Value::Null),
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Up(args) => match args.command {
				#[cfg(feature = "parachain")]
				up::Command::Parachain(cmd) => cmd.execute().await.map(|_| Value::Null),
				#[cfg(feature = "contract")]
				up::Command::Contract(cmd) => cmd.execute().await.map(|_| Value::Null),
			},
			#[cfg(feature = "contract")]
			Self::Test(args) => match args.command {
				test::Command::Contract(cmd) => match cmd.execute().await {
					Ok(feature) => Ok(json!(feature)),
					Err(e) => Err(e),
				},
			},
			Self::Clean(args) => match args.command {
				clean::Command::Cache(cmd_args) => {
					// Initialize command and execute
					clean::CleanCacheCommand { cli: &mut Cli, cache: cache()?, all: cmd_args.all }
						.execute()
						.map(|_| Value::Null)
				},
			},
		}
	}
}
