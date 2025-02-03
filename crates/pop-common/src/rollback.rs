// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use std::{
	fs::File,
	path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[derive(Debug)]
pub struct Rollback {
	// Keep the temp_files inside the rollback struct, so they live until the Rollback dissapears
	temp_files: Vec<NamedTempFile>,
	// A tuple of PathBuf representing the paths to the temporary file and the original file
	noted: Vec<(PathBuf, PathBuf)>,
	// A registry of new_files
	new_files: Vec<PathBuf>,
	// A registry of new directories
	new_dirs: Vec<PathBuf>,
}

impl Rollback {
	pub fn new() -> Self {
		Self {
			temp_files: Vec::new(),
			noted: Vec::new(),
			new_files: Vec::new(),
			new_dirs: Vec::new(),
		}
	}

	pub fn with_capacity(
		note_capacity: usize,
		new_files_capacity: usize,
		new_dirs_capacity: usize,
	) -> Self {
		Self {
			temp_files: Vec::with_capacity(note_capacity),
			noted: Vec::with_capacity(note_capacity),
			new_files: Vec::with_capacity(new_files_capacity),
			new_dirs: Vec::with_capacity(new_dirs_capacity),
		}
	}

	pub fn note_file(&mut self, original: &Path) -> Result<PathBuf, Error> {
		let temp_file = NamedTempFile::new()?;
		std::fs::write(&temp_file, &std::fs::read_to_string(original)?)?;
		let temp_file_path = temp_file.path().to_path_buf();
		self.temp_files.push(temp_file);
		self.noted.push((temp_file_path.clone(), original.to_path_buf()));
		Ok(temp_file_path)
	}

	pub fn new_file(&mut self, file: &Path) -> Result<(), Error> {
		File::create(file)?;
		self.new_files.push(file.to_path_buf());
		Ok(())
	}

	pub fn new_dir(&mut self, dir: &Path) -> Result<(), Error> {
		std::fs::create_dir(dir)?;
		self.new_dirs.push(dir.to_path_buf());
		Ok(())
	}

	pub fn noted_files(&self) -> Vec<PathBuf> {
		self.noted
			.iter()
			.map(|(_, original)| original.to_path_buf())
			.collect::<Vec<_>>()
	}

	pub fn new_files(&self) -> Vec<PathBuf> {
		self.new_files.clone()
	}

	pub fn new_dirs(&self) -> Vec<PathBuf> {
		self.new_dirs.clone()
	}

	pub fn commit(self) {
		self.noted.into_iter().for_each(|(temp, original)| {
			std::fs::write(
				original,
				std::fs::read_to_string(temp)
					.expect("The temp file exists as long as Self exists; qed"),
			)
			.expect("The original file exists; qed;")
		});
	}

	pub fn rollback(self) {
		self.new_files.into_iter().for_each(|file| {
			std::fs::remove_file(&file).expect("The file exists cause it's in the rollback; qed;")
		});
		self.new_dirs.into_iter().for_each(|dir| {
			std::fs::remove_dir_all(&dir).expect("Thee dir exists cause it's in the rollback; qed;")
		});
	}

	pub fn ok_or_rollback<S, E>(self, result: Result<S, E>) -> Result<(Self, S), E> {
		match result {
			Ok(result) => Ok((self, result)),
			Err(err) => {
				self.rollback();
				Err(err)
			},
		}
	}
}
