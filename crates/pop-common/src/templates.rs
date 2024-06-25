use crate::errors::Error;
use strum::{EnumMessage, EnumProperty, VariantArray};

pub trait Template:
	Clone + Default + EnumMessage + EnumProperty + Eq + PartialEq + VariantArray
{
	/// Get the template's name.
	fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the description of the template.
	fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the template's repository url.
	fn repository_url(&self) -> Result<&str, Error> {
		self.get_str("Repository").ok_or(Error::RepositoryMissing)
	}
	/// Get the list of supported templates.
	fn templates() -> &'static [Self] {
		Self::VARIANTS
	}
}

pub trait TemplateType<T: Template>:
	Clone + Default + EnumMessage + Eq + PartialEq + VariantArray
{
	const TYPE_ID: &'static str;

	/// Get the list of providers supported.
	fn types() -> &'static [Self] {
		Self::VARIANTS
	}

	/// Get provider's name.
	fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the default template of the provider.
	fn default_type(&self) -> T;

	/// Get the providers detailed description message.
	fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the list of templates of the provider.
	fn templates(&self) -> Vec<&T> {
		T::VARIANTS
			.iter()
			.filter(|t| t.get_str(Self::TYPE_ID) == Some(self.name()))
			.collect()
	}

	/// Check the template belongs to a template type.
	fn matches(&self, template: &T) -> bool {
		// Match explicitly on type name (message)
		template.get_str(Self::TYPE_ID) == Some(self.name())
	}
}
