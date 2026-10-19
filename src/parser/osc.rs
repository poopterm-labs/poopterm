//! Operating System Commands: titles, hyperlinks, clipboard, cwd; colors are
//! in `osc_color`.
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
			b"52" if params.len() >= 3 && params[1] == b"c" && params[2] != b"?" => {
				// OSC 52 clipboard reads are deliberately not answered: doing so would
				// disclose the user's clipboard to any process running in the shell.
				if let Some(bytes) = decode_base64(params[2])
					&& let Ok(text) = String::from_utf8(bytes)
				{
					self.clipboard_write = Some(text);
				}
			}
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

/// Most bytes an OSC 52 clipboard write may carry.
const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;

fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
	if input.len() > MAX_CLIPBOARD_BYTES.div_ceil(3) * 4 {
		return None;
	}
	crate::graphics::base64::decode(input)
}
