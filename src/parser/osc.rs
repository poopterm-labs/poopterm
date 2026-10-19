//! Operating System Commands: titles, hyperlinks and cwd; colors, the
//! clipboard and notifications have their own modules.
use std::sync::Arc;

use crate::term::{NO_EXTRA, Term};

impl Term {
	pub(super) fn apply_osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		let Some(command) = params.first().copied() else {
			return;
		};
		if self.apply_osc_color(command, &params[1..], bell) {
			return;
		}
		match command {
			b"52" if params.len() >= 3 => self.osc_selection(params[1], params[2]),
			b"5522" if params.len() >= 2 => {
				self.osc_kitty_clipboard(params[1], params.get(2).copied().unwrap_or_default());
			}
			b"9" => self.osc_notify(&params[1..]),
			b"777" => self.osc_notify_777(&params[1..]),
			b"22" => self.osc_pointer(params.get(1).copied().unwrap_or_default(), bell),
			b"7501" => self.osc_program_status(&params[1..], bell),
			b"99" if params.len() >= 3 => self.osc_kitty_notify(params[1], &params[2..], bell),
			b"8" if params.len() >= 3 => {
				let uri = params[2];
				if uri.is_empty() {
					self.active_hyperlink = 0;
					self.active_extra = NO_EXTRA;
				} else if uri.len() <= 2048
					&& let Ok(uri) = std::str::from_utf8(uri)
					&& !uri.chars().any(char::is_control)
					&& self.hyperlinks.len() < 4096
				{
					self.hyperlinks.push(uri.to_owned());
					self.active_hyperlink = (self.hyperlinks.len() - 1) as u16;
					self.active_extra = Arc::make_mut(&mut self.extras)
						.with_hyperlink(NO_EXTRA, self.active_hyperlink)
						.unwrap_or_default();
				}
			}
			b"133" => {
				let letter = params.get(1).copied().unwrap_or_default();
				if matches!(letter, b"L" | b"A" | b"N") {
					self.fresh_line();
				}
				let secondary = params[2.min(params.len())..]
					.iter()
					.any(|option| option.starts_with(b"k=") && *option != b"k=i");
				if matches!(letter, b"A" | b"N") && !secondary {
					self.status.prompt();
				}
				self.shell.record(
					params,
					self.history_base + self.scrollback.len() + self.cy,
					self.cx,
					std::time::Instant::now(),
				);
			}
			b"0" | b"1" | b"2" if params.len() >= 2 => {
				let title = clean_title(params[1]);
				if params[0] != b"2" {
					self.icon_name.clone_from(&title);
				}
				if params[0] != b"1" {
					self.set_title(title);
				}
			}
			b"7" if params.len() >= 2 => self.osc_cwd_uri(params[1]),
			b"633" => self.osc_vscode(&params[1..]),
			b"3008" => self.contexts.apply(&params[1..]),
			b"1337" if params.len() >= 2 => self.osc_iterm(&params[1..], bell),
			b"21337" => self.osc_session_status(&params[1..]),
			b"66" => self.osc_text_size(&params[1..]),
			_ => {}
		}
	}
}

impl Term {
	/// Sets the window title, redrawing the tab if it changed.
	pub(super) fn set_title(&mut self, title: String) {
		if self.title != title {
			self.title = title;
			self.title_generation = self.title_generation.wrapping_add(1);
		}
	}
}

/// A title or icon name without control characters, at most 64 characters.
pub(super) fn clean_title(text: &[u8]) -> String {
	String::from_utf8_lossy(text)
		.chars()
		.filter(|c| !c.is_control())
		.take(64)
		.collect()
}
