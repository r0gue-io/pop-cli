// SPDX-License-Identifier: GPL-3.0

//! Pop-fork integration tests for core runtime and storage subsystems.

#![cfg(feature = "integration-tests")]

mod common;

#[test]
fn pop_fork_block() {
	common::run_group("pop-fork-block", common::run_block_tests());
}

#[test]
fn pop_fork_blockchain_basics() {
	common::run_group("pop-fork-blockchain-basics", common::run_blockchain_basics_tests());
}

#[test]
fn pop_fork_blockchain_queries() {
	common::run_group("pop-fork-blockchain-queries", common::run_blockchain_queries_tests());
}

#[test]
fn pop_fork_blockchain_runtime_call() {
	common::run_group(
		"pop-fork-blockchain-runtime-call",
		common::run_blockchain_runtime_call_tests(),
	);
}

#[test]
fn pop_fork_blockchain_extrinsic() {
	common::run_group("pop-fork-blockchain-extrinsic", common::run_blockchain_extrinsic_tests());
}

#[test]
fn pop_fork_builder() {
	common::run_group("pop-fork-builder", common::run_builder_tests());
}

#[test]
fn pop_fork_executor() {
	common::run_group("pop-fork-executor", common::run_executor_tests());
}

#[test]
fn pop_fork_local() {
	common::run_group("pop-fork-local", common::run_local_tests());
}

#[test]
fn pop_fork_remote() {
	common::run_group("pop-fork-remote", common::run_remote_tests());
}

#[test]
fn pop_fork_rpc_client() {
	common::run_group("pop-fork-rpc", common::run_rpc_tests());
}

#[test]
fn pop_fork_timestamp() {
	common::run_group("pop-fork-timestamp", common::run_timestamp_tests());
}
