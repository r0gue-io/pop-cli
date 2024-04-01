use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;
use toml_edit::DocumentMut;

#[test]
fn add_parachain_pallet_template() {
	let temp_dir = tempdir().unwrap();
	// Setup new parachain
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["new", "parachain", "testchain"])
		.assert()
		.success();
	// Git setup
	use duct::cmd;
	cmd!("git", "add", ".").dir(&temp_dir.path().join("testchain")).run().unwrap();
	cmd!("git", "commit", "--no-gpg-sign", "-m", "Initialized testchain")
		.dir(&temp_dir.path().join("testchain"))
		.stdout_null()
		.run()
		.unwrap();
	// Add pallet-parachain-template
	Command::cargo_bin("pop")
		.unwrap()
		.args(&["add", "pallet", "template", "-r", "testchain/runtime/src/lib.rs"])
		.current_dir(&temp_dir.path())
		.assert()
		.success();

	let runtime_contents =
		fs::read_to_string(&temp_dir.path().join("testchain/runtime/src/lib.rs")).unwrap();
	let runtime_manifest =
		fs::read_to_string(&temp_dir.path().join("testchain/runtime/Cargo.toml")).unwrap();
	// Check runtime entries
	assert_eq!(runtime_contents.matches("pub use pallet_parachain_template;").count(), 1);
	assert_eq!(
		runtime_contents
			.matches("impl pallet_parachain_template::Config for Runtime {")
			.count(),
		1
	);
	assert_eq!(runtime_contents.matches("Template: pallet_parachain_template").count(), 1);
	// Check runtime manifest entries
	let toml = runtime_manifest.parse::<DocumentMut>().unwrap();
	assert!(toml["dependencies"]["pallet-parachain-template"].is_value());
	let std = toml["features"]["std"].as_value().unwrap().as_array().unwrap();
	assert_eq!(
		std.iter()
			.filter(|val| val.as_str().unwrap() == "pallet-parachain-template/std")
			.count(),
		1
	);
	let rt_bench = toml["features"]["runtime-benchmarks"].as_value().unwrap().as_array().unwrap();
	assert_eq!(
		rt_bench
			.iter()
			.filter(|val| val.as_str().unwrap() == "pallet-parachain-template/runtime-benchmarks")
			.count(),
		1
	);
	let try_rt = toml["features"]["try-runtime"].as_value().unwrap().as_array().unwrap();
	assert_eq!(
		try_rt
			.iter()
			.filter(|val| val.as_str().unwrap() == "pallet-parachain-template/try-runtime")
			.count(),
		1
	);
}
