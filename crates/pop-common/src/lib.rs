pub mod errors;
pub mod git;
pub mod helpers;

pub use errors::Error;
pub use git::{Git, GitHub, Release};
pub use helpers::{get_project_name_from_path, replace_in_file};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
