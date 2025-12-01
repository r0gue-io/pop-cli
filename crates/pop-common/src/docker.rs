// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use std::{io::ErrorKind, process::Command};

#[derive(Debug)]
pub enum DockerStatus {
	NotInstalled,
	Installed,
	Running,
}

impl DockerStatus {
	pub fn detect() -> Result<Self, Error> {
		match Command::new("docker").arg("info").output() {
			Ok(output) if output.status.success() => Ok(DockerStatus::Running),
			Ok(_) => Ok(DockerStatus::Installed),
			Err(err) if err.kind() == ErrorKind::NotFound => Ok(DockerStatus::NotInstalled),
			Err(err) => Err(Error::Docker(err.to_string())),
		}
	}
}

#[cfg(all(test, feature = "single-threaded-tests"))]
mod tests {
	use super::*;
	use std::{fs::Permissions, os::unix::fs::PermissionsExt};
	use tempfile::TempDir;

	struct TestBuilder {
		original_path: String,
		temp_dir: TempDir,
	}

	impl TestBuilder {
		fn with_fake_path() -> Self {
			let temp_dir = tempfile::tempdir().unwrap();
			let original_path = std::env::var("PATH").unwrap_or_default();

			// Safe as this module run in single threads due to the single-threaded-tests feature: https://doc.rust-lang.org/std/env/fn.set_var.html
			unsafe {
				std::env::set_var("PATH", temp_dir.path());
			}

			TestBuilder { original_path, temp_dir }
		}

		fn with_fake_docker_in_path(self, exit_code: i32) -> Self {
			let fake_docker_path = self.temp_dir.path().join("docker");
			let script = format!("#!/bin/sh\nexit {}", exit_code);
			std::fs::write(&fake_docker_path, script).unwrap();
			std::fs::set_permissions(&fake_docker_path, Permissions::from_mode(0o755)).unwrap();
			self
		}

		fn with_not_permissioned_fake_docker_in_path(self) -> Self {
			let fake_docker_path = self.temp_dir.path().join("docker");
			let script = format!("#!/bin/sh\nexit 0");
			std::fs::write(&fake_docker_path, script).unwrap();
			self
		}

		fn execute<F>(self, test: F)
		where
			F: FnOnce(),
		{
			test()
		}
	}

	impl Drop for TestBuilder {
		fn drop(&mut self) {
			unsafe {
				std::env::set_var("PATH", self.original_path.clone());
			}
		}
	}

	#[test]
	fn detect_docker_running() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(0).execute(|| {
			assert!(matches!(DockerStatus::detect(), Ok(DockerStatus::Running)));
		});
	}

	#[test]
	fn detect_docker_installed() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(1).execute(|| {
			assert!(matches!(DockerStatus::detect(), Ok(DockerStatus::Installed)));
		});
	}

	#[test]
	fn detect_docker_not_installed() {
		TestBuilder::with_fake_path().execute(|| {
			assert!(matches!(DockerStatus::detect(), Ok(DockerStatus::NotInstalled)));
		});
	}

	#[test]
	fn detect_docker_fails() {
		TestBuilder::with_fake_path().with_not_permissioned_fake_docker_in_path().execute(|| {
			assert!(matches!(DockerStatus::detect(), Err(err) if err.to_string() == "Docker error: Permission denied (os error 13)"));
		});
	}
}
