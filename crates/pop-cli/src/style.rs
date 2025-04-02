// SPDX-License-Identifier: GPL-3.0

use cliclack::ThemeState;
#[cfg(any(feature = "parachain", feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) use console::style;
use console::Style;

pub(crate) fn get_styles() -> clap::builder::Styles {
	use clap::builder::styling::{AnsiColor, Color, Style};
	clap::builder::Styles::styled()
		.usage(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightCyan))))
		.header(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::BrightCyan))))
		.literal(Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightMagenta))))
		.invalid(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red))))
		.error(Style::new().bold().fg_color(Some(Color::Ansi(AnsiColor::Red))))
		.valid(
			Style::new()
				.bold()
				.underline()
				.fg_color(Some(Color::Ansi(AnsiColor::BrightMagenta))),
		)
		.placeholder(Style::new().fg_color(Some(Color::Ansi(AnsiColor::White))))
}

pub(crate) struct Theme;

impl cliclack::Theme for Theme {
	fn bar_color(&self, state: &ThemeState) -> Style {
		match state {
			ThemeState::Active => Style::new().bright().magenta(),
			ThemeState::Error(_) => Style::new().bright().red(),
			_ => Style::new().magenta().dim(),
		}
	}

	fn state_symbol_color(&self, _state: &ThemeState) -> Style {
		Style::new().bright().magenta()
	}

	fn info_symbol(&self) -> String {
		"⚙".into()
	}
}

/// Formats a URL with bold and underlined style.
pub(crate) fn format_url(url: &str) -> String {
	format!("{}", style(url).bold().underlined())
}

/// Formats the step label if steps should be shown.
pub(crate) fn format_step_prefix(current: usize, total: usize, show: bool) -> String {
	show.then(|| format!("[{}/{}]: ", current, total)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
	use super::*;
	use console::Style;

	#[test]
	fn format_provider_url_works() {
		let url = "https://example.com";
		assert_eq!(format_url(url), format!("{}", Style::new().bold().underlined().apply_to(url)));
	}

	#[test]
	fn format_step_prefix_works() {
		assert_eq!(format_step_prefix(2, 5, true), "[2/5]: ");
		assert_eq!(format_step_prefix(2, 5, false), "");
	}
}
