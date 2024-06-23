pub mod errors;
pub mod git;

pub use errors::Error;
pub use git::{Git, GitHub, Release};
static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
