// SPDX-License-Identifier: GPL-3.0

use super::{BuildArgs, Profile};
use crate::cli;
use pop_contracts::{BuildMode, MetadataSpec, Verbosity, build_smart_contract};
use std::path::PathBuf;

/// Configuration for building a smart contract.
pub struct BuildContract {
	/// Path of the contract project.
	pub(crate) path: PathBuf,
	/// Build profile: `true` for release mode, `false` for debug mode, `verifiable` for
	/// deterministic, release mode.
	pub(crate) build_mode: BuildMode,
	/// Which specification to use for contract metadata.
	pub(crate) metadata: Option<MetadataSpec>,
}

impl BuildContract {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	/// Builds a smart contract
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Building your contract")?;
		// Build contract.
		let build_result =
			build_smart_contract(&self.path, self.build_mode, Verbosity::Default, self.metadata)?;
		cli.success(build_result.display())?;
		cli.outro("Build completed successfully!")?;
		Ok("contract")
	}
}

/// Resolve the `BuildMode` to use in a contract build depending on the specified args
///
/// # Arguments
/// * `args` - The `BuildArgs` needed to resolve the `BuildMode`
pub(super) fn resolve_build_mode(args: &BuildArgs) -> BuildMode {
	match (&args.profile, &args.verifiable) {
		(Some(Profile::Release), false) | (Some(Profile::Production), false) => BuildMode::Release,
		(None, true) => BuildMode::Verifiable,
		(None, false) if args.release => BuildMode::Release,
		// Fallback to debug mode
		_ => BuildMode::Debug,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn resolve_build_mode_works() {
		// Profile::Release + verifiable=false -> Release
		assert_eq!(
			resolve_build_mode(&BuildArgs {
				profile: Some(Profile::Release),
				..Default::default()
			}),
			BuildMode::Release
		);
		// Profile::Production + verifiable=false -> Release
		assert_eq!(
			resolve_build_mode(&BuildArgs {
				profile: Some(Profile::Production),
				..Default::default()
			}),
			BuildMode::Release
		);
		// No profile + verifiable=true -> Verifiable
		assert_eq!(
			resolve_build_mode(&BuildArgs { verifiable: true, ..Default::default() }),
			BuildMode::Verifiable
		);
		// No profile + verifiable=false + release=true -> Release
		assert_eq!(
			resolve_build_mode(&BuildArgs { release: true, ..Default::default() }),
			BuildMode::Release
		);
		// Profile::Debug + verifiable=false -> Debug
		assert_eq!(
			resolve_build_mode(&BuildArgs { profile: Some(Profile::Debug), ..Default::default() }),
			BuildMode::Debug
		);
		// No profile + verifiable=false + release=false -> Debug
		assert_eq!(resolve_build_mode(&BuildArgs::default()), BuildMode::Debug);
	}
}
