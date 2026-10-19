//! Operating System Commands: titles, hyperlinks and cwd; colors, the
//! clipboard and notifications have their own modules.
use std::sync::Arc;

use crate::{
	shell_integration::decode_uri_path,
	term::{NO_EXTRA, Term},
};

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
			b"133" => self.shell.record(
				params,
				self.history_base + self.scrollback.len() + self.cy,
				self.cx,
				std::time::Instant::now(),
			),
			b"0" | b"1" | b"2" if params.len() >= 2 => {
				let title = String::from_utf8_lossy(params[1]);
				let title: String = title.chars().filter(|c| !c.is_control()).take(64).collect();
				if self.title != title {
					self.title = title;
					self.title_generation = self.title_generation.wrapping_add(1);
				}
			}
			b"7" if params.len() >= 2 => {
				if let Ok(uri) = std::str::from_utf8(params[1])
					&& let Some(path) = uri.strip_prefix("file://")
					&& let Some(path) = path
						.find('/')
						.and_then(|index| decode_uri_path(&path[index..]))
					&& self.cwd != path
				{
					self.cwd = path;
					self.cwd_generation = self.cwd_generation.wrapping_add(1);
				}
			}
			_ => {}
		}
	}
}
