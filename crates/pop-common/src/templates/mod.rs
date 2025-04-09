// SPDX-License-Identifier: GPL-3.0

use strum::{EnumMessage, EnumProperty, VariantArray};
pub use thiserror::Error;

/// Functions for extracting a template's files.
pub mod extractor;

/// An error relating to templates or template variants.
#[derive(Error, Debug)]
pub enum Error {
	/// The `Repository` property is missing from the template variant.
	#[error("The `Repository` property is missing from the template variant")]
	RepositoryMissing,
	/// The `TypeMissing` property is missing from the template variant.
	#[error("The `TypeMissing` property is missing from the template variant")]
	TypeMissing,
}

/// A trait for templates. A template is a variant of a template type.
pub trait Template:
	Clone + Default + EnumMessage + EnumProperty + Eq + PartialEq + VariantArray
{
	/// The template's type property identifier.
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
		self.get_str(Self::PROPERTY).ok_or(Error::TypeMissing)
	}

	/// Get whether the template is deprecated.
	fn is_deprecated(&self) -> bool {
		self.get_str("IsDeprecated") == Some("true")
	}

	/// Get the deprecation message for the template
	fn deprecated_message(&self) -> &str {
		self.get_str("DeprecatedMessage").unwrap_or_default()
	}
}

/// A trait for defining overarching types of specific template variants.
/// A Type has many Template variants.
/// The method `default_template` should be implemented unless
/// no default templates are desired.
pub trait Type<T: Template>: Clone + Default + EnumMessage + Eq + PartialEq + VariantArray {
	/// Get the list of types supported.
	fn types() -> &'static [Self] {
		Self::VARIANTS
	}

	/// Get types's name.
	fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the default template of the type.
	fn default_template(&self) -> Option<T> {
		None
	}

	/// Get the type's description.
	fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the list of templates of the type.
	fn templates(&self) -> Vec<&T> {
		T::VARIANTS
			.iter()
			.filter(|t| t.get_str(T::PROPERTY) == Some(self.name()) && !t.is_deprecated())
			.collect()
	}

	/// Check the type provides the template.
	fn provides(&self, template: &T) -> bool {
		// Match explicitly on type name (message)
		template.get_str(T::PROPERTY) == Some(self.name())
	}
}

/// The possible values from the variants of an enum.
#[macro_export]
macro_rules! enum_variants {
	($e: ty) => {{
		PossibleValuesParser::new(
			<$e>::VARIANTS
				.iter()
				.map(|p| PossibleValue::new(p.as_ref()))
				.collect::<Vec<_>>(),
		)
		.try_map(|s| <$e>::from_str(&s).map_err(|e| format!("could not convert from {s} to type")))
	}};
}

/// The possible values from the variants of an enum which are not deprecated.
#[macro_export]
macro_rules! enum_variants_without_deprecated {
	($e:ty) => {{
		<$e>::VARIANTS
			.iter()
			.filter(|variant| !variant.is_deprecated()) // Exclude deprecated variants for --help
			.map(|v| v.as_ref())
			.collect::<Vec<_>>()
			.join(", ")
	}};
}
