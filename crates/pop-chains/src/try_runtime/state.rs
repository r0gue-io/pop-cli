// SPDX-License-Identifier: GPL-3.0

use serde::Serialize;
use std::path::PathBuf;
use strum::{Display, EnumDiscriminants};
use strum_macros::{AsRefStr, EnumMessage, EnumString, VariantArray};

/// The runtime *state*.
#[derive(Clone, Debug, clap::Subcommand, EnumDiscriminants, Serialize)]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumMessage, VariantArray, Display))]
#[strum_discriminants(name(StateCommand))]
pub enum State {
	/// A live chain.
	#[strum_discriminants(strum(
		serialize = "live",
		message = "Live",
		detailed_message = "Run the migrations on top of live state.",
	))]
	Live(LiveState),

	/// A state snapshot.
	#[strum_discriminants(strum(
		serialize = "snap",
		message = "Snapshot",
		detailed_message = "Run the migrations on top of a chain snapshot."
	))]
	Snap {
		/// Path to the snapshot file.
		#[serde(skip_serializing)]
		#[clap(short = 'p', long = "path", alias = "snapshot-path")]
		path: Option<PathBuf>,
	},
}

/// A `Live` variant for [`State`]
#[derive(Debug, Default, Clone, clap::Args, Serialize)]
pub struct LiveState {
	/// The url to connect to.
	#[arg(
		short,
		long,
		value_parser = super::parse::url,
	)]
	pub uri: Option<String>,

	/// The block hash at which to fetch the state.
	///
	/// If not provided the latest finalised head is used.
	#[arg(
		short,
		long,
		value_parser = super::parse::hash,
	)]
	pub at: Option<String>,

	/// A pallet to scrape. Can be provided multiple times. If empty, entire chain state will
	/// be scraped.
	///
	/// This is equivalent to passing `xx_hash_64(pallet)` to `--hashed_prefixes`.
	#[arg(short, long, num_args = 1..)]
	pub pallet: Vec<String>,

	/// Storage entry key prefixes to scrape and inject into the test externalities. Pass as 0x
	/// prefixed hex strings. By default, all keys are scraped and included.
	#[arg(long = "prefix", value_parser = super::parse::hash, num_args = 1..)]
	pub hashed_prefixes: Vec<String>,

	/// Fetch the child-keys.
	///
	/// Default is `false`, if `--pallets` are specified, `true` otherwise. In other
	/// words, if you scrape the whole state the child tree data is included out of the box.
	/// Otherwise, it must be enabled explicitly using this flag.
	#[arg(long)]
	pub child_tree: bool,
}
