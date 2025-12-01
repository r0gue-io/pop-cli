// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use std::{io::ErrorKind, process::Command, thread::sleep, time::Duration};

pub enum Docker {
	NotInstalled,
	Installed,
	Running,
}

impl Docker {
	/// Ensures Docker is running. If installed but not running, attempts to start it.
	pub fn ensure_running() -> Result<(), Error> {
		match Self::detect_docker_status()? {
			Docker::Running => Ok(()),
			Docker::Installed => {
				Self::try_start()?;
				Self::wait_for_ready()?;
				Ok(())
			},
			Docker::NotInstalled => Err(Error::Docker(
				"Docker is not installed. Install from: https://docs.docker.com/get-docker/"
					.to_string(),
			)),
		}
	}

	fn detect_docker_status() -> Result<Self, Error> {
		match Command::new("docker").arg("info").output() {
			Ok(output) if output.status.success() => Ok(Docker::Running),
			Ok(_) => Ok(Docker::Installed),
			Err(err) if err.kind() == ErrorKind::NotFound => Ok(Docker::NotInstalled),
			Err(err) => Err(Error::Docker(err.to_string())),
		}
	}

	/// Attempts to start Docker based on the platform.
	fn try_start() -> Result<(), Error> {
		#[cfg(target_os = "macos")]
		return Self::try_start_macos();

		#[cfg(target_os = "windows")]
		return Self::try_start_windows();

		#[cfg(target_os = "linux")]
		return Self::try_start_linux();

		#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
		Ok(())
	}

	#[allow(dead_code)] // Fine as depending on the platform it might be called or not
	fn try_start_macos() -> Result<(), Error> {
		// Try start docker using Docker Desktop.
		Command::new("open").args(["-a", "Docker"]).spawn().map_err(|_err| {
			Error::Docker(format!("Failed to start Docker. Please start it manually."))
		})?;

		Ok(())
	}

	#[allow(dead_code)] // Fine as depending on the platform it might be called or not
	fn try_start_windows() -> Result<(), Error> {
		Command::new("cmd")
			.args(["/C", "start", "", r"C:\Program Files\Docker\Docker\Docker Desktop.exe"])
			.spawn()
			.map_err(|err| {
				println!("{:?}", err);
				Error::Docker(format!("Failed to start Docker. Please start it manually."))
			})?;

		Ok(())
	}

	#[allow(dead_code)] // Fine as depending on the platform it might be called or not
	fn try_start_linux() -> Result<(), Error> {
		Err(Error::Docker(
			"Please start Docker manually:\n  sudo systemctl start docker".to_string(),
		))
	}

	/// Waits for Docker daemon to be ready (polls for up to 30 seconds)
	fn wait_for_ready() -> Result<(), Error> {
		for _i in 0..30 {
			sleep(Duration::from_secs(1));

			if matches!(Self::detect_docker_status()?, Docker::Running) {
				return Ok(());
			}
		}

		Err(Error::Docker(
			"Docker failed to start within 30 seconds. Please start it manually.".to_string(),
		))
	}
}

#[cfg(all(test, feature = "single-threaded-tests"))]
mod tests {
	use super::*;
	use std::path::Path;
	use tempfile::TempDir;

	// Helper to set executable permissions cross-platform
	fn set_executable(path: &Path) -> std::io::Result<()> {
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let permissions = std::fs::Permissions::from_mode(0o755);
			std::fs::set_permissions(path, permissions)?;
		}

		// On Windows, we don't need to set executable permissions
		// Files are executable based on extension or can be run via shell
		Ok(())
	}

	// Helper to get the correct executable name for the platform
	fn exe_name(name: &str) -> String {
		#[cfg(windows)]
		{
			format!("{}.exe", name)
		}
		#[cfg(not(windows))]
		{
			name.to_string()
		}
	}

	// Helper to create a script that exits with a given code
	fn exit_script(exit_code: i32) -> String {
		#[cfg(windows)]
		{
			format!("@echo off\r\nexit {}", exit_code)
		}
		#[cfg(not(windows))]
		{
			format!("#!/bin/sh\nexit {}", exit_code)
		}
	}

	// Helper to create a script that creates a file
	fn create_file_script(file_path: &std::path::Path) -> String {
		#[cfg(windows)]
		{
			format!("@echo off\r\ntype nul > \"{}\"", file_path.display())
		}
		#[cfg(not(windows))]
		{
			format!("#!/bin/sh\n> \"{}\"", file_path.display())
		}
	}

	// Helper to create a script that checks if a file exists and exits accordingly
	fn check_file_exists_script(file_path: &std::path::Path) -> String {
		#[cfg(windows)]
		{
			format!("@echo off\r\nif exist \"{}\" (exit 0) else (exit 1)", file_path.display())
		}
		#[cfg(not(windows))]
		{
			format!(
				"#!/bin/sh\nif [ -f \"{}\" ]; then\n    exit 0\nelse\n    exit 1\nfi",
				file_path.display()
			)
		}
	}

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
			let fake_docker_path = self.temp_dir.path().join(exe_name("docker"));
			let script = exit_script(exit_code);
			std::fs::write(&fake_docker_path, script).unwrap();
			set_executable(&fake_docker_path).unwrap();
			self
		}

		fn with_not_permissioned_fake_docker_in_path(self) -> Self {
			let fake_docker_path = self.temp_dir.path().join(exe_name("docker"));
			let script = exit_script(0);
			std::fs::write(&fake_docker_path, script).unwrap();
			self
		}

		// Helper to create a fake docker + open/cmd that simulates Docker starting
		// For macOS: creates fake docker and fake open command
		fn with_docker_that_starts_after_open(self) -> Self {
			let fake_docker_path = self.temp_dir.path().join(exe_name("docker"));
			let fake_open_path = self.temp_dir.path().join(exe_name("open"));
			let started_marker = self.temp_dir.path().join("docker_started");

			// Fake docker checks if marker file exists
			let docker_script = check_file_exists_script(&started_marker);
			std::fs::write(&fake_docker_path, docker_script).unwrap();
			set_executable(&fake_docker_path).unwrap();

			// Fake open creates marker file immediately (accepts any arguments)
			let open_script = create_file_script(&started_marker);
			std::fs::write(&fake_open_path, open_script).unwrap();
			set_executable(&fake_open_path).unwrap();

			self
		}

		// Helper to create a fake docker + cmd that simulates Docker starting
		// For Windows: creates fake docker and fake cmd command
		fn with_docker_that_starts_after_cmd(self) -> Self {
			let fake_docker_path = self.temp_dir.path().join(exe_name("docker"));
			let fake_cmd_path = self.temp_dir.path().join(exe_name("cmd"));
			let started_marker = self.temp_dir.path().join("docker_started");

			// Fake docker checks if marker file exists
			let docker_script = check_file_exists_script(&started_marker);
			std::fs::write(&fake_docker_path, docker_script).unwrap();
			set_executable(&fake_docker_path).unwrap();

			// Fake cmd creates marker file
			let cmd_script = create_file_script(&started_marker);
			std::fs::write(&fake_cmd_path, cmd_script).unwrap();
			set_executable(&fake_cmd_path).unwrap();

			self
		}

		fn with_fake_open_in_path(self) -> Self {
			let fake_open_path = self.temp_dir.path().join(exe_name("open"));
			let script = exit_script(0);
			std::fs::write(&fake_open_path, script).unwrap();
			set_executable(&fake_open_path).unwrap();
			self
		}

		fn with_fake_cmd_in_path(self) -> Self {
			let fake_cmd_path = self.temp_dir.path().join(exe_name("cmd"));
			let script = exit_script(0);
			std::fs::write(&fake_cmd_path, script).unwrap();
			set_executable(&fake_cmd_path).unwrap();
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
	fn detect_docker_status_docker_running() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(0).execute(|| {
			assert!(matches!(Docker::detect_docker_status(), Ok(Docker::Running)));
		});
	}

	#[test]
	fn detect_docker_status_docker_installed() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(1).execute(|| {
			assert!(matches!(Docker::detect_docker_status(), Ok(Docker::Installed)));
		});
	}

	#[test]
	fn detect_docker_status_docker_not_installed() {
		TestBuilder::with_fake_path().execute(|| {
			assert!(matches!(Docker::detect_docker_status(), Ok(Docker::NotInstalled)));
		});
	}

	#[test]
	fn detect_docker_status_docker_fails() {
		TestBuilder::with_fake_path().with_not_permissioned_fake_docker_in_path().execute(|| {
			assert!(matches!(Docker::detect_docker_status(), Err(Error::Docker(err)) if err == "Permission denied (os error 13)"));
		});
	}

	#[test]
	fn ensure_running_when_already_running() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(0).execute(|| {
			assert!(Docker::ensure_running().is_ok());
		});
	}

	#[test]
	fn ensure_running_when_not_installed() {
		TestBuilder::with_fake_path().execute(|| {
			assert!(matches!(Docker::ensure_running(), Err(Error::Docker(err)) if err == "Docker is not installed. Install from: https://docs.docker.com/get-docker/"));
		});
	}

	#[test]
	#[cfg(target_os = "macos")]
	fn ensure_running_starts_docker_on_macos() {
		TestBuilder::with_fake_path().with_docker_that_starts_after_open().execute(|| {
			assert!(Docker::ensure_running().is_ok());
		});
	}

	#[test]
	#[cfg(target_os = "windows")]
	fn ensure_running_starts_docker_on_windows() {
		TestBuilder::with_fake_path().with_docker_that_starts_after_cmd().execute(|| {
			assert!(Docker::ensure_running().is_ok());
		});
	}

	#[test]
	#[cfg(target_os = "linux")]
	fn ensure_running_fails_on_linux() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(1).execute(|| {
			assert!(matches!(
				Docker::ensure_running(),
				Err(
					Error::Docker(err) if err == "Please start Docker manually:\n  sudo systemctl start docker",
				)
			));
		});
	}

	#[test]
	fn try_start_macos_succeeds_with_open_command() {
		TestBuilder::with_fake_path().with_fake_open_in_path().execute(|| {
			assert!(Docker::try_start_macos().is_ok());
		});
	}

	#[test]
	fn try_start_macos_fails_without_open_command() {
		TestBuilder::with_fake_path().execute(|| {
			assert!(matches!(
				Docker::try_start_macos(),
				Err(
					Error::Docker(err)
				)  if err == "Failed to start Docker. Please start it manually."
			));
		});
	}

	#[test]
	fn try_start_windows_succeeds_with_cmd_command() {
		TestBuilder::with_fake_path().with_fake_cmd_in_path().execute(|| {
			assert!(Docker::try_start_windows().is_ok());
		});
	}

	#[test]
	fn try_start_windows_fails_without_cmd_command() {
		TestBuilder::with_fake_path().execute(|| {
			println!("{:?}", Docker::try_start_windows());
			assert!(matches!(
				Docker::try_start_windows(),
				Err(
					Error::Docker(err)
				) if err == "Failed to start Docker. Please start it manually."
			));
		});
	}

	#[test]
	fn try_start_linux_always_fails() {
		TestBuilder::with_fake_path().execute(|| {
			assert!(matches!(
				Docker::try_start_linux(),
				Err(
					Error::Docker(err)
				)  if err == "Please start Docker manually:\n  sudo systemctl start docker"
			));
		});
	}

	#[test]
	fn wait_for_ready_succeeds_when_docker_starts_on_macos() {
		TestBuilder::with_fake_path().with_docker_that_starts_after_open().execute(|| {
			// Trigger the fake open command to start docker startup
			let _ = std::process::Command::new("open").arg("-a").arg("Docker").spawn();

			assert!(Docker::wait_for_ready().is_ok());
		});
	}

	#[test]
	fn wait_for_ready_succeeds_when_docker_starts_on_windows() {
		TestBuilder::with_fake_path().with_docker_that_starts_after_cmd().execute(|| {
			// Trigger the fake cmd command to start docker startup
			let _ = std::process::Command::new("cmd").args(["/C", "start"]).spawn();

			assert!(Docker::wait_for_ready().is_ok());
		});
	}

	#[test]
	fn wait_for_ready_times_out_when_docker_never_starts() {
		TestBuilder::with_fake_path().with_fake_docker_in_path(1).execute(|| {
            assert!(matches!(Docker::wait_for_ready(), Err(Error::Docker(err)) if err == "Docker failed to start within 30 seconds. Please start it manually."));
		});
	}
}
