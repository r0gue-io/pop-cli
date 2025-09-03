// SPDX-License-Identifier: GPL-3.0

use crate::{
	cache,
	cli::Cli,
	common::Data::{self, *},
};
use clap::Subcommand;
use std::fmt::{Display, Formatter, Result};
#[cfg(feature = "chain")]
use {crate::common::Project::Network, up::network::Relay::*};

#[cfg(feature = "chain")]
pub(crate) mod bench;
pub(crate) mod build;
#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod call;
pub(crate) mod clean;
pub(crate) mod convert;
pub(crate) mod hash;
#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod install;
#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod new;
pub(crate) mod test;
#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod up;

#[derive(Subcommand)]
#[command(subcommand_required = true)]
pub(crate) enum Command {
	/// Set up the environment for development by installing required packages.
	#[clap(alias = "i")]
	#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
	Install(install::InstallArgs),
	/// Generate a new parachain, pallet or smart contract.
	#[clap(alias = "n")]
	#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
	New(new::NewArgs),
	/// Benchmark a pallet or parachain.
	#[cfg(feature = "chain")]
	Bench(bench::BenchmarkArgs),
	#[clap(alias = "b", about = about_build())]
	Build(build::BuildArgs),
	/// Call a chain or a smart contract.
	#[clap(alias = "c")]
	#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
	Call(call::CallArgs),
	#[clap(aliases = ["u", "deploy"], about = about_up())]
	#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
	Up(Box<up::UpArgs>),
	/// Test a Rust project.
	#[clap(alias = "t")]
	Test(test::TestArgs),
	/// Hash data using a supported hash algorithm.
	#[clap(alias = "h")]
	Hash(hash::HashArgs),
	/// Remove generated/cached artifacts.
	#[clap(alias = "C")]
	Clean(clean::CleanArgs),
	/// Convert between different formats.
	#[clap(alias = "cv")]
	Convert(convert::ConvertArgs),
}

/// Help message for the build command.
fn about_build() -> &'static str {
	#[cfg(all(feature = "chain", any(feature = "polkavm-contracts", feature = "wasm-contracts")))]
	return "Build a parachain, chain specification, smart contract or Rust package.";
	#[cfg(all(
		feature = "chain",
		not(any(feature = "polkavm-contracts", feature = "wasm-contracts"))
	))]
	return "Build a parachain, chain specification or Rust package.";
	#[cfg(all(
		any(feature = "polkavm-contracts", feature = "wasm-contracts"),
		not(feature = "chain")
	))]
	return "Build a smart contract or Rust package.";
	#[cfg(all(
		not(feature = "polkavm-contracts"),
		not(feature = "wasm-contracts"),
		not(feature = "chain")
	))]
	return "Build a Rust package.";
}

/// Help message for the `up` command.
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "chain"))]
fn about_up() -> &'static str {
	#[cfg(all(feature = "chain", any(feature = "polkavm-contracts", feature = "wasm-contracts")))]
	return "Deploy a rollup(parachain), deploy a smart contract or launch a local network.";
	#[cfg(all(
		feature = "chain",
		not(any(feature = "polkavm-contracts", feature = "wasm-contracts"))
	))]
	return "Deploy a rollup(parachain) or launch a local network.";
	#[cfg(all(
		any(feature = "polkavm-contracts", feature = "wasm-contracts"),
		not(feature = "chain")
	))]
	return "Deploy a smart contract.";
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<Data> {
		match self {
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::Install(args) => {
				env_logger::init();
				install::Command.execute(args).await.map(Install)
			},
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::New(args) => {
				env_logger::init();
				use crate::common::Template::*;

				// If no command is provided, guide the user to select one interactively
				let command = match args.command {
					Some(cmd) => cmd,
					None => new::guide_user_to_select_command().await?,
				};

				match command {
					#[cfg(feature = "chain")]
					new::Command::Chain(cmd) => cmd.execute().await.map(|p| New(Chain(p))),
					#[cfg(feature = "chain")]
					new::Command::Pallet(cmd) => cmd.execute().await.map(|_| New(Pallet)),
					#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
					new::Command::Contract(cmd) => cmd.execute().await.map(|c| New(Contract(c))),
				}
			},
			#[cfg(feature = "chain")]
			Self::Bench(args) => bench::Command::execute(args).await.map(|_| Null),
			Self::Build(args) => {
				env_logger::init();
				#[cfg(feature = "chain")]
				match args.command {
					None => build::Command::execute(args).map(Build),
					Some(cmd) => match cmd {
						#[cfg(feature = "chain")]
						build::Command::Spec(cmd) => cmd.execute().await.map(|_| Null),
					},
				}

				#[cfg(not(feature = "chain"))]
				build::Command::execute(args).map(Build)
			},
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::Call(args) => {
				env_logger::init();
				match args.resolve_command()? {
					#[cfg(feature = "chain")]
					call::Command::Chain(cmd) => cmd.execute().await.map(|_| Null),
					#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
					call::Command::Contract(cmd) => cmd.execute().await.map(|_| Null),
				}
			},
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::Up(args) => {
				env_logger::init();
				match args.command {
					None => up::Command::execute(*args).await.map(Up),
					Some(cmd) => match cmd {
						#[cfg(feature = "chain")]
						up::Command::Network(cmd) => cmd.execute(&mut Cli).await.map(|_| Up(Network)),
						#[cfg(feature = "chain")]
						up::Command::Paseo(mut cmd) => cmd.execute(Paseo, &mut Cli).await.map(|_| Up(Network)),
						#[cfg(feature = "chain")]
						up::Command::Kusama(mut cmd) => cmd.execute(Kusama, &mut Cli).await.map(|_| Up(Network)),
						#[cfg(feature = "chain")]
						up::Command::Polkadot(mut cmd) => cmd.execute(Polkadot, &mut Cli).await.map(|_| Up(Network)),
						#[cfg(feature = "chain")]
						up::Command::Westend(mut cmd) => cmd.execute(Westend, &mut Cli).await.map(|_| Up(Network)),
					},
				}
			},
			Self::Test(args) => {
				env_logger::init();

				#[cfg(any(
					feature = "polkavm-contracts",
					feature = "wasm-contracts",
					feature = "chain"
				))]
				match args.command {
					None => test::Command::execute(args)
						.await
						.map(|(project, feature)| Test { project, feature }),
					Some(cmd) => match cmd {
						#[cfg(feature = "chain")]
						test::Command::OnRuntimeUpgrade(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
						#[cfg(feature = "chain")]
						test::Command::ExecuteBlock(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
						#[cfg(feature = "chain")]
						test::Command::CreateSnapshot(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
						#[cfg(feature = "chain")]
						test::Command::FastForward(cmd) => cmd.execute(&mut Cli).await.map(|_| Null),
					},
				}

				#[cfg(not(any(
					feature = "polkavm-contracts",
					feature = "wasm-contracts",
					feature = "chain"
				)))]
				test::Command::execute(args)
					.await
					.map(|(project, feature)| Test { project, feature })
			},
			Self::Hash(args) => {
				env_logger::init();
				args.command.execute(&mut Cli).map(|_| Null)
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
			Command::Convert(args) => {
				env_logger::init();
				args.command.execute(&mut Cli).map(|_| Null)
			},
		}
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::Install(_) => write!(f, "install"),
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::New(args) => match &args.command {
				Some(cmd) => write!(f, "new {}", cmd),
				None => write!(f, "new"),
			},
			#[allow(unused_variables)]
			Self::Build(args) => {
				#[cfg(feature = "chain")]
				match &args.command {
					Some(cmd) => write!(f, "build {}", cmd),
					None => write!(f, "build"),
				}

				#[cfg(not(feature = "chain"))]
				write!(f, "build")
			},
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::Call(args) => match &args.command {
				Some(cmd) => write!(f, "call {}", cmd),
				None => write!(f, "call unknown"),
			},
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			#[allow(unused_variables)]
			Self::Up(args) => {
				#[cfg(feature = "chain")]
				match &args.command {
					Some(cmd) => write!(f, "up {}", cmd),
					None => write!(f, "up"),
				}

				#[cfg(not(feature = "chain"))]
				write!(f, "up")
			},
			#[allow(unused_variables)]
			Self::Test(args) => {
				#[cfg(feature = "chain")]
				match &args.command {
					Some(cmd) => write!(f, "test {}", cmd),
					None => write!(f, "test"),
				}

				#[cfg(not(feature = "chain"))]
				write!(f, "test")
			},
			Self::Clean(_) => write!(f, "clean"),
			#[cfg(feature = "chain")]
			Self::Bench(args) => write!(f, "bench {}", args.command),
			Command::Hash(args) => write!(f, "hash {}", args.command),
			Command::Convert(args) => write!(f, "convert {}", args.command),
		}
	}
}

#[cfg(test)]
#[cfg(all(feature = "chain", feature = "wasm-contracts"))]
mod tests {
	use super::*;

	#[cfg(all(feature = "chain", feature = "wasm-contracts"))]
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
			(Command::Up(up::UpArgs { command: None, ..Default::default() }.into()), "up"),
			(
				Command::Up(
					up::UpArgs {
						command: Some(up::Command::Network(Default::default())),
						..Default::default()
					}
					.into(),
				),
				"up network",
			),
			// Call.
			(
				Command::Call(call::CallArgs {
					command: Some(call::Command::Chain(Default::default())),
				}),
				"call chain",
			),
			(
				Command::Call(call::CallArgs {
					command: Some(call::Command::Contract(Default::default())),
				}),
				"call contract",
			),
			// New.
			(
				Command::New(new::NewArgs {
					command: Some(new::Command::Chain(Default::default())),
				}),
				"new chain",
			),
			(
				Command::New(new::NewArgs {
					command: Some(new::Command::Pallet(Default::default())),
				}),
				"new pallet",
			),
			(
				Command::New(new::NewArgs {
					command: Some(new::Command::Contract(Default::default())),
				}),
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

	#[test]
	fn hash_command_display_works() {
		use hash::{Command::*, Data, HashArgs};
		let command = Blake2 { length: 256, data: Data::default(), concat: false };
		assert_eq!(format!("hash {command}"), Command::Hash(HashArgs { command }).to_string());
	}

	#[test]
	fn convert_command_display_works() {
		use convert::{Command::*, ConvertArgs};
		let command = Address {
			address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".to_string(),
			prefix: None,
		};
		assert_eq!(
			format!("convert {command}"),
			Command::Convert(ConvertArgs { command }).to_string()
		);
	}
}
