pub mod build;
pub mod errors;
pub mod git;
pub mod helpers;
pub mod manifest;
pub mod sourcing;
pub mod templates;

pub use build::Profile;
pub use errors::Error;
pub use git::{Git, GitHub, Release};
pub use helpers::{get_project_name_from_path, replace_in_file};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Trait for observing status updates.
pub trait Status {
	/// Update the observer with the provided `status`.
	fn update(&self, status: &str);
}

impl Status for () {
	// no-op: status updates are ignored
	fn update(&self, _: &str) {}
}
