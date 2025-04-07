// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use duct::cmd;
use pop_common::{manifest::from_path, Profile};
pub use srtool_lib::{get_image_digest, get_image_tag, ContainerEngine};
use std::{
	env, fs,
	path::{Path, PathBuf},
};

const DEFAULT_IMAGE: &str = "docker.io/paritytech/srtool";
const TIMEOUT: u64 = 60 * 60;

/// Builds and executes the command for running a deterministic runtime build process using
/// srtool.
pub struct DeterministicBuilder {
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

impl DeterministicBuilder {
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
		package: &str,
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
			package: package.to_owned(),
			path: dir,
			profile,
			runtime_dir,
			tag,
		})
	}

	/// Executes the runtime build process and returns the path of the generated file.
	pub fn build(&self) -> Result<PathBuf, Error> {
		let command = self.build_command();
		cmd("sh", vec!["-c", &command]).stdout_null().stderr_null().run()?;
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

/// Determines whether the manifest at the supplied path is a supported Substrate runtime project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn is_supported(path: Option<&Path>) -> Result<bool, Error> {
	let manifest = from_path(path)?;
	// Simply check for a parachain dependency
	const DEPENDENCIES: [&str; 3] = ["frame-system", "frame-support", "substrate-wasm-builder"];
	let has_dependencies = DEPENDENCIES.into_iter().any(|d| {
		manifest.dependencies.contains_key(d) ||
			manifest.workspace.as_ref().is_some_and(|w| w.dependencies.contains_key(d))
	});
	let has_features = manifest.features.contains_key("runtime-benchmarks") ||
		manifest.features.contains_key("try-runtime");
	Ok(has_dependencies && has_features)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use fs::write;
	use pop_common::manifest::Dependency;
	use tempfile::tempdir;

	#[test]
	fn srtool_builder_new_works() -> Result<()> {
		let srtool_builer = DeterministicBuilder::new(
			ContainerEngine::Docker,
			None,
			&"parachain-template-runtime".to_string(),
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
			DeterministicBuilder::new(
				ContainerEngine::Podman,
				Some(path.to_path_buf()),
				&"parachain-template-runtime".to_string(),
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
		let srtool_builder = DeterministicBuilder::new(
			ContainerEngine::Podman,
			None,
			&"template-runtime".to_string(),
			Profile::Debug,
			PathBuf::from("./runtime-folder"),
		)?;
		assert_eq!(srtool_builder.get_output_path().display().to_string(), "./runtime-folder/target/srtool/debug/wbuild/template-runtime/template_runtime.compact.compressed.wasm");
		Ok(())
	}

	#[test]
	fn is_supported_works() -> Result<()> {
		let temp_dir = tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		assert!(!is_supported(Some(&path.join(name)))?);

		// Parachain runtime with dependency
		let mut manifest = from_path(Some(&path.join(name)))?;
		manifest
			.dependencies
			.insert("substrate-wasm-builder".into(), Dependency::Simple("^0.14.0".into()));
		manifest.features.insert("try-runtime".into(), vec![]);
		let manifest = toml_edit::ser::to_string_pretty(&manifest)?;
		write(path.join(name).join("Cargo.toml"), manifest)?;
		assert!(is_supported(Some(&path.join(name)))?);
		Ok(())
	}
}
