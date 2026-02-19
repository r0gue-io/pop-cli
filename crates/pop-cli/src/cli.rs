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
		/// Creates a new spinner.
		fn spinner(&self) -> super::Spinner;
		/// Creates a new multi-progress container.
		fn multi_progress(&self, msg: impl Display) -> super::MultiProgress;
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
pub(crate) struct Cli;
impl traits::Cli for Cli {
	/// Constructs a new [`Confirm`] prompt.
	fn confirm(&mut self, prompt: impl Display) -> impl traits::Confirm {
		Confirm(cliclack::confirm(prompt))
	}

	/// Prints an error message.
	fn error(&mut self, text: impl Display) -> Result<()> {
		cliclack::log::error(text)
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

	/// Constructs a new [`Password`] prompt.
	fn password(&mut self, prompt: impl Display) -> impl traits::Password {
		Password(cliclack::password(prompt))
	}

	/// Constructs a new [`Select`] prompt.
	fn select<T: Clone + Eq>(&mut self, prompt: impl Display) -> impl traits::Select<T> {
		Select::<T>(cliclack::select(prompt).max_rows(35))
	}

	/// Prints a success message.
	fn success(&mut self, message: impl Display) -> Result<()> {
		cliclack::log::success(message)
	}

	/// Prints a warning message.
	fn warning(&mut self, message: impl Display) -> Result<()> {
		cliclack::log::warning(message)?;
		#[cfg(not(test))]
		sleep(Duration::from_secs(1));
		Ok(())
	}

	fn plain(&mut self, message: impl Display) -> Result<()> {
		println!("{message}");
		Ok(())
	}

	fn spinner(&self) -> Spinner {
		Spinner::Human(cliclack::spinner())
	}

	fn multi_progress(&self, msg: impl Display) -> MultiProgress {
		MultiProgress::Human(cliclack::multi_progress(msg))
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
#[allow(dead_code)]
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

	/// The filter mode allows to filter the items by typing.
	fn filter_mode(mut self) -> Self {
		self.0 = self.0.filter_mode();
		self
	}
}

/// A password prompt using cliclack.
#[allow(dead_code)]
struct Password(cliclack::Password);
impl traits::Password for Password {
	/// Starts the prompt interaction.
	fn interact(&mut self) -> Result<String> {
		self.0.interact()
	}
}

/// A select prompt using cliclack.
#[allow(dead_code)]
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

	/// The filter mode allows to filter the items by typing.
	fn filter_mode(mut self) -> Self {
		self.0 = self.0.filter_mode();
		self
	}
}

/// A progress spinner that adapts to the current output mode.
///
/// In human mode, wraps a [`cliclack::ProgressBar`] for interactive terminal output.
/// In JSON mode, sends diagnostic messages to stderr.
/// In test mode, silently discards all output.
#[allow(dead_code)]
pub(crate) enum Spinner {
	Human(cliclack::ProgressBar),
	Json,
	#[cfg(test)]
	Mock,
}

#[allow(dead_code)]
impl Spinner {
	/// Starts the spinner with the given message.
	pub(crate) fn start(&self, msg: impl Display) {
		match self {
			Self::Human(pb) => pb.start(msg),
			Self::Json => eprintln!("{msg}"),
			#[cfg(test)]
			Self::Mock => {},
		}
	}

	/// Stops the spinner with a final message.
	pub(crate) fn stop(&self, msg: impl Display) {
		match self {
			Self::Human(pb) => pb.stop(msg),
			Self::Json => eprintln!("{msg}"),
			#[cfg(test)]
			Self::Mock => {},
		}
	}

	/// Clears the spinner from the terminal.
	pub(crate) fn clear(&self) {
		match self {
			Self::Human(pb) => pb.clear(),
			Self::Json => {},
			#[cfg(test)]
			Self::Mock => {},
		}
	}

	/// Displays an error message on the spinner.
	pub(crate) fn error(&self, msg: impl Display) {
		match self {
			Self::Human(pb) => pb.error(msg),
			Self::Json => eprintln!("{msg}"),
			#[cfg(test)]
			Self::Mock => {},
		}
	}

	/// Updates the spinner message without changing its state.
	pub(crate) fn set_message(&self, msg: impl Display) {
		match self {
			Self::Human(pb) => pb.set_message(msg),
			Self::Json => {},
			#[cfg(test)]
			Self::Mock => {},
		}
	}
}

/// A multi-progress container that adapts to the current output mode.
#[allow(dead_code)]
pub(crate) enum MultiProgress {
	Human(cliclack::MultiProgress),
	#[allow(dead_code)]
	Json,
	#[cfg(test)]
	Mock,
}

#[allow(dead_code)]
impl MultiProgress {
	/// Adds a new spinner to this multi-progress group.
	pub(crate) fn add(&self) -> Spinner {
		match self {
			Self::Human(mp) => Spinner::Human(mp.add(cliclack::spinner())),
			Self::Json => Spinner::Json,
			#[cfg(test)]
			Self::Mock => Spinner::Mock,
		}
	}

	/// Stops the multi-progress container.
	pub(crate) fn stop(&self) {
		match self {
			Self::Human(mp) => mp.stop(),
			Self::Json => {},
			#[cfg(test)]
			Self::Mock => {},
		}
	}
}

/// A CLI implementation for `--json` mode.
///
/// Suppresses all human-facing output (intro, outro, plain) and redirects
/// diagnostic messages (info, success, warning, error) to stderr.
/// All interactive prompts return an error telling the caller that `--json`
/// mode cannot drive an interactive session.
#[allow(dead_code)]
pub(crate) struct JsonCli;

impl traits::Cli for JsonCli {
	fn confirm(&mut self, _prompt: impl Display) -> impl traits::Confirm {
		JsonConfirm
	}
	fn error(&mut self, text: impl Display) -> Result<()> {
		eprintln!("{text}");
		Ok(())
	}
	fn info(&mut self, text: impl Display) -> Result<()> {
		eprintln!("{text}");
		Ok(())
	}
	fn input(&mut self, _prompt: impl Display) -> impl traits::Input {
		JsonInput
	}
	fn intro(&mut self, _title: impl Display) -> Result<()> {
		Ok(())
	}
	fn multiselect<T: Clone + Eq>(&mut self, _prompt: impl Display) -> impl traits::MultiSelect<T> {
		JsonMultiSelect(std::marker::PhantomData)
	}
	fn outro(&mut self, _message: impl Display) -> Result<()> {
		Ok(())
	}
	fn outro_cancel(&mut self, _message: impl Display) -> Result<()> {
		Ok(())
	}
	fn password(&mut self, _prompt: impl Display) -> impl traits::Password {
		JsonPassword
	}
	fn select<T: Clone + Eq>(&mut self, _prompt: impl Display) -> impl traits::Select<T> {
		JsonSelect(std::marker::PhantomData)
	}
	fn success(&mut self, message: impl Display) -> Result<()> {
		eprintln!("{message}");
		Ok(())
	}
	fn warning(&mut self, message: impl Display) -> Result<()> {
		eprintln!("{message}");
		Ok(())
	}
	fn plain(&mut self, _message: impl Display) -> Result<()> {
		Ok(())
	}

	fn spinner(&self) -> Spinner {
		Spinner::Json
	}

	fn multi_progress(&self, _msg: impl Display) -> MultiProgress {
		MultiProgress::Json
	}
}

#[allow(dead_code)]
const JSON_PROMPT_ERR: &str = "interactive prompt required but --json mode is active";

#[allow(dead_code)]
struct JsonConfirm;
impl traits::Confirm for JsonConfirm {
	fn initial_value(self, _initial_value: bool) -> Self {
		self
	}
	fn interact(&mut self) -> Result<bool> {
		Err(std::io::Error::other(JSON_PROMPT_ERR))
	}
}

#[allow(dead_code)]
struct JsonInput;
impl traits::Input for JsonInput {
	fn default_input(self, _value: &str) -> Self {
		self
	}
	fn interact(&mut self) -> Result<String> {
		Err(std::io::Error::other(JSON_PROMPT_ERR))
	}
	fn placeholder(self, _value: &str) -> Self {
		self
	}
	fn required(self, _required: bool) -> Self {
		self
	}
	fn validate(
		self,
		_validator: impl Fn(&String) -> std::result::Result<(), &'static str> + 'static,
	) -> Self {
		self
	}
}

#[allow(dead_code)]
struct JsonMultiSelect<T>(std::marker::PhantomData<T>);
impl<T: Clone + Eq> traits::MultiSelect<T> for JsonMultiSelect<T> {
	fn interact(&mut self) -> Result<Vec<T>> {
		Err(std::io::Error::other(JSON_PROMPT_ERR))
	}
	fn item(self, _value: T, _label: impl Display, _hint: impl Display) -> Self {
		self
	}
	fn required(self, _required: bool) -> Self {
		self
	}
	fn filter_mode(self) -> Self {
		self
	}
}

#[allow(dead_code)]
struct JsonPassword;
impl traits::Password for JsonPassword {
	fn interact(&mut self) -> Result<String> {
		Err(std::io::Error::other(JSON_PROMPT_ERR))
	}
}

#[allow(dead_code)]
struct JsonSelect<T>(std::marker::PhantomData<T>);
impl<T: Clone + Eq> traits::Select<T> for JsonSelect<T> {
	fn initial_value(self, _initial_value: T) -> Self {
		self
	}
	fn interact(&mut self) -> Result<T> {
		Err(std::io::Error::other(JSON_PROMPT_ERR))
	}
	fn item(self, _value: T, _label: impl Display, _hint: impl Display) -> Self {
		self
	}
	fn filter_mode(self) -> Self {
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

		fn spinner(&self) -> super::Spinner {
			super::Spinner::Mock
		}

		fn multi_progress(&self, _msg: impl Display) -> super::MultiProgress {
			super::MultiProgress::Mock
		}
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
