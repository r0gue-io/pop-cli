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
		match duct::cmd!("docker", "info")
			.timeout(Duration::from_secs(5))
			.stdout_capture()
			.stderr_capture()
			.unchecked()
			.run()
		{
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
			let args = std::env::args().skip(1).collect::<Vec<String>>().join(" ");
			return Err(Error::Docker(format!(
				"Docker is not running. Please run `sudo $(which pop) {}` to allow pop to initialize it, or start it manually.",
				args
			)));
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

	/// Pulls a Docker image. Requires Docker to be running.
	///
	/// # Arguments
	/// * `image` - The image name.
	/// * `tag` - The image tag.
	pub fn pull_image(image: &str, tag: &str) -> Result<(), Error> {
		// Check if Docker is running
		match Self::detect_docker()? {
			Docker::Running => {},
			_ => return Err(Error::Docker("Docker is not running.".to_string())),
		}

		let image_with_tag = format!("{}:{}", image, tag);

		let output = Command::new("docker")
			.args(["pull", &image_with_tag])
			.output()
			.map_err(|e| Error::Docker(format!("Failed to pull image: {}", e)))?;

		if !output.status.success() {
			return Err(Error::Docker(format!(
				"Failed to pull image {}: {}",
				image_with_tag,
				String::from_utf8_lossy(&output.stderr)
			)));
		}

		Ok(())
	}

	/// Gets the digest of a Docker image. Requires Docker to be running.
	/// If the image is not available locally, it will be pulled automatically.
	///
	/// # Arguments
	/// * `image` - The image name.
	/// * `tag` - The image tag.
	pub fn get_image_digest(image: &str, tag: &str) -> Result<String, Error> {
		// Check if Docker is running
		match Self::detect_docker()? {
			Docker::Running => {},
			_ => return Err(Error::Docker("Docker is not running.".to_string())),
		}

		let image_with_tag = format!("{}:{}", image, tag);

		let mut output = Command::new("docker")
			.args(["image", "inspect", "--format={{.RepoDigests}}", &image_with_tag])
			.output()
			.map_err(|e| Error::Docker(format!("Failed to inspect image: {}", e)))?;

		// If inspect fails, try pulling the image first
		if !output.status.success() {
			Self::pull_image(image, tag)?;

			// Retry inspect after pulling
			output = Command::new("docker")
				.args(["image", "inspect", "--format={{.RepoDigests}}", &image_with_tag])
				.output()
				.map_err(|e| Error::Docker(format!("Failed to inspect image: {}", e)))?;

			if !output.status.success() {
				return Err(Error::Docker(format!(
					"Failed to inspect image {} after pulling: {}",
					image_with_tag,
					String::from_utf8_lossy(&output.stderr)
				)));
			}
		}

		let output_str = String::from_utf8(output.stdout)
			.map_err(|e| Error::Docker(format!("Invalid UTF-8 in docker output: {}", e)))?;

		// Parse the digest from the output format: [image@sha256:...]
		let digest = output_str
			.trim()
			.trim_start_matches('[')
			.trim_end_matches(']')
			.split('@')
			.nth(1)
			.ok_or_else(|| Error::Docker("Could not parse digest from docker output.".to_string()))?
			.to_string();

		Ok(digest)
	}
}

/// Fetches the latest tag for a Docker image from a URL.
///
/// # Arguments
/// * `url` - The URL to fetch the tag from.
pub async fn fetch_image_tag(url: &str) -> Result<String, Error> {
	let response = reqwest::get(url)
		.await
		.map_err(|e| Error::Docker(format!("Failed to fetch image tag: {}", e)))?;

	if !response.status().is_success() {
		return Err(Error::Docker(format!(
			"Failed to fetch image tag from {}: HTTP {}",
			url,
			response.status()
		)));
	}

	let tag = response
		.text()
		.await
		.map_err(|e| Error::Docker(format!("Failed to read response body: {}", e)))?;

	Ok(tag.trim().to_string())
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
			r#"#!/bin/sh
if [ -f "{}" ]; then
    exit 0
else
    exit 1
fi"#,
			started_marker.display()
		);
		let open_script = format!(
			r#"#!/bin/sh
> "{}"
"#,
			started_marker.display()
		);

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
			r#"#!/bin/sh
if [ -f "{}" ]; then
    exit 0
else
    exit 1
fi"#,
			started_marker.display()
		);
		let systemctl_script = format!(
			r#"#!/bin/sh
> "{}"
"#,
			started_marker.display()
		);

		command_mock
			.with_command_script("docker", &docker_script)
			.with_command_script(
				"id",
				r#"#!/bin/sh
echo 0"#,
			) // root user
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
			.with_command_script("id", r#"#!/bin/sh
echo 1000"#) // non-root user
			.execute(|| {
                // Cannot assert too much about this, depending on how tests are called, args will contain different values
                let args = std::env::args().skip(1).collect::<Vec<String>>().join(" ");
				assert!(matches!(
					Docker::try_start_linux(),
					Err(Error::Docker(err))
					if err == format!("Docker is not running. Please run `sudo $(which pop) {}` to allow pop to initialize it, or start it manually.", args)
				));
			});
	}

	#[test]
	fn try_start_linux_succeeds_as_root_with_systemctl() {
		CommandMock::default()
			.with_command_script(
				"id",
				r#"#!/bin/sh
echo 0"#,
			) // root user
			.with_command("systemctl", 0) // systemctl succeeds
			.execute(|| {
				assert!(Docker::try_start_linux().is_ok());
			});
	}

	#[test]
	fn try_start_linux_fails_as_root_when_systemctl_fails() {
		CommandMock::default()
			.with_command_script(
				"id",
				r#"#!/bin/sh
echo 0"#,
			) // root user
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
			r#"#!/bin/sh
if [ -f "{}" ]; then
    exit 0
else
    exit 1
fi"#,
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

	#[test]
	fn pull_image_succeeds_when_docker_running() {
		CommandMock::default().with_command("docker", 0).execute(|| {
			assert!(Docker::pull_image("test/image", "latest").is_ok());
		});
	}

	#[test]
	fn pull_image_fails_when_docker_not_running() {
		CommandMock::default().with_command("docker", 1).execute(|| {
			assert!(matches!(
				Docker::pull_image("test/image", "latest"),
				Err(Error::Docker(err)) if err == "Docker is not running."
			));
		});
	}

	#[test]
	fn pull_image_fails_when_pull_command_fails() {
		let command_mock = CommandMock::default();
		let docker_info_script = r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0;
else
    exit 1;
fi"#;

		command_mock.with_command_script("docker", docker_info_script).execute(|| {
			assert!(matches!(
				Docker::pull_image("test/image", "latest"),
				Err(Error::Docker(err)) if err.contains("Failed to pull image")
			));
		});
	}

	#[test]
	fn get_image_digest_succeeds_with_local_image() {
		let command_mock = CommandMock::default();
		let docker_script = r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0
elif [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
    echo "[test/image@sha256:abcd1234]"
    exit 0
fi
exit 1"#;

		command_mock.with_command_script("docker", docker_script).execute(|| {
			let result = Docker::get_image_digest("test/image", "latest");
			assert!(result.is_ok());
			assert_eq!(result.unwrap(), "sha256:abcd1234");
		});
	}

	#[test]
	fn get_image_digest_pulls_and_succeeds_when_image_not_local() {
		let command_mock = CommandMock::default();
		let pulled_marker = command_mock.fake_path().join("image_pulled");
		let docker_script = format!(
			r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0
elif [ "$1" = "pull" ]; then
    > "{}"
    exit 0
elif [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
    if [ -f "{}" ]; then
        echo "[test/image@sha256:abcd1234]"
        exit 0
    else
        exit 1
    fi
fi
exit 1"#,
			pulled_marker.display(),
			pulled_marker.display()
		);

		command_mock.with_command_script("docker", &docker_script).execute(|| {
			let result = Docker::get_image_digest("test/image", "latest");
			assert!(result.is_ok());
			assert_eq!(result.unwrap(), "sha256:abcd1234");
		});
	}

	#[test]
	fn get_image_digest_fails_when_docker_not_running() {
		CommandMock::default().with_command("docker", 1).execute(|| {
			assert!(matches!(
				Docker::get_image_digest("test/image", "latest"),
				Err(Error::Docker(err)) if err == "Docker is not running."
			));
		});
	}

	#[test]
	fn get_image_digest_fails_when_image_cannot_be_pulled() {
		let command_mock = CommandMock::default();
		let docker_script = r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0
elif [ "$1" = "pull" ]; then
    exit 1
fi
exit 1"#;

		command_mock.with_command_script("docker", docker_script).execute(|| {
			assert!(matches!(
				Docker::get_image_digest("test/image", "nonexistent"),
				Err(Error::Docker(err)) if err.contains("Failed to pull image")
			));
		});
	}

	#[test]
	fn get_image_digest_pulls_and_fails_if_inspect_fails_after_pulling() {
		let command_mock = CommandMock::default();
		let pulled_marker = command_mock.fake_path().join("image_pulled");
		let docker_script = format!(
			r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0
elif [ "$1" = "pull" ]; then
    exit 0
elif [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
    if [ -f "{}" ]; then
        echo "[test/image@sha256:abcd1234]"
        exit 0
    else
        exit 1
    fi
fi
exit 1"#,
			pulled_marker.display()
		);

		command_mock.with_command_script("docker", &docker_script).execute(|| {
			assert!(matches!(Docker::get_image_digest("test/image", "latest"), Err(Error::Docker(err)) if err.contains("Failed to inspect image") && err.contains("after pulling")));
		});
	}

	#[test]
	fn get_image_digest_fails_when_output_has_no_at_symbol() {
		let command_mock = CommandMock::default();
		let docker_script = r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0
elif [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
    echo "[test/image-no-digest]"
    exit 0
fi
exit 1"#;

		command_mock.with_command_script("docker", docker_script).execute(|| {
			assert!(matches!(
				Docker::get_image_digest("test/image", "latest"),
				Err(Error::Docker(err)) if err == "Could not parse digest from docker output."
			));
		});
	}

	#[test]
	fn get_image_digest_fails_when_output_has_invalid_utf8() {
		let command_mock = CommandMock::default();
		let docker_script = r#"#!/bin/sh
if [ "$1" = "info" ]; then
    exit 0
elif [ "$1" = "image" ] && [ "$2" = "inspect" ]; then
    printf '\377\376'
    exit 0
fi
exit 1"#;

		command_mock.with_command_script("docker", docker_script).execute(|| {
			assert!(matches!(
				Docker::get_image_digest("test/image", "latest"),
				Err(Error::Docker(err)) if err.contains("Invalid UTF-8 in docker output")
			));
		});
	}

	#[tokio::test]
	async fn fetch_image_tag_succeeds() {
		let mut server = mockito::Server::new_async().await;
		let mock = server
			.mock("GET", "/")
			.with_status(200)
			.with_body("1.70.0\n")
			.create_async()
			.await;

		let result = fetch_image_tag(&server.url()).await;
		mock.assert_async().await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), "1.70.0");
	}

	#[tokio::test]
	async fn fetch_image_tag_fails_on_http_error() {
		let mut server = mockito::Server::new_async().await;
		let mock = server.mock("GET", "/").with_status(404).create_async().await;

		let result = fetch_image_tag(&server.url()).await;
		mock.assert_async().await;
		assert!(matches!(
			result,
			Err(Error::Docker(err)) if err.contains("Failed to fetch image tag") && err.contains("404")
		));
	}

	#[tokio::test]
	async fn fetch_image_tag_fails_on_network_error() {
		let result = fetch_image_tag("http://invalid-url-that-does-not-exist-12345.com").await;
		assert!(matches!(
			result,
			Err(Error::Docker(err)) if err.contains("Failed to fetch image tag")
		));
	}
}
