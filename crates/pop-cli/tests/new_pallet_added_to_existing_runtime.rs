// SPDX-License-Identifier: GPL-3.0

use assert_cmd::Command;
use similar::{ChangeTag, TextDiff};

#[test]
fn pop_new_pallet_modifies_runtime() {
	let temp = tempfile::tempdir().unwrap();
	let tempdir = temp.path();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&tempdir)
		.args(&["new", "parachain", "test_parachain", "-t", "standard"])
		.assert()
		.success();

	let test_parachain = tempdir.join("test_parachain");

	let workspace_manifest_path = test_parachain.join("Cargo.toml");

	let runtime_path = test_parachain.join("runtime");
	let runtime_lib_path = runtime_path.join("src").join("lib.rs");
	let configs_path = runtime_path.join("src").join("configs");
	let configs_mod_path = configs_path.join("mod.rs");
	let configs_pallet_path = configs_path.join("template.rs");

	let workspace_content_before = std::fs::read_to_string(&workspace_manifest_path).unwrap();
	let runtime_lib_content_before = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let configs_mod_content_before = std::fs::read_to_string(&configs_mod_path).unwrap();

	assert!(!configs_pallet_path.exists());

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&["new", "pallet", "template", "advanced", "-c", "runtime-event", "-d"])
		.assert()
		.success();

	let workspace_content_after = std::fs::read_to_string(&workspace_manifest_path).unwrap();
	let runtime_lib_content_after = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let configs_mod_content_after = std::fs::read_to_string(&configs_mod_path).unwrap();
	let configs_pallet_content = std::fs::read_to_string(&configs_pallet_path).unwrap();

	let runtime_lib_diff =
		TextDiff::from_lines(&runtime_lib_content_before, &runtime_lib_content_after);
	let workspace_diff = TextDiff::from_lines(&workspace_content_before, &workspace_content_after);
	let configs_mod_diff =
		TextDiff::from_lines(&configs_mod_content_before, &configs_mod_content_after);

	let expected_runtime_lib_inserted_lines = vec![
		"\n",
		"    #[runtime::pallet_index(34)]\n",
		"    pub type Template = pallet_template;\n",
	];

	let expected_workspace_inserted_lines =
		vec!["members = [\"node\", \"runtime\", \"template\"]\n"];

	let expected_workspace_deleted_lines = vec!["members = [\"node\", \"runtime\"]\n"];

	let expected_configs_mod_inserted_lines = vec!["mod template;\n"];

	let mut runtime_lib_inserted_lines = Vec::with_capacity(3);
	let mut workspace_inserted_lines = Vec::with_capacity(1);
	let mut workspace_deleted_lines = Vec::with_capacity(1);
	let mut configs_mod_inserted_lines = Vec::with_capacity(1);

	for change in runtime_lib_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("No deletions in lib expected"),
			ChangeTag::Insert => runtime_lib_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	for change in workspace_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => workspace_deleted_lines.push(change.value()),
			ChangeTag::Insert => workspace_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	for change in configs_mod_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("No deletions expected"),
			ChangeTag::Insert => configs_mod_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	assert_eq!(expected_runtime_lib_inserted_lines, runtime_lib_inserted_lines);
	assert_eq!(expected_workspace_inserted_lines, workspace_inserted_lines);
	assert_eq!(expected_workspace_deleted_lines, workspace_deleted_lines);
	assert_eq!(expected_configs_mod_inserted_lines, configs_mod_inserted_lines);

	assert_eq!(configs_pallet_content, "impl pallet_template::Config for Runtime {}\n");
}
