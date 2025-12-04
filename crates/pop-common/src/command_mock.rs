// SPDX-License-Identifier: GPL-3.0

use std::{os::unix::fs::PermissionsExt, path::Path};
use tempfile::TempDir;

/// Helper to create a script that exits with a given code
fn exit_script(exit_code: i32) -> String {
	format!("#!/bin/sh\nexit {}", exit_code)
}

pub struct CommandMock {
	temp_dir: TempDir,
}

impl Default for CommandMock {
	fn default() -> Self {
		Self { temp_dir: tempfile::tempdir().unwrap() }
	}
}

impl CommandMock {
	pub fn fake_path(&self) -> &Path {
		self.temp_dir.path()
	}

	/// Create a fake command that exits with the given code
	pub fn with_command(self, command_name: &str, exit_code: i32) -> Self {
		let fake_command_path = self.temp_dir.path().join(command_name);
		let script = exit_script(exit_code);
		std::fs::write(&fake_command_path, script).unwrap();
		Self::set_executable(&fake_command_path).unwrap();
		self
	}

	/// Create a fake command with custom script content
	pub fn with_command_script(self, command_name: &str, script: &str) -> Self {
		let fake_command_path = self.temp_dir.path().join(command_name);
		std::fs::write(&fake_command_path, script).unwrap();
		Self::set_executable(&fake_command_path).unwrap();
		self
	}

	/// Create a fake command without execute permissions
	pub fn with_non_permissioned_command(self, command_name: &str) -> Self {
		let fake_command_path = self.temp_dir.path().join(command_name);
		let script = exit_script(0);
		std::fs::write(&fake_command_path, script).unwrap();
		self
	}

	/// Execute the test with mocked commands in PATH
	pub fn execute<F>(self, test: F)
	where
		F: FnOnce(),
	{
		temp_env::with_var("PATH", Some(self.temp_dir.path()), test);
	}

	fn set_executable(path: &Path) -> std::io::Result<()> {
		let permissions = std::fs::Permissions::from_mode(0o755);
		std::fs::set_permissions(path, permissions)
	}
}
