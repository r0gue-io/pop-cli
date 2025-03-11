// SPDX-License-Identifier: GPL-3.0
use std::path::PathBuf;

/// This method is used to get the proper project path format (with or without cli flag)
pub fn get_project_path(path_flag: Option<PathBuf>, path_pos: Option<PathBuf>) -> Option<PathBuf> {
	let project_path = if let Some(ref path) = path_pos {
		Some(path) // Use positional path if present
	} else {
		path_flag.as_ref() // Otherwise, use the named path
	};
	project_path.cloned()
}
