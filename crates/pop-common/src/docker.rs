// SPDX-License-Identifier: GPL-3.0

use std::process::Command;
use std::io::ErrorKind;
use crate::Error;

pub enum DockerStatus{
    NotInstalled,
    Installed,
    Running
}

impl DockerStatus{
    pub fn detect() -> Result<Self, Error>{
        match Command::new("docker").arg("info").output(){
            Ok(output) if output.status.success() => Ok(DockerStatus::Running),
            Ok(_) => Ok(DockerStatus::Installed),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(DockerStatus::NotInstalled),
            Err(err) => Err(Error::Docker(err.to_string()))
        }
    }
}