//! An embedded database meant to be used as a temporary cache to look up
//! and store pop cli args such as commonly used file paths.
//!
//! DB functions should be as error-free as possible.
//! The caching is meant to improve the pop experience and should not impact usage
//! All errors are non-fatal and meant to be discarded unless logged at debug or error log target.
//!
//! This database is implemented using the [sled] crate, which provides an
//! in-memory LSM tree. The keys are paths as `String`s, and the values are
//! the absolute path to the file as a `String`.

use log::{debug, error};
use sled::{Config, Db};
use std::path::{Path, PathBuf};

const DB_PATH: &'static str = "/tmp/pop-cli-db";

/// The idea is to provide a default path to use based on the previous invocation of `pop new parachain`
/// to make following commands more convinient to use by allowing the user to omit path arguments
/// such as `-p <path>` or `-r <runtime path>`. This should not effect existing functionality meaning
/// For any pop subcommand, the first priority is the user specified path argument. In the absence of that
/// the second natural place to look is the current working directory and identify it as a parachain.
/// If the current working directory is not detected as a parachain, only then PopDb maybe consulted
///
/// Usage:
/// First open (or initialize) it : `PopDb::open_or_init()`.
/// On `pop new parachain` cache the dir path by calling `pop_db.set_parachain_path(path)`
/// On `pop new contract` cache the dir path by calling `pop_db.set_contract_path(path)`
///
/// Use these paths for subcommands like `pop build | test | add | new <pallet>`
/// when invoked outside of the parachain directory
pub struct PopDb {
	/// May be None due to an error in `open_or_init`
	inner: Option<Db>,
}
impl PopDb {
	/// Try to open a database or create one if it doesn't exist: `Some(db)`.
	/// If errors occur, run pop-cli without a database: `None`.
	pub(crate) fn open_or_init() -> Self {
		let db_path = Path::new(DB_PATH);
		if !db_path.exists() {
			debug!("{} does not exist, creating database", db_path.display());
			let _ = std::fs::create_dir_all(db_path).map_err(|err| {
				error!(
					"Failed to create database directory {}\nDue to : {}",
					db_path.display(),
					err
				)
			});
		}
		Self {
			inner: Config::new()
				.path(DB_PATH)
				// Set cache capacity to 10 MB
				.cache_capacity(10 * 1024 * 1024)
				.open()
				.map_err(|err| error!("Failed to open database\nDue to : {}", err))
				.ok(),
		}
	}

	/// Set parachain path
	pub(crate) fn set_parachain_path(&self, path: &Path) {
		if let Some(ref db) = self.inner {
			let path = match path.canonicalize() {
				Ok(p) => p,
				Err(err) => {
					error!("Failed to canonicalize {}\nDue to : {}", path.display(), err);
					return;
				},
			};
			let _ = db
				.insert(b"parachain", path.to_string_lossy().as_bytes())
				.map_err(|err| error!("Failed to set key parachain in database\nDue to : {}", err));
		}
	}
	/// Get most recent parachain path
	pub(crate) fn get_parachain_path(&self) -> Option<PathBuf> {
		if let Some(ref db) = self.inner {
			if let Some(Some(parachain_path)) = db.get(b"parachain").ok() {
				return Some(PathBuf::from(String::from_utf8_lossy(&parachain_path).to_string()));
			}
		}
		None
	}
}
