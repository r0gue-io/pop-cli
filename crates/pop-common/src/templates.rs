use crate::errors::Error;
use strum::{EnumMessage, EnumProperty, VariantArray};

/// A trait for templates. A template is a variant of a template type.
pub trait Template:
	Clone + Default + EnumMessage + EnumProperty + Eq + PartialEq + VariantArray
{
	// What is the template's type strum property identifier.
	const PROPERTY: &'static str = "Type";

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

	/// Get the type of the template.
	fn template_type(&self) -> Result<&str, Error> {
		self.get_str(Self::PROPERTY).ok_or(Error::TemplateTypeMissing)
	}
}

/// A trait for defining overarching types of specific template variants.
/// A TemplateType has many Template variants.
/// The method `default_template` must be implemented.
pub trait TemplateType<T: Template>:
	Clone + Default + EnumMessage + Eq + PartialEq + VariantArray
{
	/// Get the list of types supported.
	fn types() -> &'static [Self] {
		Self::VARIANTS
	}

	/// Get types's name.
	fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the default template of the type.
	fn default_template(&self) -> T;

	/// Get the type's description.
	fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the list of templates of the type.
	fn templates(&self) -> Vec<&T> {
		T::VARIANTS
			.iter()
			.filter(|t| t.get_str(T::PROPERTY) == Some(self.name()))
			.collect()
	}

	/// Check the template belongs to a type.
	fn matches(&self, template: &T) -> bool {
		// Match explicitly on type name (message)
		template.get_str(T::PROPERTY) == Some(self.name())
	}
}
