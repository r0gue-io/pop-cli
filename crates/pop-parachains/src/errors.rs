use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("a git error occurred: {0}")]
	Git(String),
	#[error("Failed to access the current directory")]
	CurrentDirAccess,
	#[error("Failed to locate the workspace")]
	WorkspaceLocate,
	#[error("Failed to create pallet directory")]
	PalletDirCreation,
}
