use duct::cmd;
use std::path::PathBuf;

const BIN_NAME: &str = "substrate-contracts-node";

pub fn run_contracts_node(cache: PathBuf) -> anyhow::Result<()> {
	let cached_file = cache.join("bin").join(BIN_NAME);
	if !cached_file.exists() {
		cmd(
			"cargo",
			vec!["install", "--root", cache.display().to_string().as_str(), "contracts-node"],
		)
		.run()?;
	}
	cmd(cached_file.display().to_string().as_str(), Vec::<&str>::new()).run()?;
	Ok(())
}
