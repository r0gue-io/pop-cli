// SPDX-License-Identifier: GPL-3.0

//! Reusable RPC test scenarios shared by integration and unit tests.

/// archive_* RPC scenarios.
pub mod archive;
/// author_* RPC scenarios.
pub mod author;
/// block tests migrated from integration helpers.
pub mod block;
/// blockchain tests migrated from integration helpers.
pub mod blockchain;
/// block builder tests migrated from integration helpers.
pub mod builder;
/// chain_* RPC scenarios.
pub mod chain;
/// chainHead_* RPC scenarios.
pub mod chain_head;
/// chainSpec_* RPC scenarios.
pub mod chain_spec;
/// runtime executor tests migrated from integration helpers.
pub mod executor;
/// local storage layer tests migrated from integration helpers.
pub mod local;
/// remote storage layer tests migrated from integration helpers.
pub mod remote;
/// RPC client tests migrated from integration helpers.
pub mod rpc;
/// state_* RPC scenarios.
pub mod state;
/// system_* RPC scenarios.
pub mod system;
/// timestamp/slot tests migrated from integration helpers.
pub mod timestamp;
