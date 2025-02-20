// This file is copied and adapted from srtool-cli (https://github.com/chevdor/srtool-cli)
// Original source: https://github.com/chevdor/srtool-cli/blob/master/cli/src/container_engine.rs

use crate::Error;

use std::{fmt::Display, process::Command};

#[derive(Clone, Copy, PartialEq)]
pub enum ContainerEngine {
	Docker,
	Podman,
}

impl ContainerEngine {
	/// Check whether you have Podman and/or Docker installed. The default will be Podman if both are present.
	pub fn detect() -> Result<ContainerEngine, Error> {
		if let Ok(engine) = std::env::var("ENGINE") {
			return ContainerEngine::try_from(engine.as_str());
		}

		let podman_output: Option<std::process::Output> = Command::new("podman").arg("--version").output().ok();
		if let Some(podman) = podman_output {
			let podman = String::from_utf8_lossy(&podman.stdout);
			if podman.to_lowercase().contains("podman") {
				return Ok(ContainerEngine::Podman);
			} else if podman.contains("docker") {
				println!("WARNING: You have podman symlinked to docker. This is strange :)");
				return Ok(ContainerEngine::Docker);
			}
		}

		let docker_output = Command::new("docker").arg("--version").output().ok();
		if let Some(docker) = docker_output {
			let docker = String::from_utf8_lossy(&docker.stdout);
			if docker.to_lowercase().contains("docker") {
				return Ok(ContainerEngine::Docker);
			} else if docker.contains("podman") {
				return Ok(ContainerEngine::Podman);
			}
		}

		Err(Error::UnknownContainerEngine(None))
	}
}
impl TryFrom<&str> for ContainerEngine {
	type Error = Error;

	fn try_from(s: &str) -> Result<Self, Self::Error> {
		match s.to_ascii_lowercase().as_str() {
			"auto" => Self::detect(),
			"podman" => Ok(ContainerEngine::Podman),
			"docker" => Ok(ContainerEngine::Docker),
			_ => Err(Error::UnknownContainerEngine(Some(s.into()))),
		}
	}
}

impl Display for ContainerEngine {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ContainerEngine::Docker => write!(f, "docker"),
			ContainerEngine::Podman => write!(f, "podman"),
		}
	}
}