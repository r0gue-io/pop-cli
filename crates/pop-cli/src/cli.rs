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
		/// Constructs a new [`Input`] prompt.
		fn input(&mut self, prompt: impl Display) -> impl Input;
		/// Prints a header of the prompt sequence.
		fn intro(&mut self, title: impl Display) -> Result<()>;
		/// Constructs a new [`MultiSelect`] prompt.
		fn multiselect<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl MultiSelect<T>;
		/// Prints a footer of the prompt sequence.
		fn outro(&mut self, message: impl Display) -> Result<()>;
		/// Prints a footer of the prompt sequence with a failure style.
		fn outro_cancel(&mut self, message: impl Display) -> Result<()>;
		/// Constructs a new [`Select`] prompt.
		fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl Select<T>;
		/// Prints a success message.
		fn success(&mut self, message: impl Display) -> Result<()>;
		/// Prints a warning message.
		fn warning(&mut self, message: impl Display) -> Result<()>;
	}

	/// A confirmation prompt.
	pub trait Confirm {
		/// Sets the initially selected value.
		fn initial_value(self, initial_value: bool) -> Self;
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<bool>;
	}

	/// A text input prompt.
	pub trait Input {
		/// Sets the default value for the input.
		fn default_input(self, value: &str) -> Self;
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<String>;
		/// Sets the placeholder (hint) text for the input.
		fn placeholder(self, value: &str) -> Self;
		/// Sets whether the input is required.
		fn required(self, required: bool) -> Self;
		/// Sets a validation callback for the input that is called when the user submits.
		fn validate(
			self,
			validator: impl Fn(&String) -> std::result::Result<(), &'static str> + 'static,
		) -> Self;
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

	/// A select prompt.
	pub trait Select<T> {
		/// Sets the initially selected value.
		fn initial_value(self, initial_value: T) -> Self;
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<T>;
		/// Adds an item to the selection prompt.
		fn item(self, value: T, label: impl Display, hint: impl Display) -> Self;
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

	/// Constructs a new [`Input`] prompt.
	fn input(&mut self, prompt: impl Display) -> impl traits::Input {
		Input(cliclack::input(prompt))
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

	/// Constructs a new [`Select`] prompt.
	fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl traits::Select<T> {
		Select::<T>(cliclack::select(prompt))
	}

	/// Prints a success message.
	fn success(&mut self, message: impl Display) -> Result<()> {
		cliclack::log::success(message)
	}

	/// Prints a warning message.
	fn warning(&mut self, message: impl Display) -> Result<()> {
		cliclack::log::warning(message)
	}
}

/// A confirmation prompt using cliclack.
struct Confirm(cliclack::Confirm);

impl traits::Confirm for Confirm {
	/// Sets the initially selected value.
	fn initial_value(mut self, initial_value: bool) -> Self {
		self.0 = self.0.initial_value(initial_value);
		self
	}
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<bool> {
		self.0.interact()
	}
}

/// A input prompt using cliclack.
struct Input(cliclack::Input);
impl traits::Input for Input {
	/// Sets the default value for the input.
	fn default_input(mut self, value: &str) -> Self {
		self.0 = self.0.default_input(value);
		self
	}
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<String> {
		self.0.interact()
	}
	/// Sets the placeholder (hint) text for the input.
	fn placeholder(mut self, placeholder: &str) -> Self {
		self.0 = self.0.placeholder(placeholder);
		self
	}
	/// Sets whether the input is required.
	fn required(mut self, required: bool) -> Self {
		self.0 = self.0.required(required);
		self
	}
	/// Sets a validation callback for the input that is called when the user submits.
	fn validate(
		mut self,
		validator: impl Fn(&String) -> std::result::Result<(), &'static str> + 'static,
	) -> Self {
		self.0 = self.0.validate(validator);
		self
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

/// A select prompt using cliclack.
struct Select<T: Clone + Eq>(cliclack::Select<T>);

impl<T: Clone + Eq> traits::Select<T> for Select<T> {
	/// Sets the initially selected value.
	fn initial_value(mut self, initial_value: T) -> Self {
		self.0 = self.0.initial_value(initial_value);
		self
	}

	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<T> {
		self.0.interact()
	}

	/// Adds an item to the selection prompt.
	fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
		self.0 = self.0.item(value, label, hint);
		self
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::traits::*;
	use std::{fmt::Display, io::Result, usize};

	/// Mock Cli with optional expectations
	#[derive(Default)]
	pub(crate) struct MockCli {
		confirm_expectation: Vec<(String, bool)>,
		info_expectations: Vec<String>,
		input_expectations: Vec<(String, String)>,
		intro_expectation: Option<String>,
		outro_expectation: Option<String>,
		multiselect_expectation:
			Option<(String, Option<bool>, bool, Option<Vec<(String, String)>>)>,
		outro_cancel_expectation: Option<String>,
		select_expectation: Vec<(String, Option<bool>, bool, Option<Vec<(String, String)>>, usize)>,
		success_expectations: Vec<String>,
		warning_expectations: Vec<String>,
	}

	impl MockCli {
		pub(crate) fn new() -> Self {
			Self::default()
		}

		pub(crate) fn expect_confirm(mut self, prompt: impl Display, confirm: bool) -> Self {
			self.confirm_expectation.insert(0, (prompt.to_string(), confirm));
			self
		}

		pub(crate) fn expect_input(mut self, prompt: impl Display, input: String) -> Self {
			self.input_expectations.insert(0, (prompt.to_string(), input));
			self
		}

		pub(crate) fn expect_info(mut self, message: impl Display) -> Self {
			self.info_expectations.insert(0, message.to_string());
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

		pub(crate) fn expect_select(
			mut self,
			prompt: impl Display,
			required: Option<bool>,
			collect: bool,
			items: Option<Vec<(String, String)>>,
			item: usize,
		) -> Self {
			self.select_expectation
				.insert(0, (prompt.to_string(), required, collect, items, item));
			self
		}

		#[allow(dead_code)]
		pub(crate) fn expect_success(mut self, message: impl Display) -> Self {
			self.success_expectations.push(message.to_string());
			self
		}

		pub(crate) fn expect_warning(mut self, message: impl Display) -> Self {
			self.warning_expectations.push(message.to_string());
			self
		}

		pub(crate) fn verify(self) -> anyhow::Result<()> {
			if !self.confirm_expectation.is_empty() {
				panic!("`{:?}` confirm expectations not satisfied", self.confirm_expectation)
			}
			if !self.info_expectations.is_empty() {
				panic!("`{}` info log expectations not satisfied", self.info_expectations.join(","))
			}
			if !self.input_expectations.is_empty() {
				panic!("`{:?}` input expectation not satisfied", self.input_expectations)
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
			if !self.select_expectation.is_empty() {
				panic!(
					"`{}` select prompt expectation not satisfied",
					self.select_expectation
						.iter()
						.map(|(s, _, _, _, _)| s.clone()) // Extract the `String` part
						.collect::<Vec<_>>()
						.join(", ")
				);
			}
			if !self.success_expectations.is_empty() {
				panic!(
					"`{}` success log expectations not satisfied",
					self.success_expectations.join(",")
				)
			}
			if !self.warning_expectations.is_empty() {
				panic!(
					"`{}` warning log expectations not satisfied",
					self.warning_expectations.join(",")
				)
			}
			Ok(())
		}
	}

	impl Cli for MockCli {
		fn confirm(&mut self, prompt: impl Display) -> impl Confirm {
			let prompt = prompt.to_string();
			if let Some((expectation, confirm)) = self.confirm_expectation.pop() {
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockConfirm { confirm };
			}
			MockConfirm::default()
		}

		fn info(&mut self, message: impl Display) -> Result<()> {
			let message = message.to_string();
			self.info_expectations.retain(|x| *x != message);
			Ok(())
		}

		fn input(&mut self, prompt: impl Display) -> impl Input {
			let prompt = prompt.to_string();
			if let Some((expectation, input)) = self.input_expectations.pop() {
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockInput {
					prompt: input.clone(),
					input,
					placeholder: "".to_string(),
					required: false,
				};
			}
			MockInput::default()
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

		fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl Select<T> {
			let prompt = prompt.to_string();
			if let Some((expectation, _, collect, items_expectation, item)) =
				self.select_expectation.pop()
			{
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockSelect {
					items_expectation,
					collect,
					items: vec![],
					item,
					initial_value: None,
				};
			}

			MockSelect::default()
		}

		fn success(&mut self, message: impl Display) -> Result<()> {
			let message = message.to_string();
			self.success_expectations.retain(|x| *x != message);
			Ok(())
		}

		fn warning(&mut self, message: impl Display) -> Result<()> {
			let message = message.to_string();
			self.warning_expectations.retain(|x| *x != message);
			Ok(())
		}
	}

	/// Mock confirm prompt
	#[derive(Default)]
	struct MockConfirm {
		confirm: bool,
	}

	impl Confirm for MockConfirm {
		fn initial_value(mut self, _initial_value: bool) -> Self {
			self.confirm = self.confirm; // Ignore initial value and always return mock value
			self
		}
		fn interact(&mut self) -> Result<bool> {
			Ok(self.confirm)
		}
	}

	/// Mock input prompt
	#[derive(Default)]
	struct MockInput {
		prompt: String,
		input: String,
		placeholder: String,
		required: bool,
	}

	impl Input for MockInput {
		fn interact(&mut self) -> Result<String> {
			Ok(self.prompt.clone())
		}
		fn default_input(mut self, value: &str) -> Self {
			self.input = value.to_string();
			self
		}

		fn placeholder(mut self, value: &str) -> Self {
			self.placeholder = value.to_string();
			self
		}

		fn required(mut self, value: bool) -> Self {
			self.required = value;
			self
		}

		fn validate(
			self,
			_validator: impl Fn(&String) -> std::result::Result<(), &'static str> + 'static,
		) -> Self {
			self
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

	/// Mock select prompt
	pub(crate) struct MockSelect<T> {
		items_expectation: Option<Vec<(String, String)>>,
		collect: bool,
		items: Vec<T>,
		item: usize,
		initial_value: Option<T>,
	}

	impl<T> MockSelect<T> {
		pub(crate) fn default() -> Self {
			Self {
				items_expectation: None,
				collect: false,
				items: vec![],
				item: 0,
				initial_value: None,
			}
		}
	}

	impl<T: Clone + Eq> Select<T> for MockSelect<T> {
		fn initial_value(mut self, initial_value: T) -> Self {
			self.initial_value = Some(initial_value);
			self
		}

		fn interact(&mut self) -> Result<T> {
			Ok(self.items[self.item].clone())
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
	}
}
