use assert_cmd::Command;
use std::fs;
use std::path::PathBuf;
use tempdir::TempDir;

#[ignore = "TomlEditor expects to find a parachain project structure initialized with git"]
#[test]
fn add_parachain_pallet_template() {
	let temp_dir = TempDir::new("add-pallet-test").unwrap();
	let output = temp_dir.path().join("test_lib.rs");
	let source_file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/add/lib.rs");
	println!("source file : {:?}", source_file);
	fs::copy(&source_file, &output).unwrap();
	Command::cargo_bin("pop")
		.unwrap()
		.args(&["add", "pallet", "template", "-r", "test_lib.rs"])
		.current_dir(&temp_dir)
		.assert()
		.success();
	let contents = fs::read_to_string(&output).unwrap();

	assert_eq!(contents.matches("pub use pallet_parachain_template;").count(), 1);
	assert_eq!(contents.matches("impl pallet_parachain_template::Config for Runtime {").count(), 1);
	assert_eq!(contents.matches("Template: pallet_parachain_template").count(), 1);
}
