// SPDX-License-Identifier: GPL-3.0

use crate::{cache, cli::Cli};

use clap::Subcommand;
use serde::Serialize;
use std::fmt::{Display, Formatter, Result};
#[cfg(feature = "chain")]
use up::network::Relay::*;

#[cfg(feature = "chain")]
pub(crate) mod bench;
pub(crate) mod build;
#[cfg(any(feature = "chain", feature = "contract"))]
pub(crate) mod call;
pub(crate) mod clean;
pub(crate) mod convert;
#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod fork;
pub(crate) mod hash;
#[cfg(any(feature = "chain", feature = "contract"))]
pub(crate) mod install;
#[cfg(any(feature = "chain", feature = "contract"))]
pub(crate) mod new;
pub(crate) mod test;
#[cfg(any(feature = "chain", feature = "contract"))]
pub(crate) mod up;
pub(crate) mod upgrade;

#[derive(Subcommand, Serialize)]
#[command(subcommand_required = true)]
pub(crate) enum Command {
	/// Set up the environment for development by installing required packages.
	#[clap(alias = "i")]
	#[cfg(any(feature = "chain", feature = "contract"))]
	Install(install::InstallArgs),
	/// Generate a new parachain, pallet or smart contract.
	#[clap(alias = "n")]
	#[cfg(any(feature = "chain", feature = "contract"))]
	New(new::NewArgs),
	/// Benchmark a pallet or parachain.
	#[cfg(feature = "chain")]
	Bench(bench::BenchmarkArgs),
	#[clap(alias = "b", about = about_build())]
	Build(build::BuildArgs),
	/// Call a chain or a smart contract.
	#[clap(alias = "c")]
	#[cfg(any(feature = "chain", feature = "contract"))]
	Call(call::CallArgs),
	#[clap(aliases = ["u", "deploy"], about = about_up())]
	#[cfg(any(feature = "chain", feature = "contract"))]
	Up(Box<up::UpArgs>),
	/// Upgrade the Polkadot SDK toolchain.
	#[clap(alias = "ug")]
	Upgrade(upgrade::UpgradeArgs),
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
	/// Create local forks of live chains or contracts.
	#[clap(alias = "f")]
	#[cfg(any(feature = "chain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
	Fork(fork::ForkArgs),
}

/// Help message for the build command.
fn about_build() -> &'static str {
	#[cfg(all(feature = "chain", feature = "contract"))]
	return "Build a parachain, chain specification, smart contract or Rust package.";
	#[cfg(all(feature = "chain", not(feature = "contract")))]
	return "Build a parachain, chain specification or Rust package.";
	#[cfg(all(feature = "contract", not(feature = "chain")))]
	return "Build a smart contract or Rust package.";
	#[cfg(all(not(feature = "contract"), not(feature = "chain")))]
	return "Build a Rust package.";
}

/// Help message for the `up` command.
#[cfg(any(feature = "contract", feature = "chain"))]
fn about_up() -> &'static str {
	#[cfg(all(feature = "chain", feature = "contract"))]
	return "Deploy a rollup(parachain), deploy a smart contract or launch a local network.";
	#[cfg(all(feature = "chain", not(feature = "contract")))]
	return "Deploy a rollup(parachain) or launch a local network.";
	#[cfg(all(feature = "contract", not(feature = "chain")))]
	return "Deploy a smart contract.";
}

impl Command {
	/// Executes the command.
	pub(crate) async fn execute(&mut self) -> anyhow::Result<()> {
		match self {
			#[cfg(any(feature = "chain", feature = "contract"))]
			Self::Install(args) => {
				env_logger::init();
				install::Command.execute(args).await
			},
			#[cfg(any(feature = "chain", feature = "contract"))]
			Self::New(args) => {
				env_logger::init();

				// If no command is provided, guide the user to select one interactively
				let command = match &mut args.command {
					Some(cmd) => cmd,
					None => &mut new::guide_user_to_select_command(&mut Cli)?,
				};

				match command {
					#[cfg(feature = "chain")]
					new::Command::Chain(cmd) => cmd.execute().await,
					#[cfg(feature = "chain")]
					new::Command::Pallet(cmd) => cmd.execute().await,
					#[cfg(feature = "contract")]
					new::Command::Contract(cmd) => cmd.execute().await,
				}
			},
			#[cfg(feature = "chain")]
			Self::Bench(args) => bench::Command::execute(args).await,
			Self::Build(args) => {
				env_logger::init();
				#[cfg(feature = "chain")]
				match &args.command {
					None => build::Command::execute(args),
					Some(cmd) => match cmd {
						#[cfg(feature = "chain")]
						build::Command::Spec(cmd) => cmd.execute().await,
					},
				}

				#[cfg(not(feature = "chain"))]
				build::Command::execute(args)
			},
			#[cfg(any(feature = "chain", feature = "contract"))]
			Self::Call(args) => {
				env_logger::init();
				match args.resolve_command()? {
					#[cfg(feature = "chain")]
					call::Command::Chain(cmd) => cmd.execute().await,
					#[cfg(feature = "contract")]
					call::Command::Contract(cmd) => cmd.execute(&mut Cli).await,
				}
			},
			#[cfg(any(feature = "chain", feature = "contract"))]
			Self::Up(args) => {
				env_logger::init();
				match &mut args.command {
					None => up::Command::execute(args).await,
					Some(cmd) => match cmd {
						#[cfg(feature = "chain")]
						up::Command::Network(cmd) => cmd.execute(&mut Cli).await,
						#[cfg(feature = "chain")]
						up::Command::Paseo(cmd) => cmd.execute(Paseo, &mut Cli).await,
						#[cfg(feature = "chain")]
						up::Command::Kusama(cmd) => cmd.execute(Kusama, &mut Cli).await,
						#[cfg(feature = "chain")]
						up::Command::Polkadot(cmd) => cmd.execute(Polkadot, &mut Cli).await,
						#[cfg(feature = "chain")]
						up::Command::Westend(cmd) => cmd.execute(Westend, &mut Cli).await,
						up::Command::Frontend(cmd) => cmd.execute(&mut Cli),
						#[cfg(feature = "contract")]
						up::Command::InkNode(cmd) => cmd.execute(&mut Cli).await,
						#[cfg(not(any(feature = "chain", feature = "contract")))]
						_ => Ok(()),
					},
				}
			},
			Self::Upgrade(args) => {
				env_logger::init();
				upgrade::Command::execute(args, &mut Cli).await
			},
			Self::Test(args) => {
				env_logger::init();

				#[cfg(any(feature = "contract", feature = "chain"))]
				match &mut args.command {
					None => test::Command::execute(args).await,
					Some(cmd) => match cmd {
						#[cfg(feature = "chain")]
						test::Command::OnRuntimeUpgrade(cmd) => cmd.execute(&mut Cli).await,
						#[cfg(feature = "chain")]
						test::Command::ExecuteBlock(cmd) => cmd.execute(&mut Cli).await,
						#[cfg(feature = "chain")]
						test::Command::CreateSnapshot(cmd) => cmd.execute(&mut Cli).await,
						#[cfg(feature = "chain")]
						test::Command::FastForward(cmd) => cmd.execute(&mut Cli).await,
						#[cfg(not(feature = "chain"))]
						_ => Ok(()),
					},
				}

				#[cfg(not(any(feature = "contract", feature = "chain")))]
				test::Command::execute(args).await
			},
			Self::Hash(args) => {
				env_logger::init();
				args.command.execute(&mut Cli)
			},
			Self::Clean(args) => {
				env_logger::init();
				match &args.command {
					clean::Command::Cache(cmd_args) => clean::CleanCacheCommand {
						cli: &mut Cli,
						cache: cache()?,
						all: cmd_args.all,
					}
					.execute(),
					clean::Command::Node(cmd_args) => clean::CleanNodesCommand {
						cli: &mut Cli,
						all: cmd_args.all,
						#[cfg(test)]
						list_nodes: None,
						#[cfg(test)]
						kill_fn: None,
					}
					.execute(),
				}
			},
			Command::Convert(args) => {
				env_logger::init();
				args.command.execute(&mut Cli)
			},
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Self::Fork(args) => {
				env_logger::init();
				match args.command {
					#[cfg(feature = "chain")]
					fork::Command::Chain(cmd) => cmd.execute().map(|_| Null),
					#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
					fork::Command::Contract(cmd) => cmd.execute().map(|_| Null),
				}
			},
		}
	}
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(any(feature = "chain", feature = "contract",))]
			Self::Install(_) => write!(f, "install"),
			#[cfg(any(feature = "chain", feature = "contract",))]
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
			#[cfg(any(feature = "chain", feature = "contract",))]
			Self::Call(args) => match &args.command {
				Some(cmd) => write!(f, "call {}", cmd),
				None => write!(f, "call unknown"),
			},
			#[cfg(any(feature = "chain", feature = "contract"))]
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
			#[cfg(any(
				feature = "chain",
				feature = "polkavm-contracts",
				feature = "wasm-contracts"
			))]
			Command::Fork(args) => write!(f, "fork {}", args.command),
			Command::Upgrade(_) => write!(f, "upgrade"),
		}
	}
}

#[cfg(test)]
#[cfg(all(feature = "chain", feature = "contract"))]
mod tests {
	use super::*;

	#[cfg(all(feature = "chain", feature = "contract"))]
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
					test: None,
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
			// Fork.
			(
				Command::Fork(fork::ForkArgs {
					command: fork::Command::Chain(Default::default()),
					endpoint: "localhost:9944".parse().unwrap(),
				}),
				"fork chain",
			),
			(
				Command::Fork(fork::ForkArgs {
					command: fork::Command::Contract(Default::default()),
					endpoint: "localhost:9944".parse().unwrap(),
				}),
				"fork contract",
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
		let command = Address { address: "0x742d35Cc6634C0532925a3b844Bc454e4438f44e".to_string() };
		assert_eq!(
			format!("convert {command}"),
			Command::Convert(ConvertArgs { command }).to_string()
		);
	}
}
