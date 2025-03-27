// SPDX-License-Identifier: GPL-3.0

use assert_cmd::Command;
use similar::{ChangeTag, TextDiff};

#[test]
fn pop_add_to_runtime_pallet_runtime_macro_v2_works() {
	let temp = tempfile::tempdir().unwrap();
	let tempdir = temp.path();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&tempdir)
		.args(&["new", "parachain", "test_parachain", "-t", "r0gue-io/base-parachain"])
		.assert()
		.success();

	let test_parachain = tempdir.join("test_parachain");
	let runtime_path = test_parachain.join("runtime");

	let manifest_path = runtime_path.join("Cargo.toml");
	let runtime_lib_path = runtime_path.join("src").join("lib.rs");
	let pallet_configs_path = runtime_path.join("src").join("configs");
	let pallet_configs_mod_path = pallet_configs_path.join("mod.rs");
	let contracts_pallet_config_path = pallet_configs_path.join("contracts.rs");

	assert!(!contracts_pallet_config_path.exists());

	let runtime_lib_content_before = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let pallet_configs_mod_content_before =
		std::fs::read_to_string(&pallet_configs_mod_path).unwrap();
	let manifest_content_before = std::fs::read_to_string(&manifest_path).unwrap();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&["add-to", "runtime", "pallet", "-p", "contracts"])
		.assert()
		.success();

	let runtime_lib_content_after = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let pallet_configs_mod_content_after =
		std::fs::read_to_string(&pallet_configs_mod_path).unwrap();
	let manifest_content_after = std::fs::read_to_string(&manifest_path).unwrap();
	let contracts_pallet_config_content =
		std::fs::read_to_string(&contracts_pallet_config_path).unwrap();

	let runtime_lib_diff =
		TextDiff::from_lines(&runtime_lib_content_before, &runtime_lib_content_after);
	let pallet_configs_mod_diff =
		TextDiff::from_lines(&pallet_configs_mod_content_before, &pallet_configs_mod_content_after);
	let manifest_diff = TextDiff::from_lines(&manifest_content_before, &manifest_content_after);

	let expected_inserted_lines_runtime_lib = vec![
		"\n",
		"    #[runtime::pallet_index(34)]\n",
		"    pub type Contracts = pallet_contracts;\n",
	];
	let expected_inserted_lines_configs_mod = vec!["mod contracts;\n"];
	let expected_inserted_lines_manifest =
		vec!["pallet-contracts = { version = \"27.0.0\", default-features = false }\n"];

	let mut inserted_lines_runtime_lib = Vec::with_capacity(3);
	let mut inserted_lines_configs_mod = Vec::with_capacity(1);
	let mut inserted_lines_manifest = Vec::with_capacity(1);

	for change in runtime_lib_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_runtime_lib.push(change.value()),
			_ => (),
		}
	}

	for change in pallet_configs_mod_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_configs_mod.push(change.value()),
			_ => (),
		}
	}

	for change in manifest_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_manifest.push(change.value()),
			_ => (),
		}
	}

	assert_eq!(expected_inserted_lines_runtime_lib, inserted_lines_runtime_lib);
	assert_eq!(expected_inserted_lines_configs_mod, inserted_lines_configs_mod);
	assert_eq!(expected_inserted_lines_manifest, inserted_lines_manifest);

	assert_eq!(
		contracts_pallet_config_content,
		r#"use crate::Balances;
parameter_types! {
    pub Schedule : pallet_contracts::Schedule < Runtime > = Default::default();
}

#[derive_impl(pallet_contracts::config_preludes::TestDefaultConfig)]
impl pallet_contracts::Config for Runtime {
    type Currency = Balances;
    type Schedule = [pallet_contracts::Frame<Self>; 5];
    type CallStack = Schedule;
}
"#
	);
}

#[test]
fn pop_add_to_runtime_pallet_construct_runtime_works() {
	let temp = tempfile::tempdir().unwrap();
	let tempdir = temp.path();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&tempdir)
		.args(&[
			"new",
			"parachain",
			"test_parachain2",
			"openzeppelin",
			"-t",
			"openzeppelin/generic-template",
			"-r",
			"v2.0.3",
		])
		.assert()
		.success();

	let test_parachain = tempdir.join("test_parachain2");
	let runtime_path = test_parachain.join("runtime");

	let manifest_path = runtime_path.join("Cargo.toml");
	let runtime_lib_path = runtime_path.join("src").join("lib.rs");

	let runtime_lib_content_before = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let manifest_content_before = std::fs::read_to_string(&manifest_path).unwrap();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&[
			"add-to",
			"runtime",
			"pallet",
			"-p",
			"contracts",
		])
		.assert()
		.success();

	let runtime_lib_content_after = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let manifest_content_after = std::fs::read_to_string(&manifest_path).unwrap();

	let manifest_diff = TextDiff::from_lines(&manifest_content_before, &manifest_content_after);

	let expected_inserted_lines_manifest =
		vec!["pallet-contracts = { version = \"27.0.0\", default-features = false }\n"];

	let mut inserted_lines_manifest = Vec::with_capacity(1);

	for change in manifest_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_manifest.push(change.value()),
			_ => (),
		}
	}

	assert_eq!(expected_inserted_lines_manifest, inserted_lines_manifest);

	// Unparsing the AST with construct_runtime is a bit unpredictable due to the well-known issue
	// of formatting a macro invocation AST, so the assertions we can do are limited. Let's just
	// state that pallet_contracts have been added.
	assert!(!runtime_lib_content_before.contains("pallet_contracts"));
	assert!(runtime_lib_content_after.contains("pallet_contracts"));
}

#[test]
fn pop_add_to_runtime_pallet_doesnt_modify_on_failure() {
	let temp = tempfile::tempdir().unwrap();
	let tempdir = temp.path();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&tempdir)
		.args(&["new", "parachain", "test_parachain3", "-t", "r0gue-io/contracts-parachain"])
		.assert()
		.success();

	let test_parachain = tempdir.join("test_parachain3");
	let runtime_path = test_parachain.join("runtime");

	let manifest_path = runtime_path.join("Cargo.toml");
	let runtime_lib_path = runtime_path.join("src").join("lib.rs");
	let pallet_configs_mod_path = runtime_path.join("src").join("configs").join("mod.rs");

	let runtime_lib_content_before = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let pallet_configs_mod_content_before =
		std::fs::read_to_string(&pallet_configs_mod_path).unwrap();
	let manifest_content_before = std::fs::read_to_string(&manifest_path).unwrap();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&["add-to", "runtime", "pallet", "-p", "contracts"])
		.assert()
		.failure()
		.stderr(predicates::str::contains("Error: contracts is already in use."));

	let runtime_lib_content_after = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let pallet_configs_mod_content_after =
		std::fs::read_to_string(&pallet_configs_mod_path).unwrap();
	let manifest_content_after = std::fs::read_to_string(&manifest_path).unwrap();

	assert_eq!(runtime_lib_content_before, runtime_lib_content_after);
	assert_eq!(pallet_configs_mod_content_before, pallet_configs_mod_content_after);
	assert_eq!(manifest_content_before, manifest_content_after);
}
