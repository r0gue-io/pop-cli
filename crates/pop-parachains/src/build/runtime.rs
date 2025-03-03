// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use duct::cmd;
use pop_common::Profile;
pub use srtool_lib::{get_image_digest, get_image_tag, ContainerEngine};
use std::{env, fs, path::PathBuf};

const DEFAULT_IMAGE: &str = "docker.io/paritytech/srtool";
const TIMEOUT: u64 = 60 * 60;

/// Builds and executes the command for running a deterministic runtime build process using
/// srtool.
pub struct Builder {
	/// Mount point for cargo cache.
	cache_mount: String,
	/// List of default features to enable during the build process.
	default_features: String,
	/// Digest of the image for reproducibility.
	digest: String,
	/// The container engine used to run the build process.
	engine: ContainerEngine,
	/// Name of the image used for building.
	image: String,
	/// The runtime package name.
	package: String,
	/// The path to the project directory.
	path: PathBuf,
	/// The profile used for building.
	profile: Profile,
	/// The directory path where the runtime is located.
	runtime_dir: PathBuf,
	/// The tag of the image to use.
	tag: String,
}

impl Builder {
	/// Creates a new instance of `Builder`.
	///
	/// # Arguments
	/// * `engine` - The container engine to use.
	/// * `path` - The path to the project.
	/// * `package` - The runtime package name.
	/// * `profile` - The profile to build the runtime.
	/// * `runtime_dir` - The directory path where the runtime is located.
	pub fn new(
		engine: ContainerEngine,
		path: Option<PathBuf>,
		package: String,
		profile: Profile,
		runtime_dir: PathBuf,
	) -> Result<Self, Error> {
		let default_features = String::new();
		let tag = get_image_tag(Some(TIMEOUT)).map_err(|_| Error::ImageTagRetrievalFailed)?;
		let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
		let dir = fs::canonicalize(path.unwrap_or_else(|| PathBuf::from("./")))?;
		let tmpdir = env::temp_dir().join("cargo");

		let no_cache = engine == ContainerEngine::Podman;
		let cache_mount =
			if !no_cache { format!("-v {}:/cargo-home", tmpdir.display()) } else { String::new() };

		Ok(Self {
			cache_mount,
			default_features,
			digest,
			engine,
			image: DEFAULT_IMAGE.to_string(),
			package,
			path: dir,
			profile,
			runtime_dir,
			tag,
		})
	}

	/// Executes the runtime build process and returns the path of the generated file.
	pub fn build(&self) -> Result<PathBuf, Error> {
		let command = self.build_command();
		cmd("sh", vec!["-c", &command]).run()?;
		let wasm_path = self.get_output_path();
		Ok(wasm_path)
	}

	// Builds the srtool runtime container command string.
	fn build_command(&self) -> String {
		format!(
			"{} run --name srtool --rm \
			 -e PACKAGE={} \
			 -e RUNTIME_DIR={} \
			 -e DEFAULT_FEATURES={} \
			 -e PROFILE={} \
			 -e IMAGE={} \
			 -v {}:/build \
			 {} \
			 {}:{} build --app --json",
			self.engine,
			self.package,
			self.runtime_dir.display(),
			self.default_features,
			self.profile,
			self.digest,
			self.path.display(),
			self.cache_mount,
			self.image,
			self.tag
		)
	}

	// Returns the expected output path of the compiled runtime `.wasm` file.
	fn get_output_path(&self) -> PathBuf {
		self.runtime_dir
			.join("target")
			.join("srtool")
			.join(self.profile.to_string())
			.join("wbuild")
			.join(&self.package)
			.join(format!("{}.compact.compressed.wasm", self.package.replace("-", "_")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn srtool_builder_new_works() -> Result<()> {
		let srtool_builer = Builder::new(
			ContainerEngine::Docker,
			None,
			"parachain-template-runtime".to_string(),
			Profile::Release,
			PathBuf::from("./runtime"),
		)?;
		assert_eq!(
			srtool_builer.cache_mount,
			format!("-v {}:/cargo-home", env::temp_dir().join("cargo").display())
		);
		assert_eq!(srtool_builer.default_features, "");

		let tag = get_image_tag(Some(TIMEOUT))?;
		let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
		assert_eq!(srtool_builer.digest, digest);
		assert_eq!(srtool_builer.tag, tag);

		assert!(srtool_builer.engine == ContainerEngine::Docker);
		assert_eq!(srtool_builer.image, DEFAULT_IMAGE);
		assert_eq!(srtool_builer.package, "parachain-template-runtime");
		assert_eq!(srtool_builer.path, fs::canonicalize(PathBuf::from("./"))?);
		assert_eq!(srtool_builer.profile, Profile::Release);
		assert_eq!(srtool_builer.runtime_dir, PathBuf::from("./runtime"));

		Ok(())
	}

	#[test]
	fn build_command_works() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();
		let tag = get_image_tag(Some(TIMEOUT))?;
		let digest = get_image_digest(DEFAULT_IMAGE, &tag).unwrap_or_default();
		assert_eq!(
			Builder::new(
				ContainerEngine::Podman,
				Some(path.to_path_buf()),
				"parachain-template-runtime".to_string(),
				Profile::Production,
				PathBuf::from("./runtime"),
			)?
			.build_command(),
			format!(
				"podman run --name srtool --rm \
			 -e PACKAGE=parachain-template-runtime \
			 -e RUNTIME_DIR=./runtime \
			 -e DEFAULT_FEATURES= \
			 -e PROFILE=production \
			 -e IMAGE={} \
			 -v {}:/build \
			 {} \
			 {}:{} build --app --json",
				digest,
				fs::canonicalize(path)?.display(),
				String::new(),
				DEFAULT_IMAGE,
				tag
			)
		);
		Ok(())
	}

	#[test]
	fn get_output_path_works() -> Result<()> {
		let srtool_builder = Builder::new(
			ContainerEngine::Podman,
			None,
			"template-runtime".to_string(),
			Profile::Debug,
			PathBuf::from("./runtime-folder"),
		)?;
		assert_eq!(srtool_builder.get_output_path().display().to_string(), "./runtime-folder/target/srtool/debug/wbuild/template-runtime/template_runtime.compact.compressed.wasm");
		Ok(())
	}
}
