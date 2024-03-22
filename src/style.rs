// Copyright (C) R0GUE IO LTD.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use cliclack::ThemeState;
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
