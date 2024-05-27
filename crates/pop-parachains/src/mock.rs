// SPDX-License-Identifier: GPL-3.0

// Mock the command that builds the parachain
pub fn cmd(_program: &str, _args: Vec<&str>) -> duct::Expression {
	duct::cmd!("echo")
}
