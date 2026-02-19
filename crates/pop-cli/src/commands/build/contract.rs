// SPDX-License-Identifier: GPL-3.0

use super::{BuildArgs, Profile};
use crate::cli;
use pop_contracts::{BuildMode, ImageVariant, MetadataSpec, Verbosity, build_smart_contract};
use std::path::PathBuf;

/// Configuration for building a smart contract.
pub struct BuildContract {
	/// Path of the contract project.
	pub(crate) path: PathBuf,
	/// Build profile: `Release` for release mode, `Debug` for debug mode, `Verifiable` for
	/// deterministic, release mode.
	pub(crate) build_mode: BuildMode,
	/// Which specification to use for contract metadata.
	pub(crate) metadata: Option<MetadataSpec>,
	/// A custom image for a verifiable build
	pub(crate) image: Option<ImageVariant>,
}

impl BuildContract {
	/// Executes the command.
	pub(crate) fn execute(self) -> anyhow::Result<&'static str> {
		self.build(&mut cli::Cli)
	}

	/// Executes the command in JSON mode and returns the built artifact path.
	pub(crate) fn execute_json(self) -> anyhow::Result<PathBuf> {
		let build_results = build_smart_contract(
			&self.path,
			self.build_mode,
			Verbosity::Quiet,
			self.metadata,
			self.image,
		)?;
		let artifact_path = build_results
			.iter()
			.find_map(|result| result.dest_binary.clone())
			.or_else(|| build_results.first().map(|result| result.target_directory.clone()))
			.unwrap_or_else(|| self.path.clone());
		Ok(artifact_path)
	}

	/// Builds a smart contract
	///
	/// # Arguments
	/// * `cli` - The CLI implementation to be used.
	fn build(self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<&'static str> {
		cli.intro("Building your contract")?;
		// Build contract.
		let build_results = build_smart_contract(
			&self.path,
			self.build_mode,
			Verbosity::Default,
			self.metadata,
			self.image,
		)?;
		for result in build_results {
			cli.success(result.display())?;
		}
		cli.outro("Build completed successfully!")?;
		Ok("contract")
	}
}

/// Resolve the `BuildMode` to use in a contract build depending on the specified args
///
/// # Arguments
/// * `args` - The `BuildArgs` needed to resolve the `BuildMode`
pub(super) fn resolve_build_mode(args: &BuildArgs) -> BuildMode {
	match (&args.profile, args.verifiable) {
		(Some(Profile::Release), false) | (Some(Profile::Production), false) => BuildMode::Release,
		(None, true) => BuildMode::Verifiable,
		(None, false) if args.release => BuildMode::Release,
		// Fallback to debug mode
		_ => BuildMode::Debug,
	}
}

pub(super) fn resolve_image(args: &BuildArgs) -> anyhow::Result<Option<ImageVariant>> {
	match (&args.image, args.verifiable) {
		(Some(image), true) => Ok(Some(ImageVariant::Custom(image.clone()))),
		(None, true) => Ok(Some(ImageVariant::Default)),
		(None, false) => Ok(None),
		(Some(_), false) =>
			Err(anyhow::anyhow!("Custom images can only be used in verifiable builds")),
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

	#[test]
	fn resolve_image_works() {
		// Custom image + verifiable=true -> Custom image
		assert!(matches!(resolve_image(&BuildArgs {
				image: Some("my-image:latest".to_string()),
				verifiable: true,
				..Default::default()
			}), Ok(Some(ImageVariant::Custom(custom))) if custom == "my-image:latest"
		));
		// No image + verifiable=true -> Default image
		assert!(matches!(
			resolve_image(&BuildArgs { verifiable: true, ..Default::default() }),
			Ok(Some(ImageVariant::Default))
		));
		// No image + verifiable=false -> None
		assert!(matches!(resolve_image(&BuildArgs::default()), Ok(None)));
		// Custom image + verifiable=false -> Error
		let err = resolve_image(&BuildArgs {
			image: Some("my-image:latest".to_string()),
			verifiable: false,
			..Default::default()
		})
		.unwrap_err();
		assert_eq!(err.to_string(), "Custom images can only be used in verifiable builds");
	}
}
