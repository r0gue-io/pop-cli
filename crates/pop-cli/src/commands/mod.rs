// SPDX-License-Identifier: GPL-3.0

use crate::{
	cache,
	cli::Cli,
	common::{
		builds::get_project_path,
		Data::{self, *},
		Project::{self, Network},
		Template::*,
	},
};
use clap::Subcommand;
use std::fmt::{Display, Formatter, Result};
#[cfg(feature = "parachain")]
pub(crate) mod bench;
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
	/// Benchmark a pallet or parachain.
	#[cfg(feature = "parachain")]
	Bench(bench::BenchmarkArgs),
	#[clap(alias = "b", about = about_build())]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Build(build::BuildArgs),
	/// Call a chain or a smart contract.
	#[clap(alias = "c")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Call(call::CallArgs),
	#[clap(aliases = ["u", "deploy"], about = about_up())]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Up(up::UpArgs),
	/// Test a Rust project.
	#[clap(alias = "t")]
	#[cfg(any(feature = "parachain", feature = "contract"))]
	Test(test::TestArgs),
	/// Remove generated/cached artifacts.
	#[clap(alias = "C")]
	Clean(clean::CleanArgs),
}

/// Help message for the build command.
fn about_build() -> &'static str {
	#[cfg(all(feature = "parachain", feature = "contract"))]
	return "Build a parachain, chain specification, smart contract or Rust package.";
	#[cfg(all(feature = "parachain", not(feature = "contract")))]
	return "Build a parachain, chain specification or Rust package.";
	#[cfg(all(feature = "contract", not(feature = "parachain")))]
	return "Build a smart contract or Rust package.";
}

/// Help message for the `up` command.
fn about_up() -> &'static str {
	#[cfg(all(feature = "parachain", feature = "contract"))]
	return "Deploy a rollup(parachain), deploy a smart contract or launch a local network.";
	#[cfg(all(feature = "parachain", not(feature = "contract")))]
	return "Deploy a rollup(parachain) or launch a local network.";
	#[cfg(all(feature = "contract", not(feature = "parachain")))]
	return "Deploy a smart contract.";
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<Data> {
		match self {
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Install(args) => {
				env_logger::init();
				install::Command.execute(args).await.map(Install)
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::New(args) => {
				env_logger::init();
				match args.command {
					#[cfg(feature = "parachain")]
					new::Command::Parachain(cmd) => cmd.execute().await.map(|p| New(Chain(p))),
					#[cfg(feature = "parachain")]
					new::Command::Pallet(cmd) => cmd.execute().await.map(|_| New(Pallet)),
					#[cfg(feature = "contract")]
					new::Command::Contract(cmd) => cmd.execute().await.map(|c| New(Contract(c))),
				}
			},
			#[cfg(feature = "parachain")]
			Self::Bench(args) => bench::Command::execute(args).await.map(|_| Null),
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Build(args) => {
				env_logger::init();
				match args.command {
					None => build::Command::execute(args).map(Build),
					Some(cmd) => match cmd {
						#[cfg(feature = "parachain")]
						build::Command::Spec(cmd) => cmd.execute().await.map(|_| Null),
					},
				}
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Call(args) => {
				env_logger::init();
				match args.command {
					#[cfg(feature = "parachain")]
					call::Command::Chain(cmd) => cmd.execute().await.map(|_| Null),
					#[cfg(feature = "contract")]
					call::Command::Contract(cmd) => cmd.execute().await.map(|_| Null),
				}
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Up(args) => {
				env_logger::init();
				match args.command {
					None => up::Command::execute(args).await.map(Up),
					Some(cmd) => match cmd {
						#[cfg(feature = "parachain")]
						up::Command::Network(mut cmd) => {
							cmd.valid = true;
							cmd.execute().await.map(|_| Up(Network))
						},
						// TODO: Deprecated, will be removed in v0.8.0.
						#[cfg(feature = "parachain")]
						up::Command::Parachain(cmd) => cmd.execute().await.map(|_| Null),
						// TODO: Deprecated, will be removed in v0.8.0.
						#[cfg(feature = "contract")]
						up::Command::Contract(mut cmd) => {
							cmd.path = get_project_path(args.path, args.path_pos);
							cmd.execute().await.map(|_| Null)
						},
					},
				}
			},
			Self::Test(args) => {
				env_logger::init();
				match args.command {
					None => test::Command::execute(args)
						.await
						.map(|(project, feature)| Test { project, feature }),
					Some(cmd) => match cmd {
						// TODO: Deprecated, will be removed in v0.8.0.
						#[cfg(feature = "contract")]
						test::Command::Contract(cmd) => cmd
							.execute(&mut Cli)
							.await
							.map(|feature| Test { project: Project::Contract, feature }),
						#[cfg(feature = "parachain")]
						test::Command::OnRuntimeUpgrade(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
						#[cfg(feature = "parachain")]
						test::Command::ExecuteBlock(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
						#[cfg(feature = "parachain")]
						test::Command::CreateSnapshot(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
						#[cfg(feature = "parachain")]
						test::Command::FastForward(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
					},
				}
			},
			Self::Clean(args) => {
				env_logger::init();
				match args.command {
					clean::Command::Cache(cmd_args) => {
						// Initialize command and execute
						clean::CleanCacheCommand {
							cli: &mut Cli,
							cache: cache()?,
							all: cmd_args.all,
						}
						.execute()
						.map(|_| Null)
					},
				}
			},
		}
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Install(_) => write!(f, "install"),
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::New(args) => write!(f, "new {}", args.command),
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Build(args) => match &args.command {
				Some(cmd) => write!(f, "build {}", cmd),
				None => write!(f, "build"),
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Call(args) => write!(f, "call {}", args.command),
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Up(args) => match &args.command {
				Some(cmd) => write!(f, "up {}", cmd),
				None => write!(f, "up"),
			},
			#[cfg(any(feature = "parachain", feature = "contract"))]
			Self::Test(args) => match &args.command {
				Some(cmd) => write!(f, "test {}", cmd),
				None => write!(f, "test"),
			},
			Self::Clean(_) => write!(f, "clean"),
			#[cfg(feature = "parachain")]
			Self::Bench(args) => write!(f, "bench {}", args.command),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[cfg(all(feature = "parachain", feature = "contract"))]
	#[test]
	fn command_display_works() {
		let test_cases = vec![
			// Install.
			(Command::Install(Default::default()), "install"),
			// Clean.
			(Command::Clean(Default::default()), "clean"),
			// Test.
			(Command::Test(test::TestArgs::default()), "test"),
			(
				Command::Test(test::TestArgs {
					command: Some(test::Command::Contract(Default::default())),
					..Default::default()
				}),
				"test contract",
			),
			(
				Command::Test(test::TestArgs {
					command: Some(test::Command::OnRuntimeUpgrade(Default::default())),
					..Default::default()
				}),
				"test on runtime upgrade",
			),
			(
				Command::Test(test::TestArgs {
					command: Some(test::Command::ExecuteBlock(Default::default())),
					..Default::default()
				}),
				"test execute block",
			),
			(
				Command::Test(test::TestArgs {
					command: Some(test::Command::CreateSnapshot(Default::default())),
					..Default::default()
				}),
				"test create snapshot",
			),
			(
				Command::Test(test::TestArgs {
					command: Some(test::Command::FastForward(Default::default())),
					..Default::default()
				}),
				"test fast forward",
			),
			// Build.
			(Command::Build(build::BuildArgs { command: None, ..Default::default() }), "build"),
			(
				Command::Build(build::BuildArgs {
					command: Some(build::Command::Spec(Default::default())),
					..Default::default()
				}),
				"build spec",
			),
			// Up.
			(Command::Up(up::UpArgs { command: None, ..Default::default() }), "up"),
			(
				Command::Up(up::UpArgs {
					command: Some(up::Command::Network(Default::default())),
					..Default::default()
				}),
				"up network",
			),
			(
				Command::Up(up::UpArgs {
					command: Some(up::Command::Parachain(Default::default())),
					..Default::default()
				}),
				"up chain",
			),
			(
				Command::Up(up::UpArgs {
					command: Some(up::Command::Contract(Default::default())),
					..Default::default()
				}),
				"up contract",
			),
			// Call.
			(
				Command::Call(call::CallArgs { command: call::Command::Chain(Default::default()) }),
				"call chain",
			),
			(
				Command::Call(call::CallArgs {
					command: call::Command::Contract(Default::default()),
				}),
				"call contract",
			),
			// New.
			(
				Command::New(new::NewArgs { command: new::Command::Parachain(Default::default()) }),
				"new chain",
			),
			(
				Command::New(new::NewArgs { command: new::Command::Pallet(Default::default()) }),
				"new pallet",
			),
			(
				Command::New(new::NewArgs { command: new::Command::Contract(Default::default()) }),
				"new contract",
			),
			// Bench.
			(
				Command::Bench(bench::BenchmarkArgs {
					command: bench::Command::Pallet(Default::default()),
				}),
				"bench pallet",
			),
		];

		for (command, expected) in test_cases {
			assert_eq!(command.to_string(), expected);
		}
	}
}
