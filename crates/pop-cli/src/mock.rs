use std::path::PathBuf;

// Mock the build function
#[cfg(test)]
pub fn build_parachain(_path: &Option<PathBuf>) -> anyhow::Result<()> {
	Ok(())
}
