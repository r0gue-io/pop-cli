// SPDX-License-Identifier: GPL-3.0

use assert_cmd::Command;
use similar::{ChangeTag, TextDiff};

#[test]
fn pop_add_to_pallet_config_type_works() {
	let temp = tempfile::tempdir().unwrap();
	let tempdir = temp.path();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&tempdir)
		.args(&["new", "parachain", "test_parachain", "-t", "r0gue-io/base-parachain"])
		.assert()
		.success();

	let test_parachain = tempdir.join("test_parachain");

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&["new", "pallet", "template", "advanced", "-c", "runtime-event", "-d"])
		.assert()
		.success();

	let pallet_path = test_parachain.join("template");
	let pallet_lib_path = pallet_path.join("src").join("lib.rs");
	let pallet_mock_path = pallet_path.join("src").join("mock.rs");
	let pallet_impl_path =
		test_parachain.join("runtime").join("src").join("configs").join("template.rs");
	let config_preludes_path = pallet_path.join("src").join("config_preludes.rs");

	let lib_content_before_new_type = std::fs::read_to_string(&pallet_lib_path).unwrap();
	let mock_content_before_new_type = std::fs::read_to_string(&pallet_mock_path).unwrap();
	let pallet_impl_content_before_new_type = std::fs::read_to_string(&pallet_impl_path).unwrap();
	let config_preludes_path_content_before_new_type =
		std::fs::read_to_string(&config_preludes_path).unwrap();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&pallet_path)
		.args(&["add-to", "pallet", "config-type", "-t", "fungibles"])
		.assert()
		.success();

	let lib_content_after_new_type = std::fs::read_to_string(&pallet_lib_path).unwrap();
	let mock_content_after_new_type = std::fs::read_to_string(&pallet_mock_path).unwrap();
	let pallet_impl_content_after_new_type = std::fs::read_to_string(&pallet_impl_path).unwrap();
	let config_preludes_path_content_after_new_type =
		std::fs::read_to_string(&config_preludes_path).unwrap();

	let lib_diff = TextDiff::from_lines(&lib_content_before_new_type, &lib_content_after_new_type);
	let mock_diff =
		TextDiff::from_lines(&mock_content_before_new_type, &mock_content_after_new_type);
	let pallet_impl_diff = TextDiff::from_lines(
		&pallet_impl_content_before_new_type,
		&pallet_impl_content_after_new_type,
	);
	let config_preludes_diff = TextDiff::from_lines(
		&config_preludes_path_content_before_new_type,
		&config_preludes_path_content_after_new_type,
	);

	let expected_lib_inserted_lines = vec![
		"        #[pallet::no_default]\n",
		"        type Fungibles: fungible::Inspect<Self::AccountId>\n",
		"            + fungible::Mutate<Self::AccountId>\n",
		"            + fungible::hold::Inspect<Self::AccountId>\n",
		"            + fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>\n",
		"            + fungible::freeze::Inspect<Self::AccountId>\n",
		"            + fungible::freeze::Mutate<Self::AccountId>;\n",
		"        /// A reason for placing a hold on funds\n",
		"        #[pallet::no_default_bounds]\n",
		"        type RuntimeHoldReason: From<HoldReason>;\n",
		"        /// A reason for placing a freeze on funds\n",
		"        #[pallet::no_default_bounds]\n",
		"        type RuntimeFreezeReason: VariantCount;\n",
		"    /// A reason for the pallet placing a hold on funds.\n",
		"    #[pallet::composite_enum]\n",
		"    pub enum HoldReason {\n",
		"        /// Some hold reason\n",
		"        #[codec(index = 0)]\n",
		"        SomeHoldReason,\n",
		"    }\n",
		"use frame::traits::fungible;\n",
		"use frame::traits::VariantCount;\n",
	];

	let expected_mock_inserted_lines = vec![
		"impl pallet_template::Config for Test {\n",
		"    type Fungibles = Balances;\n",
		"}\n",
	];

	let expected_mock_deleted_lines = vec!["impl pallet_template::Config for Test {}\n"];

	let expected_impl_type_inserted_lines = vec![
		"impl pallet_template::Config for Runtime {\n",
		"    type Fungibles = Balances;\n",
		"}\n",
	];

	let expected_impl_type_deleted_lines = vec!["impl pallet_template::Config for Runtime {}\n"];

	// There's 4 default configs (Testchain, solochain, relaychain, parachain), so the changes are
	// applied four times each.
	let expected_config_preludes_inserted_lines = vec![
		"    #[inject_runtime_type]\n",
		"    type RuntimeHoldReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeHoldReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeHoldReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeHoldReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeFreezeReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeFreezeReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeFreezeReason = ();\n",
		"    #[inject_runtime_type]\n",
		"    type RuntimeFreezeReason = ();\n",
	];

	let mut lib_inserted_lines = Vec::with_capacity(18);
	let mut mock_inserted_lines = Vec::with_capacity(1);
	let mut mock_deleted_lines = Vec::with_capacity(3);
	let mut pallet_impl_inserted_lines = Vec::with_capacity(1);
	let mut pallet_impl_deleted_lines = Vec::with_capacity(3);
	let mut config_preludes_inserted_lines = Vec::with_capacity(16);

	for change in lib_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("No deletions in lib expected"),
			ChangeTag::Insert => lib_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	for change in mock_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => mock_deleted_lines.push(change.value()),
			ChangeTag::Insert => mock_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	for change in pallet_impl_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => pallet_impl_deleted_lines.push(change.value()),
			ChangeTag::Insert => pallet_impl_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	for change in config_preludes_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("No deletions in lib expected"),
			ChangeTag::Insert => config_preludes_inserted_lines.push(change.value()),
			_ => (),
		}
	}

	assert_eq!(expected_lib_inserted_lines, lib_inserted_lines);
	assert_eq!(expected_mock_inserted_lines, mock_inserted_lines);
	assert_eq!(expected_mock_deleted_lines, mock_deleted_lines);
	assert_eq!(expected_impl_type_inserted_lines, pallet_impl_inserted_lines);
	assert_eq!(expected_impl_type_deleted_lines, pallet_impl_deleted_lines);
	assert_eq!(expected_config_preludes_inserted_lines, config_preludes_inserted_lines);
}

#[test]
fn pop_add_to_pallet_config_type_doesnt_modify_on_fail() {
	let temp = tempfile::tempdir().unwrap();
	let tempdir = temp.path();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&tempdir)
		.args(&["new", "parachain", "test_parachain", "-t", "r0gue-io/base-parachain"])
		.assert()
		.success();

	let test_parachain = tempdir.join("test_parachain");

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&["new", "pallet", "template", "advanced", "-c", "runtime-event", "-d"])
		.assert()
		.success();

	let pallet_path = test_parachain.join("template");
	let pallet_lib_path = pallet_path.join("src").join("lib.rs");
	let pallet_mock_path = pallet_path.join("src").join("mock.rs");
	let pallet_impl_path =
		test_parachain.join("runtime").join("src").join("configs").join("template.rs");
	let config_preludes_path = pallet_path.join("src").join("config_preludes.rs");

	let lib_content_before_new_type = std::fs::read_to_string(&pallet_lib_path).unwrap();
	let mock_content_before_new_type = std::fs::read_to_string(&pallet_mock_path).unwrap();
	let pallet_impl_content_before_new_type = std::fs::read_to_string(&pallet_impl_path).unwrap();
	let config_preludes_path_content_before_new_type =
		std::fs::read_to_string(&config_preludes_path).unwrap();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&pallet_path)
		.args(&["add-to", "pallet", "config-type", "-t", "fungibles", "runtime-event"])
		.assert()
		.failure()
		.stderr(predicates::str::contains("Error: RuntimeEvent is already in use."));

	let lib_content_after_new_type = std::fs::read_to_string(&pallet_lib_path).unwrap();
	let mock_content_after_new_type = std::fs::read_to_string(&pallet_mock_path).unwrap();
	let pallet_impl_content_after_new_type = std::fs::read_to_string(&pallet_impl_path).unwrap();
	let config_preludes_path_content_after_new_type =
		std::fs::read_to_string(&config_preludes_path).unwrap();

	assert_eq!(lib_content_before_new_type, lib_content_after_new_type);
	assert_eq!(mock_content_before_new_type, mock_content_after_new_type);
	assert_eq!(pallet_impl_content_before_new_type, pallet_impl_content_after_new_type);
	assert_eq!(
		config_preludes_path_content_before_new_type,
		config_preludes_path_content_after_new_type
	);
}
