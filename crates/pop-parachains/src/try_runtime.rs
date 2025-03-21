use strum_macros::{AsRefStr, EnumMessage, EnumString, VariantArray};

/// Chain state options for testing the runtime migrations.
#[derive(AsRefStr, Clone, Debug, EnumString, EnumMessage, VariantArray, Eq, PartialEq)]
pub enum Migration {
	/// Run the migrations of a given runtime on top of a live state.
	#[strum(
		serialize = "live",
		message = "Live",
		detailed_message = "Run the migrations of a given runtime on top of a live state."
	)]
	Live,
	/// Run the migrations of a given runtime on top of a chain snapshot.
	#[strum(
		serialize = "snapshot",
		message = "Live",
		detailed_message = "Run the migrations of a given runtime on top of a chain snapshot."
	)]
	Snapshot,
}
