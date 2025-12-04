// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use std::{io::ErrorKind, process::Command, thread::sleep, time::Duration};

/// Represents the state of Docker in the user's machine
pub enum Docker {
	/// Docker isn't installed
	NotInstalled,
	/// Docker is installed but not running
	Installed,
	/// Docker is already running
	Running,
}

impl Docker {
	/// Ensures Docker is running. If installed but not running, attempts to start it.
	pub fn ensure_running() -> Result<(), Error> {
		match Self::detect_docker()? {
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

	fn detect_docker() -> Result<Self, Error> {
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

		#[cfg(target_os = "linux")]
		return Self::try_start_linux();

		#[cfg(not(any(target_os = "macos", target_os = "linux")))]
		Ok(())
	}

	#[allow(dead_code)] // Fine as depending on the platform it might be called or not
	fn try_start_macos() -> Result<(), Error> {
		// Try start docker using Docker Desktop.
		Command::new("open").args(["-a", "Docker"]).spawn().map_err(|_err| {
			Error::Docker("Failed to start Docker. Please start it manually.".to_owned())
		})?;

		Ok(())
	}

	#[allow(dead_code)] // Fine as depending on the platform it might be called or not
	fn try_start_linux() -> Result<(), Error> {
		// Check if running as root
		if !crate::helpers::is_root() {
			return Err(Error::Docker(
				"Docker is not running. Please run this command with sudo to allow pop to initialize it, or start it manually.".to_string(),
			));
		}

		// Try to start Docker with systemctl
		Command::new("systemctl").args(["start", "docker"]).status().map_or_else(
			|_| {
				Err(Error::Docker(
					"Failed to start Docker automatically. Please start it manually.".to_string(),
				))
			},
			|status| {
				if status.success() {
					Ok(())
				} else {
					Err(Error::Docker(
						"Failed to start Docker automatically. Please start it manually."
							.to_string(),
					))
				}
			},
		)
	}

	/// Waits for Docker daemon to be ready (polls for up to 30 seconds)
	fn wait_for_ready() -> Result<(), Error> {
		for _i in 0..30 {
			sleep(Duration::from_secs(1));

			if matches!(Self::detect_docker()?, Docker::Running) {
				return Ok(());
			}
		}

		Err(Error::Docker(
			"Docker failed to start within 30 seconds. Please start it manually.".to_string(),
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::command_mock::CommandMock;

	#[test]
	fn detect_docker_docker_running() {
		CommandMock::default().with_command("docker", 0).execute(|| {
			assert!(matches!(Docker::detect_docker(), Ok(Docker::Running)));
		});
	}

	#[test]
	fn detect_docker_docker_installed() {
		CommandMock::default().with_command("docker", 1).execute(|| {
			assert!(matches!(Docker::detect_docker(), Ok(Docker::Installed)));
		});
	}

	#[test]
	fn detect_docker_docker_not_installed() {
		CommandMock::default().execute(|| {
			assert!(matches!(Docker::detect_docker(), Ok(Docker::NotInstalled)));
		});
	}

	#[test]
	fn detect_docker_docker_fails() {
		CommandMock::default().with_non_permissioned_command("docker").execute(|| {
			assert!(matches!(Docker::detect_docker(), Err(Error::Docker(err)) if err == "Permission denied (os error 13)"));
		});
	}

	#[test]
	fn ensure_running_when_already_running() {
		CommandMock::default().with_command("docker", 0).execute(|| {
			assert!(Docker::ensure_running().is_ok());
		});
	}

	#[test]
	fn ensure_running_when_not_installed() {
		CommandMock::default().execute(|| {
			assert!(matches!(Docker::ensure_running(), Err(Error::Docker(err)) if err == "Docker is not installed. Install from: https://docs.docker.com/get-docker/"));
		});
	}

	#[test]
	#[cfg(target_os = "macos")]
	fn ensure_running_starts_docker_on_macos() {
		let command_mock = CommandMock::default();
		let started_marker = command_mock.fake_path().join("docker_started");
		let docker_script = format!(
			"#!/bin/sh\nif [ -f \"{}\" ]; then\n    exit 0\nelse\n    exit 1\nfi",
			started_marker.display()
		);
		let open_script = format!("#!/bin/sh\n> \"{}\"", started_marker.display());

		command_mock
			.with_command_script("docker", &docker_script)
			.with_command_script("open", &open_script)
			.execute(|| {
				assert!(Docker::ensure_running().is_ok());
			});
	}

	#[test]
	#[cfg(target_os = "linux")]
	fn ensure_running_starts_docker_on_linux_as_root() {
		let command_mock = CommandMock::default();
		let started_marker = command_mock.fake_path().join("docker_started");
		let docker_script = format!(
			"#!/bin/sh\nif [ -f \"{}\" ]; then\n    exit 0\nelse\n    exit 1\nfi",
			started_marker.display()
		);
		let systemctl_script = format!("#!/bin/sh\n> \"{}\"", started_marker.display());

		command_mock
			.with_command_script("docker", &docker_script)
			.with_command_script("id", "#!/bin/sh\necho 0") // root user
			.with_command_script("systemctl", &systemctl_script)
			.execute(|| {
				assert!(Docker::ensure_running().is_ok());
			});
	}

	#[test]
	fn try_start_macos_succeeds_with_open_command() {
		CommandMock::default().with_command("open", 0).execute(|| {
			assert!(Docker::try_start_macos().is_ok());
		});
	}

	#[test]
	fn try_start_macos_fails_without_open_command() {
		CommandMock::default().execute(|| {
			assert!(matches!(
				Docker::try_start_macos(),
				Err(
					Error::Docker(err)
				)  if err == "Failed to start Docker. Please start it manually."
			));
		});
	}

	#[test]
	fn try_start_linux_fails_when_not_root() {
		CommandMock::default()
			.with_command_script("id", "#!/bin/sh\necho 1000") // non-root user
			.execute(|| {
				assert!(matches!(
					Docker::try_start_linux(),
					Err(Error::Docker(err))
					if err == "Docker is not running. Please run this command with sudo to allow pop to initialize it, or start it manually."
				));
			});
	}

	#[test]
	fn try_start_linux_succeeds_as_root_with_systemctl() {
		CommandMock::default()
			.with_command_script("id", "#!/bin/sh\necho 0") // root user
			.with_command("systemctl", 0) // systemctl succeeds
			.execute(|| {
				assert!(Docker::try_start_linux().is_ok());
			});
	}

	#[test]
	fn try_start_linux_fails_as_root_when_systemctl_fails() {
		CommandMock::default()
			.with_command_script("id", "#!/bin/sh\necho 0") // root user
			.with_command("systemctl", 1) // systemctl fails
			.execute(|| {
				assert!(matches!(
					Docker::try_start_linux(),
					Err(Error::Docker(err))
					if err == "Failed to start Docker automatically. Please start it manually."
				));
			});
	}

	#[test]
	fn wait_for_ready_succeeds_when_docker_starts() {
		let command_mock = CommandMock::default();
		let started_marker = command_mock.fake_path().join("docker_started");
		let docker_script = format!(
			"#!/bin/sh\nif [ -f \"{}\" ]; then\n    exit 0\nelse\n    exit 1\nfi",
			started_marker.display()
		);

		command_mock.with_command_script("docker", &docker_script).execute(|| {
			// Create the marker file to simulate Docker starting
			std::fs::write(&started_marker, "").unwrap();

			assert!(Docker::wait_for_ready().is_ok());
		});
	}

	#[test]
	fn wait_for_ready_times_out_when_docker_never_starts() {
		CommandMock::default().with_command("docker", 1).execute(|| {
            assert!(matches!(Docker::wait_for_ready(), Err(Error::Docker(err)) if err == "Docker failed to start within 30 seconds. Please start it manually."));
		});
	}
}
