// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "parachain")]
/// Contains benchmarking utilities.
pub mod bench;
/// Contains utilities for sourcing binaries.
pub mod binary;
pub mod builds;
#[cfg(feature = "parachain")]
pub mod chain;
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub mod contracts;
pub mod helpers;
/// Contains utilities for interacting with the CLI prompt.
pub mod prompt;
/// Contains runtime utilities.
#[cfg(feature = "parachain")]
pub mod runtime;
/// Contains try-runtime utilities.
#[cfg(feature = "parachain")]
pub mod try_runtime;
#[cfg(feature = "wallet-integration")]
pub mod wallet;

use std::fmt::{Display, Formatter, Result};
use strum::VariantArray;

/// Data returned after command execution.
#[derive(Debug, PartialEq)]
pub enum Data {
	/// Project that was built.
	Build(Project),
	/// Project and feature test details.
	Test {
		/// Project tested.
		project: Project,
		/// Test feature.
		feature: TestFeature,
	},
	/// Project that was started.
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "parachain"))]
	Up(Project),
	/// OS where installation occurred.
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "parachain"))]
	Install(Os),
	/// Template that was created.
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "parachain"))]
	New(Template),
	/// No additional data.
	Null,
}

/// Project type.
#[derive(Debug, PartialEq, Clone, VariantArray)]
pub enum Project {
	/// Smart contract.
	Contract,
	/// Chain.
	Chain,
	/// Network.
	Network,
	/// Unidentified project.
	Unknown,
}

/// Test feature.
#[derive(Debug, PartialEq, Clone, VariantArray)]
pub enum TestFeature {
	/// Unit tests.
	Unit,
	/// End-to-end tests.
	E2e,
}

/// Project templates.
#[derive(Debug, PartialEq, Clone)]
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "parachain"))]
pub enum Template {
	/// Smart contract template.
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	Contract(pop_contracts::Contract),
	/// Chain template.
	#[cfg(feature = "parachain")]
	Chain(pop_parachains::Parachain),
	/// Pallet template.
	#[cfg(feature = "parachain")]
	Pallet,
}

/// Supported operating systems.
#[derive(Debug, PartialEq, Clone, VariantArray)]
pub enum Os {
	/// Linux.
	Linux,
	/// macOS.
	Mac,
	/// Unsupported.
	Unsupported,
}

// Display the telemetry in a human-readable format while excluding the command name to prevent
// double display.
impl Display for Data {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Data::*;
		#[cfg(any(
			feature = "polkavm-contracts",
			feature = "wasm-contracts",
			feature = "parachain"
		))]
		use {strum::EnumMessage, Template::*};

		match self {
			Null => write!(f, ""),
			Build(project) => write!(f, "{}", project),
			Test { project, feature } => write!(f, "{} {}", project, feature),
			#[cfg(any(
				feature = "polkavm-contracts",
				feature = "wasm-contracts",
				feature = "parachain"
			))]
			Install(os) => write!(f, "{}", os),
			#[cfg(any(
				feature = "polkavm-contracts",
				feature = "wasm-contracts",
				feature = "parachain"
			))]
			Up(project) => write!(f, "{}", project),
			#[cfg(any(
				feature = "polkavm-contracts",
				feature = "wasm-contracts",
				feature = "parachain"
			))]
			New(template) => match template {
				#[cfg(feature = "parachain")]
				Chain(chain) => write!(f, "{}", chain.get_message().unwrap_or("")),
				#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
				Contract(contract) => write!(f, "{}", contract.get_message().unwrap_or("")),
				#[cfg(feature = "parachain")]
				Pallet => write!(f, "pallet"),
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

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts", feature = "parachain"))]
impl Display for Template {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use Template::*;
		match self {
			#[cfg(feature = "parachain")]
			Chain(chain) => write!(f, "{}", chain),
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			Contract(contract) => write!(f, "{}", contract),
			#[cfg(feature = "parachain")]
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

impl Display for TestFeature {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		use TestFeature::*;
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
		assert_eq!(Data::Null.to_string(), "");

		// Build.
		for project in Project::VARIANTS {
			let telemetry = Data::Build(project.clone());
			assert_eq!(telemetry.to_string(), project.to_string());
		}

		// Test.
		for project in Project::VARIANTS {
			for feature in TestFeature::VARIANTS {
				let telemetry = Data::Test { project: project.clone(), feature: feature.clone() };
				assert_eq!(telemetry.to_string(), format!("{} {}", project, feature));
			}
		}

		// Install.
		#[cfg(any(feature = "contract", feature = "parachain"))]
		for os in Os::VARIANTS {
			let telemetry = Data::Install(os.clone());
			assert_eq!(telemetry.to_string(), os.to_string());
		}

		// Up.
		#[cfg(any(feature = "contract", feature = "parachain"))]
		for project in Project::VARIANTS {
			let telemetry = Data::Up(project.clone());
			assert_eq!(telemetry.to_string(), project.to_string());
		}

		// New.
		#[cfg(feature = "parachain")]
		assert_eq!(Data::New(Template::Pallet).to_string(), "pallet");
		#[cfg(feature = "parachain")]
		assert_eq!(
			Data::New(Template::Chain(pop_parachains::Parachain::Contracts)).to_string(),
			"Contracts"
		);
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(
			Data::New(Template::Contract(pop_contracts::Contract::ERC20)).to_string(),
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
		for feature in TestFeature::VARIANTS {
			let expected = match feature {
				TestFeature::Unit => "unit",
				TestFeature::E2e => "e2e",
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
		#[cfg(feature = "parachain")]
		assert_eq!(Template::Pallet.to_string(), "pallet");
		// Test Chain variant with all Parachain types.
		#[cfg(feature = "parachain")]
		for parachain in pop_parachains::Parachain::VARIANTS {
			let template = Template::Chain(parachain.clone());
			assert_eq!(template.to_string(), parachain.to_string());
		}
		// Test Contract variant with all Contract types.
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		for contract in pop_contracts::Contract::VARIANTS {
			let template = Template::Contract(contract.clone());
			assert_eq!(template.to_string(), contract.to_string());
		}
	}
}
