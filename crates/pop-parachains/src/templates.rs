#[derive(Debug, Clone, PartialEq)]
pub struct Config {
	pub symbol: String,
	pub decimals: u8,
	pub initial_endowment: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Template {
	// Pop
	Base,
	// Parity
	ParityContracts,
	ParityFPT,
}
impl Template {
	pub fn is_provider_correct(&self, provider: &Provider) -> bool {
		match provider {
			Provider::Pop => self == &Template::Base,
			Provider::Parity => self == &Template::ParityContracts || self == &Template::ParityFPT,
		}
	}
	pub fn from(provider_name: &str) -> Self {
		match provider_name {
			"base" => Template::Base,
			"cpt" => Template::ParityContracts,
			"fpt" => Template::ParityFPT,
			_ => Template::Base,
		}
	}
	pub fn repository_url(&self) -> &str {
		match &self {
			Template::Base => "r0gue-io/base-parachain",
			Template::ParityContracts => "paritytech/substrate-contracts-node",
			Template::ParityFPT => "paritytech/frontier-parachain-template",
		}
	}
}

#[derive(Clone, Default, Debug, PartialEq)]
pub enum Provider {
	#[default]
	Pop,
	Parity,
}
impl Provider {
	pub fn default_template(&self) -> Template {
		match &self {
			Provider::Pop => Template::Base,
			Provider::Parity => Template::ParityContracts,
		}
	}
	pub fn from(provider_name: &str) -> Self {
		match provider_name {
			"Pop" => Provider::Pop,
			"Parity" => Provider::Parity,
			_ => Provider::Pop,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_is_template_correct() {
		let mut template = Template::Base;
		assert_eq!(template.is_provider_correct(&Provider::Pop), true);
		assert_eq!(template.is_provider_correct(&Provider::Parity), false);

		template = Template::ParityContracts;
		assert_eq!(template.is_provider_correct(&Provider::Pop), false);
		assert_eq!(template.is_provider_correct(&Provider::Parity), true);

		template = Template::ParityFPT;
		assert_eq!(template.is_provider_correct(&Provider::Pop), false);
		assert_eq!(template.is_provider_correct(&Provider::Parity), true);
	}

	#[test]
	fn test_convert_string_to_template() {
		assert_eq!(Template::from("base"), Template::Base);
		assert_eq!(Template::from(""), Template::Base);
		assert_eq!(Template::from("cpt"), Template::ParityContracts);
		assert_eq!(Template::from("fpt"), Template::ParityFPT);
	}

	#[test]
	fn test_repository_url() {
		let mut template = Template::Base;
		assert_eq!(template.repository_url(), "r0gue-io/base-parachain");
		template = Template::ParityContracts;
		assert_eq!(template.repository_url(), "paritytech/substrate-contracts-node");
		template = Template::ParityFPT;
		assert_eq!(template.repository_url(), "paritytech/frontier-parachain-template");
	}

	#[test]
	fn test_default_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.default_template(), Template::Base);
		provider = Provider::Parity;
		assert_eq!(provider.default_template(), Template::ParityContracts);
	}

	#[test]
	fn test_convert_string_to_provider() {
		assert_eq!(Provider::from("Pop"), Provider::Pop);
		assert_eq!(Provider::from(""), Provider::Pop);
		assert_eq!(Provider::from("Parity"), Provider::Parity);
	}
}
