// SPDX-License-Identifier: GPL-3.0

//! String constants for the builder module.

/// Runtime API method names used during block building.
pub mod runtime_api {
	/// Runtime method to initialize a new block.
	///
	/// Called with the encoded header to set up block execution context.
	/// This prepares the runtime state for applying extrinsics.
	pub const CORE_INITIALIZE_BLOCK: &str = "Core_initialize_block";

	/// Runtime method to apply an extrinsic to the block.
	///
	/// Called for both inherent and user extrinsics.
	/// Returns a dispatch result indicating success or failure.
	pub const BLOCK_BUILDER_APPLY_EXTRINSIC: &str = "BlockBuilder_apply_extrinsic";

	/// Runtime method to finalize the block.
	///
	/// Called after all extrinsics have been applied.
	/// Returns the final block header with computed roots.
	pub const BLOCK_BUILDER_FINALIZE_BLOCK: &str = "BlockBuilder_finalize_block";
}
