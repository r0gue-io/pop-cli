// SPDX-License-Identifier: GPL-3.0

use assert_cmd::Command;
use predicates::prelude::*;

struct TestBuilder {
	cmd: Command,
}

impl TestBuilder {
	fn cmd_add_pallet_config_type() -> Self {
		let mut cmd = Command::cargo_bin("pop").unwrap();
		cmd.arg("add")
			.arg("pallet")
			.arg("path/to/pallet")
			.arg("config")
			.arg("MyType")
			.arg("--bounds")
			.arg("MyBound");
		Self { cmd }
	}

	fn cmd_success(&mut self) {
		self.cmd.assert().success();
	}

	fn cmd_fails_with_message(&mut self, message: &str) {
		self.cmd.assert().failure().stderr(predicate::str::contains(message));
	}
}

#[test]
fn config_options_no_default_custom_validation_works() {
	// --no-default + --default-value isn't a valid combination
	let mut builder = TestBuilder::cmd_add_pallet_config_type();
	builder.cmd.arg("--no-default").arg("--default-value").arg("u32");

	builder.cmd_fails_with_message("Cannot specify a default value for a no-default config type.");

	// --no-default without --runtime-value isn't a valid combination
	builder = TestBuilder::cmd_add_pallet_config_type();
	builder.cmd.arg("--no-default");
	builder.cmd_fails_with_message("Types without a default value need a runtime value.");

	// --no-default + --runtime-value OK
	builder.cmd.arg("--runtime-value").arg("u32");
	builder.cmd_success();
}

#[test]
fn config_options_no_default_bounds_validation_works() {
	// --no-default-bounds fails without runtime value and default value
	let mut builder = TestBuilder::cmd_add_pallet_config_type();
	builder.cmd.arg("--no-default-bounds");
	builder.cmd_fails_with_message("The type needs at least a default value or a runtime value.");

	// --no-default-bounds + --runtime-value works
	builder.cmd.arg("--runtime-value").arg("u32");
	builder.cmd_success();

	// --no-default-bounds + --default-value works
	builder = TestBuilder::cmd_add_pallet_config_type();
	builder.cmd.arg("--no-default-bounds").arg("--default-value").arg("u32");
	builder.cmd_success();

	// --no-default-bounds + --default-value + --runtime-value works
	builder.cmd.arg("--runtime-value").arg("u32");
	builder.cmd_success();
}

#[test]
fn config_options_default_not_specified_validation_works() {
	// default not specified fails without runtime value and default value
	let mut builder = TestBuilder::cmd_add_pallet_config_type();
	builder.cmd_fails_with_message("The type needs at least a default value or a runtime value.");

	// default not specified + --runtime-value works
	builder.cmd.arg("--runtime-value").arg("u32");
	builder.cmd_success();

	// default not specified + --default-value works
	builder = TestBuilder::cmd_add_pallet_config_type();
	builder.cmd.arg("--default-value").arg("u32");
	builder.cmd_success();

	// default not specified + --default-value + --runtime-value works
	builder.cmd.arg("--runtime-value").arg("u32");
	builder.cmd_success();
}
