// Copyright (C) R0GUE IO LTD.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "parachain")]
mod parachain;

use clap::{Args, Subcommand};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	#[command(subcommand)]
	pub(crate) command: UpCommands,
}

#[derive(Subcommand)]
pub(crate) enum UpCommands {
	#[cfg(feature = "parachain")]
	/// Deploy a parachain to a network.
	#[clap(alias = "p")]
	Parachain(parachain::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// Deploy a smart contract to a node.
	#[clap(alias = "c")]
	Contract(contract::UpContractCommand),
}
