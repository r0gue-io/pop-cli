use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempdir::TempDir;
#[test]
fn add_parachain_pallet_template() {
	let temp_dir = TempDir::new("add-pallet-test").unwrap();
	let output = temp_dir.path().join("test_lib.rs");
	fs::copy(PathBuf::from("tests/add/lib.rs"), &output).unwrap();
	Command::cargo_bin("pop")
		.unwrap()
		.args(&["add", "template", "-r", "test_lib.rs"])
		.current_dir(&temp_dir)
		.assert()
		.success();
	let contents : String = fs::read_to_string(&output).unwrap();
	assert!(contents.contains("pub use pallet_parachain_template;"));
	assert!(contents.contains("impl pallet_parachain_template::Config for Runtime {"));
	assert!(contents.contains("Template: pallet_parachain_template"));
}
