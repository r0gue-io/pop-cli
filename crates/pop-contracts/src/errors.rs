use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to create new contract project: {0}")]
    NewContractFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}