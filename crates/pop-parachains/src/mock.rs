// Mock the command that build the parachain
pub fn cmd(_program: &str, _args: Vec<&str>) -> duct::Expression {
	duct::cmd!("echo")
}
