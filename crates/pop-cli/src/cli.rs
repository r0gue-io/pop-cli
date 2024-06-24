// SPDX-License-Identifier: GPL-3.0

use std::{fmt::Display, io::Result};
#[cfg(test)]
pub(crate) use tests::MockCli;

pub(crate) mod traits {
	use std::{fmt::Display, io::Result};

	/// A command line interface.
	pub trait Cli {
		/// Constructs a new [`Confirm`] prompt.
		fn confirm(&mut self, prompt: impl Display) -> impl Confirm;
		/// Prints an info message.
		fn info(&mut self, text: impl Display) -> Result<()>;
		/// Prints a header of the prompt sequence.
		fn intro(&mut self, title: impl Display) -> Result<()>;
		/// Constructs a new [`MultiSelect`] prompt.
		fn multiselect<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl MultiSelect<T>;
		/// Prints a footer of the prompt sequence.
		fn outro(&mut self, message: impl Display) -> Result<()>;
		/// Prints a footer of the prompt sequence with a failure style.
		fn outro_cancel(&mut self, message: impl Display) -> Result<()>;
	}

	/// A confirmation prompt.
	pub trait Confirm {
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<bool>;
	}

	/// A multi-select prompt.
	pub trait MultiSelect<T> {
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<Vec<T>>;
		/// Adds an item to the list of options.
		fn item(self, value: T, label: impl Display, hint: impl Display) -> Self;
		/// Sets whether the input is required.
		fn required(self, required: bool) -> Self;
	}
}

/// A command line interface using cliclack.
pub(crate) struct Cli;
impl traits::Cli for Cli {
	/// Constructs a new [`Confirm`] prompt.
	fn confirm(&mut self, prompt: impl Display) -> impl traits::Confirm {
		Confirm(cliclack::confirm(prompt))
	}

	/// Prints an info message.
	fn info(&mut self, text: impl Display) -> Result<()> {
		cliclack::log::info(text)
	}

	/// Prints a header of the prompt sequence.
	fn intro(&mut self, title: impl Display) -> Result<()> {
		cliclack::clear_screen()?;
		cliclack::set_theme(crate::style::Theme);
		cliclack::intro(format!("{}: {title}", console::style(" Pop CLI ").black().on_magenta()))
	}

	/// Constructs a new [`MultiSelect`] prompt.
	fn multiselect<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl traits::MultiSelect<T> {
		MultiSelect::<T>(cliclack::multiselect(prompt))
	}

	/// Prints a footer of the prompt sequence.
	fn outro(&mut self, message: impl Display) -> Result<()> {
		cliclack::outro(message)
	}

	/// Prints a footer of the prompt sequence with a failure style.
	fn outro_cancel(&mut self, message: impl Display) -> Result<()> {
		cliclack::outro_cancel(message)
	}
}

/// A confirmation prompt using cliclack.
struct Confirm(cliclack::Confirm);
impl traits::Confirm for Confirm {
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<bool> {
		self.0.interact()
	}
}

/// A multi-select prompt using cliclack.
struct MultiSelect<T: Clone + Eq>(cliclack::MultiSelect<T>);

impl<T: Clone + Eq> traits::MultiSelect<T> for MultiSelect<T> {
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<Vec<T>> {
		self.0.interact()
	}

	/// Adds an item to the list of options.
	fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
		self.0 = self.0.item(value, label, hint);
		self
	}

	/// Sets whether the input is required.
	fn required(mut self, required: bool) -> Self {
		self.0 = self.0.required(required);
		self
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::traits::*;
	use std::{fmt::Display, io::Result};

	/// Mock Cli with optional expectations
	#[derive(Default)]
	pub(crate) struct MockCli {
		confirm_expectation: Option<(String, bool)>,
		info_expectations: Vec<String>,
		intro_expectation: Option<String>,
		outro_expectation: Option<String>,
		multiselect_expectation:
			Option<(String, Option<bool>, bool, Option<Vec<(String, String)>>)>,
		outro_cancel_expectation: Option<String>,
	}

	impl MockCli {
		pub(crate) fn new() -> Self {
			Self::default()
		}

		pub(crate) fn expect_confirm(mut self, prompt: impl Display, confirm: bool) -> Self {
			self.confirm_expectation = Some((prompt.to_string(), confirm));
			self
		}

		pub(crate) fn expect_info(mut self, text: impl Display) -> Self {
			self.info_expectations.push(text.to_string());
			self
		}

		pub(crate) fn expect_intro(mut self, title: impl Display) -> Self {
			self.intro_expectation = Some(title.to_string());
			self
		}

		pub(crate) fn expect_multiselect<T>(
			mut self,
			prompt: impl Display,
			required: Option<bool>,
			collect: bool,
			items: Option<Vec<(String, String)>>,
		) -> Self {
			self.multiselect_expectation = Some((prompt.to_string(), required, collect, items));
			self
		}

		pub(crate) fn expect_outro(mut self, message: impl Display) -> Self {
			self.outro_expectation = Some(message.to_string());
			self
		}

		pub(crate) fn expect_outro_cancel(mut self, message: impl Display) -> Self {
			self.outro_cancel_expectation = Some(message.to_string());
			self
		}

		pub(crate) fn verify(self) -> anyhow::Result<()> {
			if let Some((expectation, _)) = self.confirm_expectation {
				panic!("`{expectation}` confirm expectation not satisfied")
			}
			if !self.info_expectations.is_empty() {
				panic!("`{}` info log expectations not satisfied", self.info_expectations.join(","))
			}
			if let Some(expectation) = self.intro_expectation {
				panic!("`{expectation}` intro expectation not satisfied")
			}
			if let Some((prompt, _, _, _)) = self.multiselect_expectation {
				panic!("`{prompt}` multiselect prompt expectation not satisfied")
			}
			if let Some(expectation) = self.outro_expectation {
				panic!("`{expectation}` outro expectation not satisfied")
			}
			if let Some(expectation) = self.outro_cancel_expectation {
				panic!("`{expectation}` outro cancel expectation not satisfied")
			}
			Ok(())
		}
	}

	impl Cli for MockCli {
		fn confirm(&mut self, prompt: impl Display) -> impl Confirm {
			let prompt = prompt.to_string();
			if let Some((expectation, confirm)) = self.confirm_expectation.take() {
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockConfirm { confirm };
			}
			MockConfirm::default()
		}

		fn info(&mut self, text: impl Display) -> Result<()> {
			let text = text.to_string();
			self.info_expectations.retain(|x| *x != text);
			Ok(())
		}

		fn intro(&mut self, title: impl Display) -> Result<()> {
			if let Some(expectation) = self.intro_expectation.take() {
				assert_eq!(expectation, title.to_string(), "intro does not satisfy expectation");
			}
			Ok(())
		}

		fn multiselect<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl MultiSelect<T> {
			let prompt = prompt.to_string();
			if let Some((expectation, required_expectation, collect, items_expectation)) =
				self.multiselect_expectation.take()
			{
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockMultiSelect {
					required_expectation,
					items_expectation,
					collect,
					items: vec![],
				};
			}

			MockMultiSelect::default()
		}

		fn outro(&mut self, message: impl Display) -> Result<()> {
			if let Some(expectation) = self.outro_expectation.take() {
				assert_eq!(
					expectation,
					message.to_string(),
					"outro message does not satisfy expectation"
				);
			}
			Ok(())
		}

		fn outro_cancel(&mut self, message: impl Display) -> Result<()> {
			if let Some(expectation) = self.outro_cancel_expectation.take() {
				assert_eq!(
					expectation,
					message.to_string(),
					"outro message does not satisfy expectation"
				);
			}
			Ok(())
		}
	}

	/// Mock confirm prompt
	#[derive(Default)]
	struct MockConfirm {
		confirm: bool,
	}

	impl Confirm for MockConfirm {
		fn interact(&mut self) -> Result<bool> {
			Ok(self.confirm)
		}
	}

	/// Mock multi-select prompt
	pub(crate) struct MockMultiSelect<T> {
		required_expectation: Option<bool>,
		items_expectation: Option<Vec<(String, String)>>,
		collect: bool,
		items: Vec<T>,
	}

	impl<T> MockMultiSelect<T> {
		pub(crate) fn default() -> Self {
			Self {
				required_expectation: None,
				items_expectation: None,
				collect: false,
				items: vec![],
			}
		}
	}

	impl<T: Clone + Eq> MultiSelect<T> for MockMultiSelect<T> {
		fn interact(&mut self) -> Result<Vec<T>> {
			// Pass any collected items
			Ok(self.items.clone())
		}

		fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
			// Check expectations
			if let Some(items) = self.items_expectation.as_mut() {
				let item = (label.to_string(), hint.to_string());
				assert!(items.contains(&item), "`{item:?}` item does not satisfy any expectations");
				items.retain(|x| *x != item);
			}
			// Collect if specified
			if self.collect {
				self.items.push(value);
			}
			self
		}

		fn required(mut self, required: bool) -> Self {
			if let Some(expectation) = self.required_expectation.as_ref() {
				assert_eq!(*expectation, required, "required does not satisfy expectation");
				self.required_expectation = None;
			}
			self
		}
	}
}
