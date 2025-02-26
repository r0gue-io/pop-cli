use anyhow::Result;
use std::io::{self, Read, Seek, SeekFrom, Write};

#[cfg(unix)]
use nix::unistd::{close, dup, dup2};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(unix)]
use tempfile::tempfile;

#[cfg(windows)]
use std::os::windows::io::FromRawHandle;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, RawHandle};
#[cfg(windows)]
use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
#[cfg(windows)]
use windows::Win32::Storage::FileSystem::{
	CreateFileW, FILE_ATTRIBUTE_TEMPORARY, FILE_FLAG_DELETE_ON_CLOSE, GENERIC_READ, GENERIC_WRITE,
	OPEN_ALWAYS,
};
#[cfg(windows)]
use windows::Win32::System::Console::{GetStdHandle, SetStdHandle, STD_OUTPUT_HANDLE};

/// Wrapper function to capture stdout across platforms.
pub fn capture_stdout<F>(func: F) -> Result<String>
where
	F: FnOnce() -> Result<()>,
{
	// Ensure all buffered output is written.
	io::stdout().flush()?;

	#[cfg(unix)]
	return capture_stdout_unix(func);

	#[cfg(windows)]
	return capture_stdout_windows(func);
}

/// Unix-specific stdout capture using file descriptors.
#[cfg(unix)]
fn capture_stdout_unix<F>(func: F) -> Result<String>
where
	F: FnOnce() -> Result<()>,
{
	// Save original stdout.
	let stdout_fd: RawFd = io::stdout().as_raw_fd();
	let saved_fd = dup(stdout_fd)?;

	// Create a temporary file and redirect stdout to temp file.
	let mut temp_file = tempfile()?;
	dup2(temp_file.as_raw_fd(), stdout_fd)?;

	func()?;

	// Flush again to capture all output from running the function.
	io::stdout().flush()?;

	// Restore the original stdout and close the file descriptor.
	dup2(saved_fd, stdout_fd)?;
	close(saved_fd)?;

	// Read captured output.
	let mut output = String::new();
	temp_file.seek(SeekFrom::Start(0))?; // Seek to the beginning
	temp_file.read_to_string(&mut output)?;
	Ok(output)
}

/// Windows-specific stdout capture using `SetStdHandle`.
#[cfg(windows)]
fn capture_stdout_windows<F>(func: F) -> Result<String>
where
	F: FnOnce() -> Result<()>,
{
	// Save original stdout.
	let stdout_handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE).unwrap() };
	// Create a temporary file and redirect stdout to temp file.
	let temp_file = unsafe {
		CreateFileW(
			"temp_stdout.txt",
			GENERIC_READ | GENERIC_WRITE,
			0,
			std::ptr::null_mut(),
			OPEN_ALWAYS,
			FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE,
			None,
		)
	};
	if temp_file == INVALID_HANDLE_VALUE {
		return Err(anyhow::anyhow!("Failed to create temp file"));
	}
	let saved_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
	unsafe { SetStdHandle(STD_OUTPUT_HANDLE, temp_file) };

	func()?;

	// Flush again to capture all output from running the function.
	io::stdout().flush()?;
	// Restore the original stdout.
	unsafe { SetStdHandle(STD_OUTPUT_HANDLE, saved_stdout) };

	let mut file = unsafe { File::from_raw_handle(temp_file.0 as RawHandle) };
	let mut output = String::new();
	file.seek(SeekFrom::Start(0))?;
	file.read_to_string(&mut output)?;
	Ok(output)
}
