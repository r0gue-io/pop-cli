use assert_cmd::Command;
use std::fs;
use tempdir::TempDir;

#[ignore = "test fails to run"]
#[test]
fn add_parachain_pallet_template() {
	let temp_dir = TempDir::new("add-pallet-test").unwrap();
	// Setup new parachain
	Command::cargo_bin("pop")
		.unwrap()
		.args(&["new", "parachain", "testchain"])
		.assert()
		.success();

	println!("{:?}", temp_dir.path().display());
	// Add pallet-parachain-template
	Command::cargo_bin("pop")
		.unwrap()
		.args(&["add", "pallet", "template", "-r", "runtime/src/lib.rs"])
		.current_dir(&temp_dir.path().join("testchain"))
		.assert()
		.success();

	let runtime_contents =
		fs::read_to_string(&temp_dir.path().join("testchain/runtime/src/lib.rs")).unwrap();
	let runtime_manifest =
		fs::read_to_string(&temp_dir.path().join("testchain/runtime/Cargo.toml")).unwrap();

	assert_eq!(runtime_contents.matches("pub use pallet_parachain_template;").count(), 1);
	assert_eq!(
		runtime_contents
			.matches("impl pallet_parachain_template::Config for Runtime {")
			.count(),
		1
	);
	assert_eq!(runtime_contents.matches("Template: pallet_parachain_template").count(), 1);

	assert_eq!(runtime_manifest.matches("pallet-parachain-template").count(), 3);
}
