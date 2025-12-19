// SPDX-License-Identifier: GPL-3.0

use std::{fmt::Display, io::Result};
#[cfg(not(test))]
use std::{thread::sleep, time::Duration};
#[cfg(test)]
pub(crate) use tests::MockCli;

pub(crate) mod traits {
	use std::{fmt::Display, io::Result};

	/// A command line interface.
	#[allow(dead_code)]
	pub trait Cli {
		/// Returns whether the output should be in JSON format.
		fn is_json(&self) -> bool;
		/// Constructs a new [`Confirm`] prompt.
		fn confirm(&mut self, prompt: impl Display) -> impl Confirm;
		/// Prints an error message.
		fn error(&mut self, text: impl Display) -> Result<()>;
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
		/// Constructs a new [`Password`] prompt.
		#[allow(dead_code)]
		fn password(&mut self, prompt: impl Display) -> impl Password;
		/// Constructs a new [`Select`] prompt.
		fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl Select<T>;
		/// Prints a success message.
		fn success(&mut self, message: impl Display) -> Result<()>;
		/// Prints a warning message.
		fn warning(&mut self, message: impl Display) -> Result<()>;
		/// Prints a plain message.
		fn plain(&mut self, message: impl Display) -> Result<()>;
		/// Constructs a new [`Spinner`].
		fn spinner(&mut self) -> Box<dyn Spinner + Send>;
		/// Constructs a new [`MultiProgress`].
		fn multi_progress(&mut self, title: impl Display) -> Box<dyn MultiProgress + Send>;
	}

	/// A spinner.
	pub trait Spinner: Send {
		/// Starts the spinner.
		fn start(&self, message: &str);
		/// Sets the message of the spinner.
		fn set_message(&self, message: &str);
		/// Stops the spinner.
		fn stop(&self, message: &str);
		/// Stops the spinner with an error message.
		fn error(&self, message: &str);
		/// Stops the spinner with a cancel message.
		#[allow(dead_code)]
		fn cancel(&self, message: &str);
		/// Clears the spinner.
		fn clear(&self);
	}

	/// A multi-progress bar.
	pub trait MultiProgress: Send {
		/// Adds a spinner to the multi-progress bar.
		fn add(&mut self, message: &str) -> Box<dyn Spinner + Send>;
		/// Stops the multi-progress bar.
		fn stop(&mut self);
	}

	/// A confirmation prompt.
	#[allow(dead_code)]
	pub trait Confirm {
		/// Sets the initially selected value.
		fn initial_value(self, initial_value: bool) -> Self;
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<bool>;
	}

	/// A text input prompt.
	#[allow(dead_code)]
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
	#[allow(dead_code)]
	pub trait MultiSelect<T> {
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<Vec<T>>;
		/// Adds an item to the list of options.
		fn item(self, value: T, label: impl Display, hint: impl Display) -> Self;
		/// Sets whether the input is required.
		fn required(self, required: bool) -> Self;
		/// The filter mode allows to filter the items by typing.
		fn filter_mode(self) -> Self;
	}

	/// A prompt that masks the input.
	#[allow(dead_code)]
	pub trait Password {
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<String>;
	}

	/// A select prompt.
	#[allow(dead_code)]
	pub trait Select<T> {
		/// Sets the initially selected value.
		fn initial_value(self, initial_value: T) -> Self;
		/// Starts the prompt interaction.
		fn interact(&mut self) -> Result<T>;
		/// Adds an item to the selection prompt.
		fn item(self, value: T, label: impl Display, hint: impl Display) -> Self;
		/// The filter mode allows to filter the items by typing.
		fn filter_mode(self) -> Self;
	}
}

/// A command line interface using cliclack.
pub(crate) struct Cli {
	pub(crate) json: bool,
}

impl traits::Cli for Cli {
	fn is_json(&self) -> bool {
		self.json
	}

	/// Constructs a new [`Confirm`] prompt.
	fn confirm(&mut self, prompt: impl Display) -> impl traits::Confirm {
		Confirm { inner: cliclack::confirm(prompt), json: self.json }
	}

	/// Prints an error message.
	fn error(&mut self, text: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", text);
			Ok(())
		} else {
			cliclack::log::error(text)
		}
	}

	/// Prints an info message.
	fn info(&mut self, text: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", text);
			Ok(())
		} else {
			cliclack::log::info(text)
		}
	}

	/// Constructs a new [`Input`] prompt.
	fn input(&mut self, prompt: impl Display) -> impl traits::Input {
		Input { inner: cliclack::input(prompt), json: self.json }
	}

	/// Prints a header of the prompt sequence.
	fn intro(&mut self, title: impl Display) -> Result<()> {
		if self.json {
			return Ok(());
		}
		cliclack::clear_screen()?;
		cliclack::set_theme(crate::style::Theme);
		cliclack::intro(format!("{}: {title}", console::style(" Pop CLI ").black().on_magenta()))
	}

	/// Constructs a new [`MultiSelect`] prompt.
	fn multiselect<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl traits::MultiSelect<T> {
		MultiSelect::<T> { inner: cliclack::multiselect(prompt), json: self.json }
	}

	/// Prints a footer of the prompt sequence.
	fn outro(&mut self, message: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", message);
			Ok(())
		} else {
			cliclack::outro(message)
		}
	}

	/// Prints a footer of the prompt sequence with a failure style.
	fn outro_cancel(&mut self, message: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", message);
			Ok(())
		} else {
			cliclack::outro_cancel(message)
		}
	}

	/// Constructs a new [`Password`] prompt.
	fn password(&mut self, prompt: impl Display) -> impl traits::Password {
		Password { inner: cliclack::password(prompt), json: self.json }
	}

	/// Constructs a new [`Select`] prompt.
	fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl traits::Select<T> {
		Select::<T> { inner: cliclack::select(prompt), json: self.json }
	}

	/// Prints a success message.
	fn success(&mut self, message: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", message);
			Ok(())
		} else {
			cliclack::log::success(message)
		}
	}

	/// Prints a warning message.
	fn warning(&mut self, message: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", message);
			Ok(())
		} else {
			cliclack::log::warning(message)?;
			#[cfg(not(test))]
			sleep(Duration::from_secs(1));
			Ok(())
		}
	}

	fn plain(&mut self, message: impl Display) -> Result<()> {
		if self.json {
			eprintln!("{}", message);
		} else {
			println!("{message}");
		}
		Ok(())
	}

	fn spinner(&mut self) -> Box<dyn traits::Spinner + Send> {
		Box::new(Spinner {
			inner: std::sync::Arc::new(std::sync::Mutex::new(None)),
			json: self.json,
		})
	}

	fn multi_progress(&mut self, title: impl Display) -> Box<dyn traits::MultiProgress + Send> {
		if !self.json {
			cliclack::intro(title).ok();
		} else {
			eprintln!("{}", title);
		}
		Box::new(MultiProgress {
			inner: std::sync::Arc::new(std::sync::Mutex::new(if self.json {
				None
			} else {
				Some(cliclack::multi_progress(""))
			})),
			json: self.json,
		})
	}
}

/// Constructs a new [`Spinner`].
pub fn spinner() -> impl traits::Spinner {
	Spinner { inner: std::sync::Arc::new(std::sync::Mutex::new(None)), json: pop_common::is_json() }
}

/// A spinner using cliclack.
#[derive(Clone)]
struct Spinner {
	inner: std::sync::Arc<std::sync::Mutex<Option<cliclack::ProgressBar>>>,
	json: bool,
}

impl traits::Spinner for Spinner {
	fn start(&self, message: &str) {
		if !self.json {
			let s = cliclack::spinner();
			s.start(message);
			if let Ok(mut inner) = self.inner.lock() {
				*inner = Some(s);
			}
		} else {
			eprintln!("{}", message);
		}
	}

	fn set_message(&self, message: &str) {
		if let Ok(mut inner) = self.inner.lock() {
			if let Some(ref mut s) = *inner {
				s.set_message(message);
			} else if self.json {
				eprintln!("{}", message);
			}
		}
	}

	fn stop(&self, message: &str) {
		if let Ok(mut inner) = self.inner.lock() {
			if let Some(s) = inner.take() {
				s.stop(message);
			} else if self.json {
				eprintln!("{}", message);
			}
		}
	}

	fn cancel(&self, message: &str) {
		if let Ok(mut inner) = self.inner.lock() {
			if let Some(s) = inner.take() {
				s.cancel(message);
			} else if self.json {
				eprintln!("{}", message);
			}
		}
	}

	fn error(&self, message: &str) {
		if let Ok(mut inner) = self.inner.lock() {
			if let Some(s) = inner.take() {
				s.error(message);
			} else if self.json {
				eprintln!("{}", message);
			}
		}
	}

	fn clear(&self) {
		if let Ok(mut inner) = self.inner.lock() &&
			let Some(s) = inner.take()
		{
			s.clear();
		}
	}
}

/// A multi-progress bar using cliclack.
struct MultiProgress {
	inner: std::sync::Arc<std::sync::Mutex<Option<cliclack::MultiProgress>>>,
	json: bool,
}

impl traits::MultiProgress for MultiProgress {
	fn add(&mut self, message: &str) -> Box<dyn traits::Spinner + Send> {
		if let Ok(mut inner_multi) = self.inner.lock() {
			if let Some(ref mut m) = *inner_multi {
				let s = cliclack::spinner();
				s.start(message);
				let s = m.add(s);
				Box::new(Spinner {
					inner: std::sync::Arc::new(std::sync::Mutex::new(Some(s))),
					json: self.json,
				})
			} else {
				eprintln!("{}", message);
				Box::new(Spinner {
					inner: std::sync::Arc::new(std::sync::Mutex::new(None)),
					json: self.json,
				})
			}
		} else {
			Box::new(Spinner {
				inner: std::sync::Arc::new(std::sync::Mutex::new(None)),
				json: self.json,
			})
		}
	}

	fn stop(&mut self) {
		if let Ok(mut inner) = self.inner.lock() &&
			let Some(m) = inner.take()
		{
			m.stop();
		}
	}
}

/// A confirmation prompt using cliclack.
struct Confirm {
	inner: cliclack::Confirm,
	json: bool,
}

impl traits::Confirm for Confirm {
	/// Sets the initially selected value.
	fn initial_value(mut self, initial_value: bool) -> Self {
		self.inner = self.inner.initial_value(initial_value);
		self
	}

	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<bool> {
		if self.json {
			return Err(std::io::Error::other("Prompt required"));
		}
		self.inner.interact()
	}
}

/// A input prompt using cliclack.
#[allow(dead_code)]
struct Input {
	inner: cliclack::Input,
	json: bool,
}

impl traits::Input for Input {
	/// Sets the default value for the input.
	fn default_input(mut self, value: &str) -> Self {
		self.inner = self.inner.default_input(value);
		self
	}

	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<String> {
		if self.json {
			return Err(std::io::Error::other("Prompt required"));
		}
		self.inner.interact()
	}

	/// Sets the placeholder (hint) text for the input.
	fn placeholder(mut self, placeholder: &str) -> Self {
		self.inner = self.inner.placeholder(placeholder);
		self
	}

	/// Sets whether the input is required.
	fn required(mut self, required: bool) -> Self {
		self.inner = self.inner.required(required);
		self
	}

	/// Sets a validation callback for the input that is called when the user submits.
	fn validate(
		mut self,
		validator: impl Fn(&String) -> std::result::Result<(), &'static str> + 'static,
	) -> Self {
		self.inner = self.inner.validate(validator);
		self
	}
}

/// A multi-select prompt using cliclack.
struct MultiSelect<T: Clone + Eq> {
	inner: cliclack::MultiSelect<T>,
	json: bool,
}

impl<T: Clone + Eq> traits::MultiSelect<T> for MultiSelect<T> {
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<Vec<T>> {
		if self.json {
			return Err(std::io::Error::other("Prompt required"));
		}
		self.inner.interact()
	}

	/// Adds an item to the list of options.
	fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
		self.inner = self.inner.item(value, label, hint);
		self
	}

	/// Sets whether the input is required.
	fn required(mut self, required: bool) -> Self {
		self.inner = self.inner.required(required);
		self
	}

	/// The filter mode allows to filter the items by typing.
	fn filter_mode(mut self) -> Self {
		self.inner = self.inner.filter_mode();
		self
	}
}

/// A password prompt using cliclack.
#[allow(dead_code)]
struct Password {
	inner: cliclack::Password,
	json: bool,
}

impl traits::Password for Password {
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<String> {
		if self.json {
			return Err(std::io::Error::other("Prompt required"));
		}
		self.inner.interact()
	}
}

/// A select prompt using cliclack.
#[allow(dead_code)]
struct Select<T: Clone + Eq> {
	inner: cliclack::Select<T>,
	json: bool,
}

impl<T: Clone + Eq> traits::Select<T> for Select<T> {
	/// Sets the initially selected value.
	fn initial_value(mut self, initial_value: T) -> Self {
		self.inner = self.inner.initial_value(initial_value);
		self
	}

	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<T> {
		if self.json {
			return Err(std::io::Error::other("Prompt required"));
		}
		self.inner.interact()
	}

	/// Adds an item to the selection prompt.
	fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
		self.inner = self.inner.item(value, label, hint);
		self
	}

	/// The filter mode allows to filter the items by typing.
	fn filter_mode(mut self) -> Self {
		self.inner = self.inner.filter_mode();
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
		pub(crate) json: bool,
		confirm_expectation: Vec<(String, bool)>,
		error_expectations: Vec<String>,
		info_expectations: Vec<String>,
		input_expectations: Vec<(String, String)>,
		intro_expectation: Option<String>,
		outro_expectation: Option<String>,
		multiselect_expectation:
			Option<(String, Option<bool>, bool, Option<Vec<(String, String)>>, Option<bool>)>,
		outro_cancel_expectation: Option<String>,
		password_expectations: Vec<(String, String)>,
		select_expectation:
			Vec<(String, Option<bool>, bool, Option<Vec<(String, String)>>, usize, Option<bool>)>,
		success_expectations: Vec<String>,
		warning_expectations: Vec<String>,
		plain_expectations: Vec<String>,
	}

	#[allow(dead_code)]
	impl MockCli {
		pub(crate) fn new() -> Self {
			Self::default()
		}

		pub(crate) fn expect_confirm(mut self, prompt: impl Display, confirm: bool) -> Self {
			self.confirm_expectation.insert(0, (prompt.to_string(), confirm));
			self
		}

		pub(crate) fn expect_error(mut self, message: impl Display) -> Self {
			self.error_expectations.insert(0, message.to_string());
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

		pub(crate) fn expect_multiselect(
			mut self,
			prompt: impl Display,
			required: Option<bool>,
			collect: bool,
			items: Option<Vec<(String, String)>>,
			filter_mode: Option<bool>,
		) -> Self {
			self.multiselect_expectation =
				Some((prompt.to_string(), required, collect, items, filter_mode));
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

		pub(crate) fn expect_password(mut self, prompt: impl Display, input: String) -> Self {
			self.password_expectations.insert(0, (prompt.to_string(), input));
			self
		}

		pub(crate) fn expect_select(
			mut self,
			prompt: impl Display,
			required: Option<bool>,
			collect: bool,
			items: Option<Vec<(String, String)>>,
			item: usize,
			filter_mode: Option<bool>,
		) -> Self {
			self.select_expectation
				.insert(0, (prompt.to_string(), required, collect, items, item, filter_mode));
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

		pub(crate) fn expect_plain(mut self, message: impl Display) -> Self {
			self.plain_expectations.push(message.to_string());
			self
		}

		pub(crate) fn verify(self) -> anyhow::Result<()> {
			if !self.confirm_expectation.is_empty() {
				panic!("`{:?}` confirm expectations not satisfied", self.confirm_expectation)
			}
			if !self.error_expectations.is_empty() {
				panic!(
					"`{}` error log expectations not satisfied",
					self.error_expectations.join(",")
				)
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
			if let Some((prompt, _, _, _, _)) = self.multiselect_expectation {
				panic!("`{prompt}` multiselect prompt expectation not satisfied")
			}
			if let Some(expectation) = self.outro_expectation {
				panic!("`{expectation}` outro expectation not satisfied")
			}
			if let Some(expectation) = self.outro_cancel_expectation {
				panic!("`{expectation}` outro cancel expectation not satisfied")
			}
			if !self.password_expectations.is_empty() {
				panic!("`{:?}` password expectation not satisfied", self.password_expectations)
			}
			if !self.select_expectation.is_empty() {
				panic!(
					"`{}` select prompt expectation not satisfied",
					self.select_expectation
						.iter()
						.map(|(s, _, _, _, _, _)| s.clone()) // Extract the `String` part
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
			if !self.plain_expectations.is_empty() {
				panic!(
					"`{}` plain log expectations not satisfied",
					self.plain_expectations.join(",")
				)
			}
			Ok(())
		}
	}

	impl Cli for MockCli {
		fn is_json(&self) -> bool {
			self.json
		}

		fn confirm(&mut self, prompt: impl Display) -> impl Confirm {
			let prompt = prompt.to_string();
			if let Some((expectation, confirm)) = self.confirm_expectation.pop() {
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockConfirm { confirm };
			}
			MockConfirm::default()
		}

		fn error(&mut self, message: impl Display) -> Result<()> {
			let message = message.to_string();
			self.error_expectations.retain(|x| *x != message);
			Ok(())
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
					validate_fn: None,
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
			if let Some((
				expectation,
				required_expectation,
				collect,
				items_expectation,
				filter_mode_expectation,
			)) = self.multiselect_expectation.take()
			{
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockMultiSelect {
					required_expectation,
					items_expectation,
					collect,
					items: vec![],
					filter_mode_expectation,
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

		fn password(&mut self, prompt: impl Display) -> impl Password {
			let prompt = prompt.to_string();
			if let Some((expectation, input)) = self.password_expectations.pop() {
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockPassword { prompt: input.clone() };
			}
			MockPassword::default()
		}

		fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl Select<T> {
			let prompt = prompt.to_string();
			if let Some((
				expectation,
				_,
				collect,
				items_expectation,
				item,
				filter_mode_expectation,
			)) = self.select_expectation.pop()
			{
				assert_eq!(expectation, prompt, "prompt does not satisfy expectation");
				return MockSelect {
					items_expectation,
					collect,
					items: vec![],
					item,
					initial_value: None,
					filter_mode_expectation,
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

		fn plain(&mut self, message: impl Display) -> Result<()> {
			let message = message.to_string();
			self.plain_expectations.retain(|x| *x != message);
			Ok(())
		}

		fn spinner(&mut self) -> Box<dyn Spinner + Send> {
			Box::new(MockSpinner {})
		}

		fn multi_progress(&mut self, _title: impl Display) -> Box<dyn MultiProgress + Send> {
			Box::new(MockMultiProgress {})
		}
	}

	/// Mock spinner
	struct MockSpinner {}

	impl Spinner for MockSpinner {
		fn start(&self, _message: &str) {}
		fn set_message(&self, _message: &str) {}
		fn stop(&self, _message: &str) {}
		fn cancel(&self, _message: &str) {}
		fn error(&self, _message: &str) {}
		fn clear(&self) {}
	}

	/// Mock multi-progress bar
	struct MockMultiProgress {}

	impl MultiProgress for MockMultiProgress {
		fn add(&mut self, _message: &str) -> Box<dyn Spinner + Send> {
			Box::new(MockSpinner {})
		}

		fn stop(&mut self) {}
	}

	/// Mock confirm prompt
	#[derive(Default)]
	struct MockConfirm {
		confirm: bool,
	}

	impl Confirm for MockConfirm {
		fn initial_value(self, _initial_value: bool) -> Self {
			// Ignore initial value and always return mock value
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
		#[allow(unused)]
		placeholder: String,
		#[allow(unused)]
		required: bool,
		validate_fn: Option<Box<dyn Fn(&String) -> std::result::Result<(), &'static str>>>,
	}

	impl Input for MockInput {
		fn interact(&mut self) -> Result<String> {
			if let Some(validator) = &self.validate_fn {
				validator(&self.prompt).map_err(std::io::Error::other)?;
			}
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
			mut self,
			validator: impl Fn(&String) -> std::result::Result<(), &'static str> + 'static,
		) -> Self {
			self.validate_fn = Some(Box::new(validator));
			self
		}
	}

	/// Mock multi-select prompt
	#[allow(dead_code)]
	pub(crate) struct MockMultiSelect<T> {
		required_expectation: Option<bool>,
		items_expectation: Option<Vec<(String, String)>>,
		collect: bool,
		items: Vec<T>,
		filter_mode_expectation: Option<bool>,
	}

	impl<T> MockMultiSelect<T> {
		pub(crate) fn default() -> Self {
			Self {
				required_expectation: None,
				filter_mode_expectation: None,
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

		fn filter_mode(mut self) -> Self {
			if let Some(expectation) = self.filter_mode_expectation.as_ref() {
				assert!(*expectation, "filter mode does not satisfy expectation");
				self.filter_mode_expectation = None;
			}
			self
		}
	}

	/// Mock password prompt
	#[allow(dead_code)]
	#[derive(Default)]
	struct MockPassword {
		prompt: String,
	}

	impl Password for MockPassword {
		fn interact(&mut self) -> Result<String> {
			Ok(self.prompt.clone())
		}
	}

	/// Mock select prompt
	#[allow(dead_code)]
	pub(crate) struct MockSelect<T> {
		items_expectation: Option<Vec<(String, String)>>,
		collect: bool,
		items: Vec<T>,
		item: usize,
		initial_value: Option<T>,
		filter_mode_expectation: Option<bool>,
	}

	impl<T> MockSelect<T> {
		pub(crate) fn default() -> Self {
			Self {
				items_expectation: None,
				collect: false,
				items: vec![],
				item: 0,
				initial_value: None,
				filter_mode_expectation: None,
			}
		}
	}

	impl<T: Clone + Eq> Select<T> for MockSelect<T> {
		fn initial_value(mut self, initial_value: T) -> Self {
			self.initial_value = Some(initial_value);
			self
		}

		fn interact(&mut self) -> Result<T> {
			let item = self.items.get(self.item).ok_or_else(|| {
				std::io::Error::new(
					std::io::ErrorKind::NotFound,
					format!("Missing item at position {}", self.item),
				)
			})?;
			Ok(item.clone())
		}

		fn item(mut self, value: T, label: impl Display, hint: impl Display) -> Self {
			// Check expectations
			if let Some(items) = self.items_expectation.as_mut() {
				let item = (label.to_string(), hint.to_string());
				assert!(
					items.contains(&item),
					"`{item:?}` item does not satisfy any expectations.\nAvailable expectations:\n{items:#?}"
				);
				items.retain(|x| *x != item);
			}
			// Collect if specified
			if self.collect {
				self.items.push(value);
			}
			self
		}

		fn filter_mode(mut self) -> Self {
			if let Some(expectation) = self.filter_mode_expectation.as_ref() {
				assert!(*expectation, "filter mode does not satisfy expectation");
				self.filter_mode_expectation = None;
			}
			self
		}
	}
}
