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

use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub(crate) mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CallArgs {
	#[command(subcommand)]
	pub command: CallCommands,
}

#[derive(Subcommand)]
pub(crate) enum CallCommands {
	/// Call a contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}
