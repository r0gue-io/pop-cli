// SPDX-License-Identifier: GPL-3.0

//! Pop-fork integration tests for RPC server method groups.

#![cfg(feature = "integration-tests")]

mod common;

#[test]
fn pop_fork_rpc_server_archive() {
	common::run_group("pop-fork-rpc-archive", common::run_rpc_server_archive_tests());
}

#[test]
fn pop_fork_rpc_server_author() {
	common::run_group("pop-fork-rpc-author", common::run_rpc_server_author_tests());
}

#[test]
fn pop_fork_rpc_server_chain() {
	common::run_group("pop-fork-rpc-chain", common::run_rpc_server_chain_tests());
}

#[test]
fn pop_fork_rpc_server_chain_head() {
	common::run_group("pop-fork-rpc-chain-head", common::run_rpc_server_chain_head_tests());
}

#[test]
fn pop_fork_rpc_server_chain_spec() {
	common::run_group("pop-fork-rpc-chain-spec", common::run_rpc_server_chain_spec_tests());
}

#[test]
fn pop_fork_rpc_server_state() {
	common::run_group("pop-fork-rpc-state", common::run_rpc_server_state_tests());
}

#[test]
fn pop_fork_rpc_server_system() {
	common::run_group("pop-fork-rpc-system", common::run_rpc_server_system_tests());
}
