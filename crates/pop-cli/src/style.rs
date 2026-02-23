// SPDX-License-Identifier: GPL-3.0

use cliclack::ThemeState;
use console::Style;
pub(crate) use console::style;

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
