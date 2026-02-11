// SPDX-License-Identifier: GPL-3.0

//! Pop-fork integration scenarios grouped by subsystem.

#![cfg(feature = "integration-tests")]

use pop_fork::{
	rpc_server::test_scenarios::{
		archive as rpc_server_archive, author as rpc_server_author, block, blockchain, builder,
		chain, chain_head as rpc_server_chain_head, chain_spec, executor, local, remote, rpc,
		state as rpc_server_state, system as rpc_server_system, timestamp,
	},
	testing::TestContext,
};
use std::future::Future;

async fn run_block_tests() {
	block::fork_point_with_hash_creates_block_with_correct_metadata().await;
	block::fork_point_with_non_existent_hash_returns_error().await;
	block::fork_point_with_number_creates_block_with_correct_metadata().await;
	block::fork_point_with_non_existent_number_returns_error().await;
	block::child_creates_block_with_correct_metadata().await;
	block::child_commits_parent_storage().await;
	block::child_storage_inherits_parent_modifications().await;
}

async fn run_blockchain_tests() {
	blockchain::fork_creates_blockchain_with_correct_fork_point().await;
	blockchain::fork_at_creates_blockchain_at_specific_block().await;
	blockchain::fork_with_invalid_endpoint_fails().await;
	blockchain::fork_at_with_invalid_block_number_fails().await;
	blockchain::fork_detects_relay_chain_type().await;
	blockchain::fork_retrieves_chain_name().await;
	blockchain::build_empty_block_advances_chain().await;
	blockchain::build_multiple_empty_blocks_creates_chain().await;
	blockchain::storage_returns_value_for_existing_key().await;
	blockchain::storage_returns_none_for_nonexistent_key().await;
	blockchain::storage_at_queries_specific_block().await;
	blockchain::call_executes_runtime_api().await;
	blockchain::head_returns_current_block().await;
	blockchain::head_updates_after_building_block().await;
	blockchain::build_block_with_signed_transfer_updates_balances().await;
	blockchain::block_body_returns_extrinsics_for_head().await;
	blockchain::block_body_returns_extrinsics_for_parent_block().await;
	blockchain::block_body_returns_extrinsics_for_fork_point_from_remote().await;
	blockchain::block_body_returns_none_for_unknown_hash().await;
	blockchain::block_header_returns_header_for_head().await;
	blockchain::block_header_returns_header_for_different_blocks().await;
	blockchain::block_header_returns_header_for_fork_point().await;
	blockchain::block_header_returns_none_for_unknown_hash().await;
	blockchain::block_header_returns_header_for_historical_block().await;
	blockchain::block_hash_at_returns_hash_for_head().await;
	blockchain::block_hash_at_returns_hash_for_parent_block().await;
	blockchain::block_hash_at_returns_hash_for_fork_point().await;
	blockchain::block_hash_at_returns_hash_for_block_before_fork_point().await;
	blockchain::block_hash_at_returns_none_for_future_block().await;
	blockchain::block_number_by_hash_returns_number_for_head().await;
	blockchain::block_number_by_hash_returns_number_for_parent().await;
	blockchain::block_number_by_hash_returns_number_for_fork_point().await;
	blockchain::block_number_by_hash_returns_none_for_unknown().await;
	blockchain::block_number_by_hash_returns_number_for_historical_block().await;
	blockchain::call_at_block_executes_at_head().await;
	blockchain::call_at_block_executes_at_fork_point().await;
	blockchain::call_at_block_executes_at_parent_block().await;
	blockchain::call_at_block_returns_none_for_unknown_hash().await;
	blockchain::call_at_block_executes_at_historical_block().await;
	blockchain::call_at_block_does_not_persist_storage().await;
	blockchain::validate_extrinsic_accepts_valid_transfer().await;
	blockchain::validate_extrinsic_rejects_garbage().await;
	blockchain::build_block_result_tracks_included_extrinsics().await;
	blockchain::build_block_result_tracks_failed_extrinsics().await;
}

async fn run_builder_tests() {
	builder::new_creates_builder_with_empty_extrinsics().await;
	builder::initialize_succeeds_and_modifies_storage().await;
	builder::initialize_twice_fails().await;
	builder::apply_inherents_without_providers_returns_empty().await;
	builder::apply_inherents_before_initialize_fails().await;
	builder::apply_extrinsic_before_initialize_fails().await;
	builder::finalize_before_initialize_fails().await;
	builder::apply_inherents_twice_fails().await;
	builder::apply_extrinsic_before_inherents_fails().await;
	builder::finalize_before_inherents_fails().await;
	builder::finalize_produces_child_block().await;
	builder::create_next_header_increments_block_number().await;
	builder::create_next_header_includes_digest_items().await;
}

async fn run_executor_tests() {
	executor::core_version_executes_successfully().await;
	executor::metadata_executes_successfully().await;
	executor::with_config_applies_custom_settings().await;
	executor::logs_are_captured_during_execution().await;
	executor::core_initialize_block_modifies_storage().await;
	executor::storage_reads_from_accumulated_changes().await;
	executor::storage_changes_persist_across_calls().await;
	executor::runtime_version_extracts_version_info().await;
}

async fn run_local_tests_part_1() {
	local::new_creates_empty_layer().await;
	local::get_returns_local_modification().await;
	local::get_non_existent_block_returns_none().await;
	local::get_returns_none_for_deleted_prefix_if_exact_key_not_found().await;
	local::get_returns_some_for_deleted_prefix_if_exact_key_found_after_deletion().await;
	local::get_falls_back_to_parent().await;
	local::get_local_overrides_parent().await;
	local::get_returns_none_for_nonexistent_key().await;
	local::get_retrieves_modified_value_from_fork_history().await;
	local::get_retrieves_unmodified_value_from_remote_at_past_forked_block().await;
	local::get_historical_block().await;
	local::set_stores_value().await;
	local::set_overwrites_previous_value().await;
	local::get_batch_empty_keys().await;
	local::get_batch_returns_local_modifications().await;
	local::get_batch_returns_none_for_deleted_prefix().await;
	local::get_batch_falls_back_to_parent().await;
	local::get_batch_local_overrides_parent().await;
	local::get_batch_mixed_sources().await;
	local::get_batch_maintains_order().await;
	local::get_batch_retrieves_modified_value_from_fork_history().await;
	local::get_batch_retrieves_unmodified_value_from_remote_at_past_forked_block().await;
	local::get_batch_historical_block().await;
	local::get_batch_non_existent_block_returns_none().await;
	local::get_batch_mixed_block_scenarios().await;
	local::set_batch_empty_entries().await;
	local::set_batch_stores_multiple_values().await;
	local::set_batch_with_deletions().await;
	local::set_batch_overwrites_previous_values().await;
	local::set_batch_duplicate_keys_last_wins().await;
	local::delete_prefix_removes_matching_keys().await;
	local::delete_prefix_blocks_parent_reads().await;
	local::delete_prefix_adds_to_deleted_prefixes().await;
	local::delete_prefix_with_empty_prefix().await;
	local::is_deleted_returns_false_initially().await;
	local::is_deleted_returns_true_after_delete().await;
	local::is_deleted_exact_match_only().await;
	local::diff_returns_empty_initially().await;
	local::diff_returns_all_modifications().await;
	local::diff_includes_deletions().await;
	local::diff_excludes_prefix_deleted_keys().await;
}

async fn run_local_tests_part_2() {
	local::commit_writes_to_cache().await;
	local::commit_preserves_modifications().await;
	local::commit_with_deletions().await;
	local::commit_empty_modifications().await;
	local::commit_multiple_times().await;
	local::commit_validity_ranges_work_properly().await;
	local::next_key_returns_next_key_from_parent().await;
	local::next_key_returns_none_when_no_more_keys().await;
	local::next_key_skips_deleted_prefix().await;
	local::next_key_skips_multiple_deleted_keys().await;
	local::next_key_returns_none_when_all_remaining_deleted().await;
	local::next_key_with_empty_prefix().await;
	local::next_key_with_nonexistent_prefix().await;
	local::metadata_at_returns_metadata_for_future_blocks().await;
	local::metadata_at_fetches_from_remote_for_pre_fork_blocks().await;
	local::register_metadata_version_adds_new_version().await;
	local::register_metadata_version_respects_block_boundaries().await;
	local::has_code_changed_at_returns_false_when_no_code_modified().await;
	local::has_code_changed_at_returns_false_for_non_code_modifications().await;
	local::has_code_changed_at_returns_true_when_code_modified().await;
	local::has_code_changed_at_returns_false_for_different_block().await;
	local::has_code_changed_at_tracks_modification_block_correctly().await;
}

async fn run_remote_tests() {
	remote::get_fetches_and_caches().await;
	remote::get_caches_empty_values().await;
	remote::get_batch_fetches_mixed().await;
	remote::get_batch_uses_cache().await;
	remote::prefetch_prefix().await;
	remote::layer_is_cloneable().await;
	remote::accessor_methods().await;
	remote::fetch_and_cache_block_by_number_caches_block().await;
	remote::fetch_and_cache_block_by_number_non_existent().await;
	remote::fetch_and_cache_block_by_number_multiple_blocks().await;
	remote::fetch_and_cache_block_by_number_idempotent().await;
	remote::fetch_and_cache_block_by_number_verifies_parent_chain().await;
}

async fn run_rpc_tests() {
	rpc::connect_to_node().await;
	rpc::fetch_finalized_head().await;
	rpc::fetch_header().await;
	rpc::fetch_storage().await;
	rpc::fetch_metadata().await;
	rpc::fetch_runtime_code().await;
	rpc::fetch_storage_keys_paged().await;
	rpc::fetch_storage_batch().await;
	rpc::fetch_system_chain().await;
	rpc::fetch_system_properties().await;
	rpc::connect_to_invalid_endpoint_fails().await;
	rpc::fetch_header_non_existent_block_fails().await;
	rpc::fetch_storage_non_existent_key_returns_none().await;
	rpc::fetch_storage_batch_with_mixed_keys().await;
	rpc::fetch_storage_batch_empty_keys().await;
}

async fn run_rpc_server_archive_tests() {
	let ctx = TestContext::for_rpc_server().await;
	let ws_url = ctx.ws_url();
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";

	let expected_head = ctx.blockchain().head_number().await;
	rpc_server_archive::archive_finalized_height_returns_correct_value_at(&ws_url, expected_head)
		.await;
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	rpc_server_archive::archive_finalized_height_returns_correct_value_at(
		&ws_url,
		expected_head + 1,
	)
	.await;

	let expected_genesis = format!(
		"0x{}",
		hex::encode(
			ctx.blockchain()
				.block_hash_at(0)
				.await
				.expect("genesis hash query should succeed")
				.expect("genesis hash should exist")
				.as_bytes()
		)
	);
	rpc_server_archive::archive_genesis_hash_returns_valid_hash_at(&ws_url, &expected_genesis)
		.await;

	let block_1 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let block_2 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let fork_height = ctx.blockchain().fork_point_number();
	rpc_server_archive::archive_hash_by_height_returns_hash_at_height_at(
		&ws_url,
		fork_height,
		&format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes())),
	)
	.await;
	rpc_server_archive::archive_hash_by_height_returns_hash_at_height_at(
		&ws_url,
		block_1.number,
		&format!("0x{}", hex::encode(block_1.hash.as_bytes())),
	)
	.await;
	rpc_server_archive::archive_hash_by_height_returns_hash_at_height_at(
		&ws_url,
		block_2.number,
		&format!("0x{}", hex::encode(block_2.hash.as_bytes())),
	)
	.await;
	rpc_server_archive::archive_hash_by_height_returns_none_for_unknown_height_at(
		&ws_url,
		999_999_999u32,
	)
	.await;

	let fork_hash = format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes()));
	rpc_server_archive::archive_header_returns_header_for_hash_at(&ws_url, &fork_hash).await;
	rpc_server_archive::archive_header_returns_none_for_unknown_hash_at(&ws_url, unknown_hash)
		.await;
	let parent = ctx.blockchain().build_empty_block().await.expect("block build should work");
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	rpc_server_archive::archive_header_returns_header_for_hash_at(
		&ws_url,
		&format!("0x{}", hex::encode(parent.hash.as_bytes())),
	)
	.await;
	rpc_server_archive::archive_header_is_idempotent_for_hash_at(
		&ws_url,
		&format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes())),
	)
	.await;

	let fork_body =
		rpc_server_archive::archive_body_returns_extrinsics_for_hash_at(&ws_url, &fork_hash).await;
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	let head_body =
		rpc_server_archive::archive_body_returns_extrinsics_for_hash_at(&ws_url, &head_hash).await;
	assert_ne!(fork_body, head_body);
	rpc_server_archive::archive_body_is_idempotent_for_hash_at(&ws_url, &head_hash).await;
	rpc_server_archive::archive_body_returns_none_for_unknown_hash_at(&ws_url, unknown_hash).await;

	rpc_server_archive::archive_call_executes_runtime_api_at(
		&ws_url,
		&head_hash,
		"Core_version",
		"0x",
	)
	.await;
	rpc_server_archive::archive_call_returns_error_for_invalid_function_at(
		&ws_url,
		&head_hash,
		"NonExistent_function",
		"0x",
	)
	.await;
	rpc_server_archive::archive_call_returns_null_for_unknown_block_at(
		&ws_url,
		unknown_hash,
		"Core_version",
		"0x",
	)
	.await;
	rpc_server_archive::archive_call_executes_runtime_api_at(
		&ws_url,
		&fork_hash,
		"Core_version",
		"0x",
	)
	.await;
	rpc_server_archive::archive_call_rejects_invalid_hex_hash_at(
		&ws_url,
		"not_valid_hex",
		"Core_version",
		"0x",
	)
	.await;

	let mut system_number_key = Vec::new();
	system_number_key.extend(sp_core::twox_128(b"System"));
	system_number_key.extend(sp_core::twox_128(b"Number"));
	let system_number_key_hex = format!("0x{}", hex::encode(system_number_key));
	rpc_server_archive::archive_storage_returns_value_for_existing_key_at(
		&ws_url,
		&head_hash,
		&system_number_key_hex,
	)
	.await;
	rpc_server_archive::archive_storage_returns_none_for_nonexistent_key_at(
		&ws_url,
		&head_hash,
		&format!("0x{}", hex::encode(b"nonexistent_key_12345")),
	)
	.await;
	rpc_server_archive::archive_header_rejects_invalid_hex_at(&ws_url, "not_valid_hex").await;
	rpc_server_archive::archive_call_rejects_invalid_hex_parameters_at(
		&ws_url,
		&head_hash,
		"Core_version",
	)
	.await;
	let head = ctx.blockchain().head().await;
	let head_hash_for_init = format!("0x{}", hex::encode(head.hash.as_bytes()));
	let header_bytes = pop_fork::create_next_header(
		&head,
		vec![pop_fork::DigestItem::PreRuntime(
			pop_fork::consensus_engine::AURA,
			0u64.to_le_bytes().to_vec(),
		)],
	);
	rpc_server_archive::archive_call_does_not_persist_storage_changes_at(
		&ws_url,
		&head_hash_for_init,
		&format!("0x{}", hex::encode(header_bytes)),
		&system_number_key_hex,
	)
	.await;
	rpc_server_archive::archive_storage_returns_hash_when_requested_at(
		&ws_url,
		&head_hash,
		&system_number_key_hex,
	)
	.await;
	let block_for_storage_1 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let block1_hash = format!("0x{}", hex::encode(block_for_storage_1.hash.as_bytes()));
	let block_for_storage_2 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let block2_hash = format!("0x{}", hex::encode(block_for_storage_2.hash.as_bytes()));
	rpc_server_archive::archive_storage_queries_at_specific_block_at(
		&ws_url,
		&block1_hash,
		&block2_hash,
		&system_number_key_hex,
	)
	.await;
	rpc_server_archive::archive_storage_returns_error_for_unknown_block_at(
		&ws_url,
		unknown_hash,
		"0x1234",
	)
	.await;

	let modified_key = b"test_storage_diff_key";
	let modified_key_hex = format!("0x{}", hex::encode(modified_key));
	ctx.blockchain().set_storage_for_testing(modified_key, Some(b"value1")).await;
	let modified_block1 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let modified_block1_hash = format!("0x{}", hex::encode(modified_block1.hash.as_bytes()));
	ctx.blockchain().set_storage_for_testing(modified_key, Some(b"value2")).await;
	let modified_block2 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let modified_block2_hash = format!("0x{}", hex::encode(modified_block2.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_detects_modified_value_at(
		&ws_url,
		&modified_block2_hash,
		&modified_block1_hash,
		&modified_key_hex,
		&format!("0x{}", hex::encode(b"value2")),
	)
	.await;

	let unchanged_key = b"test_unchanged_key";
	let unchanged_key_hex = format!("0x{}", hex::encode(unchanged_key));
	ctx.blockchain()
		.set_storage_for_testing(unchanged_key, Some(b"constant_value"))
		.await;
	let unchanged_block1 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let unchanged_block1_hash = format!("0x{}", hex::encode(unchanged_block1.hash.as_bytes()));
	let unchanged_block2 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let unchanged_block2_hash = format!("0x{}", hex::encode(unchanged_block2.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_returns_empty_for_unchanged_keys_at(
		&ws_url,
		&unchanged_block2_hash,
		&unchanged_block1_hash,
		&unchanged_key_hex,
	)
	.await;

	let added_key = b"test_added_key";
	let added_key_hex = format!("0x{}", hex::encode(added_key));
	let added_block1 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let added_block1_hash = format!("0x{}", hex::encode(added_block1.hash.as_bytes()));
	ctx.blockchain().set_storage_for_testing(added_key, Some(b"new_value")).await;
	let added_block2 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let added_block2_hash = format!("0x{}", hex::encode(added_block2.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_returns_added_for_new_key_at(
		&ws_url,
		&added_block2_hash,
		&added_block1_hash,
		&added_key_hex,
		&format!("0x{}", hex::encode(b"new_value")),
	)
	.await;

	let deleted_key = b"test_deleted_key";
	let deleted_key_hex = format!("0x{}", hex::encode(deleted_key));
	ctx.blockchain()
		.set_storage_for_testing(deleted_key, Some(b"will_be_deleted"))
		.await;
	let deleted_block1 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let deleted_block1_hash = format!("0x{}", hex::encode(deleted_block1.hash.as_bytes()));
	ctx.blockchain().set_storage_for_testing(deleted_key, None).await;
	let deleted_block2 =
		ctx.blockchain().build_empty_block().await.expect("block build should work");
	let deleted_block2_hash = format!("0x{}", hex::encode(deleted_block2.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_returns_deleted_for_removed_key_at(
		&ws_url,
		&deleted_block2_hash,
		&deleted_block1_hash,
		&deleted_key_hex,
	)
	.await;

	let hash_key = b"test_hash_key";
	let hash_key_hex = format!("0x{}", hex::encode(hash_key));
	ctx.blockchain().set_storage_for_testing(hash_key, Some(b"value1")).await;
	let hash_block1 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let hash_block1_hash = format!("0x{}", hex::encode(hash_block1.hash.as_bytes()));
	let new_hash_value = b"value2";
	ctx.blockchain().set_storage_for_testing(hash_key, Some(new_hash_value)).await;
	let hash_block2 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let hash_block2_hash = format!("0x{}", hex::encode(hash_block2.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_returns_hash_when_requested_at(
		&ws_url,
		&hash_block2_hash,
		&hash_block1_hash,
		&hash_key_hex,
		&format!("0x{}", hex::encode(sp_core::blake2_256(new_hash_value))),
	)
	.await;

	let valid_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	rpc_server_archive::archive_storage_diff_returns_error_for_unknown_hash_at(
		&ws_url,
		unknown_hash,
		&valid_hash,
		"0x1234",
	)
	.await;
	rpc_server_archive::archive_storage_diff_returns_error_for_unknown_previous_hash_at(
		&ws_url,
		&valid_hash,
		unknown_hash,
		"0x1234",
	)
	.await;

	let parent_key = b"test_parent_key";
	let parent_key_hex = format!("0x{}", hex::encode(parent_key));
	ctx.blockchain()
		.set_storage_for_testing(parent_key, Some(b"parent_value"))
		.await;
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	ctx.blockchain().set_storage_for_testing(parent_key, Some(b"child_value")).await;
	let child_block = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let child_hash = format!("0x{}", hex::encode(child_block.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_uses_parent_when_previous_hash_omitted_at(
		&ws_url,
		&child_hash,
		&parent_key_hex,
		&format!("0x{}", hex::encode(b"child_value")),
	)
	.await;

	let multi_added = b"test_multi_added";
	let multi_modified = b"test_multi_modified";
	let multi_deleted = b"test_multi_deleted";
	let multi_unchanged = b"test_multi_unchanged";
	ctx.blockchain()
		.set_storage_for_testing(multi_modified, Some(b"old_value"))
		.await;
	ctx.blockchain()
		.set_storage_for_testing(multi_deleted, Some(b"to_delete"))
		.await;
	ctx.blockchain()
		.set_storage_for_testing(multi_unchanged, Some(b"constant"))
		.await;
	let multi_block1 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let multi_block1_hash = format!("0x{}", hex::encode(multi_block1.hash.as_bytes()));
	ctx.blockchain().set_storage_for_testing(multi_added, Some(b"new_key")).await;
	ctx.blockchain()
		.set_storage_for_testing(multi_modified, Some(b"new_value"))
		.await;
	ctx.blockchain().set_storage_for_testing(multi_deleted, None).await;
	let multi_block2 = ctx.blockchain().build_empty_block().await.expect("block build should work");
	let multi_block2_hash = format!("0x{}", hex::encode(multi_block2.hash.as_bytes()));
	rpc_server_archive::archive_storage_diff_handles_multiple_items_at(
		&ws_url,
		&multi_block2_hash,
		&multi_block1_hash,
		&format!("0x{}", hex::encode(multi_added)),
		&format!("0x{}", hex::encode(multi_modified)),
		&format!("0x{}", hex::encode(multi_deleted)),
		&format!("0x{}", hex::encode(multi_unchanged)),
	)
	.await;
}

async fn run_rpc_server_author_tests() {
	let config = pop_fork::ExecutorConfig {
		signature_mock: pop_fork::SignatureMockMode::AlwaysValid,
		..Default::default()
	};
	let ctx = TestContext::for_rpc_server_with_config(config).await;
	let ws_url = ctx.ws_url();
	let ext_hex = rpc_server_author::build_transfer_extrinsic_hex(ctx.blockchain()).await;
	let expected_hash = format!(
		"0x{}",
		hex::encode(sp_core::blake2_256(
			&hex::decode(ext_hex.trim_start_matches("0x")).expect("extrinsic hex should decode")
		))
	);

	rpc_server_author::author_submit_extrinsic_returns_correct_hash_at(
		&ws_url,
		&ext_hex,
		&expected_hash,
	)
	.await;
	rpc_server_author::author_pending_extrinsics_empty_after_submit_at(&ws_url, &ext_hex).await;
	rpc_server_author::author_submit_extrinsic_invalid_hex_at(&ws_url).await;
	rpc_server_author::author_submit_and_watch_sends_lifecycle_events_at(&ws_url, &ext_hex).await;

	let invalid_ctx = pop_fork::testing::TestContextBuilder::new().with_server().build().await;
	let invalid_ws = invalid_ctx.ws_url();
	let garbage_hex = "0xdeadbeef";
	rpc_server_author::author_submit_extrinsic_rejects_garbage_with_error_code_at(
		&invalid_ws,
		garbage_hex,
	)
	.await;
	rpc_server_author::author_submit_extrinsic_does_not_build_block_on_validation_failure_at(
		&invalid_ws,
		garbage_hex,
	)
	.await;
	rpc_server_author::author_submit_and_watch_sends_invalid_on_validation_failure_at(
		&invalid_ws,
		garbage_hex,
	)
	.await;
}

async fn run_rpc_server_chain_tests() {
	let ctx = TestContext::for_rpc_server().await;
	ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let head_number = ctx.blockchain().head_number().await;
	let expected_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	chain::chain_get_block_hash_returns_head_hash(&ctx.ws_url(), head_number, &expected_hash).await;
	chain::chain_get_block_hash_without_number_returns_head_hash(&ctx.ws_url(), &expected_hash)
		.await;
	chain::chain_get_block_hash_returns_none_hash(&ctx.ws_url(), head_number + 999).await;
	let fork_point_number = ctx.blockchain().fork_point_number();
	let fork_hash = format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes()));
	chain::chain_get_block_hash_returns_head_hash(&ctx.ws_url(), fork_point_number, &fork_hash)
		.await;

	if fork_point_number > 0 {
		let historical_number = fork_point_number - 1;
		let historical_hash = ctx
			.blockchain()
			.block_hash_at(historical_number)
			.await
			.expect("query should succeed")
			.expect("historical hash should exist");
		chain::chain_get_block_hash_returns_head_hash(
			&ctx.ws_url(),
			historical_number,
			&format!("0x{}", hex::encode(historical_hash.as_bytes())),
		)
		.await;
	}

	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));
	let parent_hash_hex = format!("0x{}", hex::encode(block.parent_hash.as_bytes()));
	let number_hex = format!("0x{:x}", block.number);
	let expected_extrinsics = block
		.extrinsics
		.iter()
		.map(|ext| format!("0x{}", hex::encode(ext)))
		.collect::<Vec<_>>();

	chain::chain_get_header_returns_valid_header(
		&ctx.ws_url(),
		&block_hash_hex,
		&number_hex,
		&parent_hash_hex,
	)
	.await;
	chain::chain_get_header_returns_head_when_no_hash(&ctx.ws_url(), &number_hex).await;
	chain::chain_get_header_returns_number(
		&ctx.ws_url(),
		&format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes())),
		&format!("0x{:x}", ctx.blockchain().fork_point_number()),
	)
	.await;
	chain::chain_get_block_returns_full_block(
		&ctx.ws_url(),
		&block_hash_hex,
		&number_hex,
		&parent_hash_hex,
		&expected_extrinsics,
	)
	.await;
	chain::chain_get_block_returns_head_when_no_hash(&ctx.ws_url(), &number_hex).await;
	chain::chain_get_finalized_head_returns_head_hash(&ctx.ws_url(), &expected_hash).await;

	let new_block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	chain::chain_get_finalized_head_returns_head_hash(
		&ctx.ws_url(),
		&format!("0x{}", hex::encode(new_block.hash.as_bytes())),
	)
	.await;
}

async fn run_rpc_server_chain_head_tests() {
	let ctx = TestContext::for_rpc_server().await;
	rpc_server_chain_head::follow_returns_subscription_and_initialized_event_at(&ctx.ws_url())
		.await;
	rpc_server_chain_head::header_returns_header_for_valid_subscription_at(&ctx.ws_url()).await;
	rpc_server_chain_head::invalid_subscription_returns_error_at(&ctx.ws_url()).await;
}

async fn run_rpc_server_chain_spec_tests() {
	let ctx = TestContext::for_rpc_server().await;
	chain_spec::chain_spec_chain_name_returns_string(&ctx.ws_url(), "ink-node").await;
	let _ = chain_spec::chain_spec_genesis_hash_returns_valid_hex_hash(&ctx.ws_url(), None).await;
	chain_spec::chain_spec_genesis_hash_matches_archive(&ctx.ws_url()).await;
	chain_spec::chain_spec_properties_returns_json_or_null(&ctx.ws_url(), None).await;
}

async fn run_rpc_server_state_tests() {
	let ctx = TestContext::for_rpc_server().await;
	let key_hex = format!(
		"0x{}",
		hex::encode(pop_fork::testing::helpers::account_storage_key(
			&pop_fork::testing::accounts::ALICE,
		))
	);
	let block = ctx.blockchain().build_empty_block().await.expect("block build should work");
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));
	let head_hash_hex = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));

	rpc_server_state::state_get_storage_returns_value_at(&ctx.ws_url(), &key_hex).await;
	rpc_server_state::state_get_storage_at_block_hash_at(&ctx.ws_url(), &key_hex, &block_hash_hex)
		.await;
	rpc_server_state::state_get_storage_returns_none_for_nonexistent_key_at(&ctx.ws_url()).await;
	rpc_server_state::state_get_metadata_returns_metadata_at(&ctx.ws_url()).await;
	rpc_server_state::state_get_metadata_at_block_hash_at(&ctx.ws_url(), &head_hash_hex).await;
	rpc_server_state::state_get_runtime_version_returns_version_at(&ctx.ws_url()).await;
	rpc_server_state::state_get_runtime_version_at_block_hash_at(&ctx.ws_url(), &head_hash_hex)
		.await;
	rpc_server_state::state_get_storage_invalid_hex_returns_error_at(&ctx.ws_url()).await;
	rpc_server_state::state_get_storage_invalid_block_hash_returns_error_at(
		&ctx.ws_url(),
		&key_hex,
	)
	.await;
}

async fn run_rpc_server_system_tests() {
	let ctx = TestContext::for_rpc_server().await;
	rpc_server_system::chain_works_at(&ctx.ws_url(), "ink-node").await;
	rpc_server_system::name_works_at(&ctx.ws_url(), "pop-fork").await;
	rpc_server_system::version_works_at(&ctx.ws_url(), "1.0.0").await;
	rpc_server_system::health_works_at(&ctx.ws_url()).await;
	rpc_server_system::properties_returns_json_or_null_at(&ctx.ws_url()).await;
	rpc_server_system::account_next_index_returns_nonce_at(&ctx.ws_url()).await;
	rpc_server_system::account_next_index_returns_zero_for_nonexistent_at(&ctx.ws_url()).await;
	rpc_server_system::account_next_index_invalid_address_returns_error_at(&ctx.ws_url()).await;
}

async fn run_timestamp_tests() {
	timestamp::get_slot_duration_falls_back_when_aura_api_unavailable().await;
	timestamp::get_slot_duration_from_live_aura_chain().await;
	timestamp::get_slot_duration_from_live_babe_chain().await;
}

fn run_group<F>(thread_name: &str, scenario_group: F)
where
	F: Future<Output = ()> + Send + 'static,
{
	let handle = std::thread::Builder::new()
		.name(thread_name.to_string())
		.stack_size(64 * 1024 * 1024)
		.spawn(move || {
			let runtime = tokio::runtime::Builder::new_multi_thread()
				.enable_all()
				.build()
				.expect("tokio runtime should build");
			runtime.block_on(scenario_group);
		})
		.expect("integration thread should spawn");

	handle.join().expect("integration thread should complete");
}

#[test]
fn pop_fork_block_scenarios() {
	run_group("pop-fork-block", run_block_tests());
}

#[test]
fn pop_fork_blockchain_scenarios() {
	run_group("pop-fork-blockchain", run_blockchain_tests());
}

#[test]
fn pop_fork_builder_scenarios() {
	run_group("pop-fork-builder", run_builder_tests());
}

#[test]
fn pop_fork_executor_scenarios() {
	run_group("pop-fork-executor", run_executor_tests());
}

#[test]
fn pop_fork_local_scenarios_part_1() {
	run_group("pop-fork-local-1", run_local_tests_part_1());
}

#[test]
fn pop_fork_local_scenarios_part_2() {
	run_group("pop-fork-local-2", run_local_tests_part_2());
}

#[test]
fn pop_fork_remote_scenarios() {
	run_group("pop-fork-remote", run_remote_tests());
}

#[test]
fn pop_fork_rpc_client_scenarios() {
	run_group("pop-fork-rpc", run_rpc_tests());
}

#[test]
fn pop_fork_rpc_server_archive_scenarios() {
	run_group("pop-fork-rpc-archive", run_rpc_server_archive_tests());
}

#[test]
fn pop_fork_rpc_server_author_scenarios() {
	run_group("pop-fork-rpc-author", run_rpc_server_author_tests());
}

#[test]
fn pop_fork_rpc_server_chain_scenarios() {
	run_group("pop-fork-rpc-chain", run_rpc_server_chain_tests());
}

#[test]
fn pop_fork_rpc_server_chain_head_scenarios() {
	run_group("pop-fork-rpc-chain-head", run_rpc_server_chain_head_tests());
}

#[test]
fn pop_fork_rpc_server_chain_spec_scenarios() {
	run_group("pop-fork-rpc-chain-spec", run_rpc_server_chain_spec_tests());
}

#[test]
fn pop_fork_rpc_server_state_scenarios() {
	run_group("pop-fork-rpc-state", run_rpc_server_state_tests());
}

#[test]
fn pop_fork_rpc_server_system_scenarios() {
	run_group("pop-fork-rpc-system", run_rpc_server_system_tests());
}

#[test]
fn pop_fork_timestamp_scenarios() {
	run_group("pop-fork-timestamp", run_timestamp_tests());
}
