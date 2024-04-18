use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("a git error occurred: {0}")]
	Git(String),
}
