// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "parachain")]
/// Contains benchmarking utilities.
pub mod bench;
/// Contains utilities for sourcing binaries.
pub mod binary;
pub mod builds;
#[cfg(feature = "parachain")]
pub mod chain;
#[cfg(feature = "contract")]
pub mod contracts;
pub mod helpers;
/// Contains utilities for interacting with the CLI prompt.
pub mod prompt;
/// Contains runtime utilities.
pub mod runtime;
/// Contains try-runtime utilities.
#[cfg(feature = "parachain")]
pub mod try_runtime;
pub mod wallet;

use std::fmt::{Display, Formatter, Result};
use strum::VariantArray;

#[derive(Debug, PartialEq)]
pub enum Telemetry {
	Null,
	Build(Project),
	Test { project: Project, feature: Feature },
	Install(Os),
	Up(Project),
	New(Template),
}

/// Represents the type of project being operated on.
#[derive(Debug, PartialEq, Clone, VariantArray)]
pub enum Project {
	Network,
	/// A blockchain project (parachain, etc.).
	Chain,
	/// A smart contract project.
	Contract,
	/// Unknown project type.
	Unknown,
}

#[derive(Debug, PartialEq)]
pub enum Template {
	Chain(pop_parachains::Parachain),
	Contract(pop_contracts::Contract),
	Pallet,
}

#[derive(Debug, PartialEq, Clone, VariantArray)]
pub enum Feature {
	Unit,
	E2e,
}

#[derive(Debug, PartialEq, Clone, VariantArray)]
pub enum Os {
	Mac,
	Linux,
	Unsupported,
}

// Display the telemetry in a human-readable format while excluding the command name to prevent
// double display.
impl Display for Telemetry {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use strum::EnumMessage;
		use Telemetry::*;
		use Template::*;

		match self {
			Null => write!(f, "null"),
			Build(project) => write!(f, "{}", project),
			Test { project, feature } => write!(f, "{} {}", project, feature),
			Install(os) => write!(f, "{}", os),
			Up(project) => write!(f, "{}", project),
			New(template) => {
				match template {
					// Chain(chain) => write!(f, "{}", chain.get_str("Message").unwrap_or("")),
					Chain(chain) => write!(f, "{}", chain.get_message().unwrap_or("")),
					Contract(contract) => write!(f, "{}", contract.get_message().unwrap_or("")),
					Pallet => write!(f, "pallet"),
				}
			},
		}
	}
}

impl Display for Project {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Project::*;
		match self {
			Network => write!(f, "network"),
			Chain => write!(f, "chain"),
			Contract => write!(f, "contract"),
			Unknown => write!(f, "unknown"),
		}
	}
}

impl Display for Template {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Template::*;
		match self {
			Chain(chain) => write!(f, "{}", chain),
			Contract(contract) => write!(f, "{}", contract),
			Pallet => write!(f, "pallet"),
		}
	}
}
impl Display for Os {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Os::*;
		match self {
			Mac => write!(f, "mac"),
			Linux => write!(f, "linux"),
			Unsupported => write!(f, "unsupported"),
		}
	}
}

impl Display for Feature {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Feature::*;
		match self {
			Unit => write!(f, "unit"),
			E2e => write!(f, "e2e"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use strum::VariantArray;

	#[test]
	fn telemetry_display_works() {
		// Null.
		assert_eq!(Telemetry::Null.to_string(), "null");

		// Build.
		for project in Project::VARIANTS {
			let telemetry = Telemetry::Build(project.clone());
			assert_eq!(telemetry.to_string(), project.to_string());
		}

		// Test.
		for project in Project::VARIANTS {
			for feature in Feature::VARIANTS {
				let telemetry =
					Telemetry::Test { project: project.clone(), feature: feature.clone() };
				assert_eq!(telemetry.to_string(), format!("{} {}", project, feature));
			}
		}

		// Install.
		for os in Os::VARIANTS {
			let telemetry = Telemetry::Install(os.clone());
			assert_eq!(telemetry.to_string(), os.to_string());
		}

		// Up.
		for project in Project::VARIANTS {
			let telemetry = Telemetry::Up(project.clone());
			assert_eq!(telemetry.to_string(), project.to_string());
		}

		// New.
		assert_eq!(Telemetry::New(Template::Pallet).to_string(), "pallet");

		assert_eq!(
			Telemetry::New(Template::Chain(pop_parachains::Parachain::Contracts)).to_string(),
			"Contracts"
		);
		assert_eq!(
			Telemetry::New(Template::Contract(pop_contracts::Contract::ERC20)).to_string(),
			"Erc20"
		);
	}

	#[test]
	fn project_display_works() {
		for project in Project::VARIANTS {
			let expected = match project {
				Project::Network => "network",
				Project::Chain => "chain",
				Project::Contract => "contract",
				Project::Unknown => "unknown",
			};
			assert_eq!(project.to_string(), expected);
		}
	}

	#[test]
	fn feature_display_works() {
		for feature in Feature::VARIANTS {
			let expected = match feature {
				Feature::Unit => "unit",
				Feature::E2e => "e2e",
			};
			assert_eq!(feature.to_string(), expected);
		}
	}

	#[test]
	fn os_display_works() {
		for os in Os::VARIANTS {
			let expected = match os {
				Os::Mac => "mac",
				Os::Linux => "linux",
				Os::Unsupported => "unsupported",
			};
			assert_eq!(os.to_string(), expected);
		}
	}

	#[test]
	fn template_display_works() {
		assert_eq!(Template::Pallet.to_string(), "pallet");
		// Test Chain variant with all Parachain types.
		for parachain in pop_parachains::Parachain::VARIANTS {
			let template = Template::Chain(parachain.clone());
			assert_eq!(template.to_string(), parachain.to_string());
		}
		// Test Contract variant with all Contract types.
		for contract in pop_contracts::Contract::VARIANTS {
			let template = Template::Contract(contract.clone());
			assert_eq!(template.to_string(), contract.to_string());
		}
	}
}
