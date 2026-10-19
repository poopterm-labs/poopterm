//! iTerm2's tab chrome: the tab color (OSC 6;1) and session status
//! (OSC 21337).
use crate::term::{
	Term,
	tab_style::{MAX_STATUS, TabStyle},
};

impl Term {
	/// `OSC 6;1;bg;{red,green,blue};brightness;N` sets one channel of the
	/// tab color; `OSC 6;1;bg;*;default` removes it.
	pub(super) fn osc_tab_color(&mut self, params: &[&[u8]]) {
		let color = match params {
			[b"1", b"bg", b"*", b"default"] => None,
			[b"1", b"bg", channel, b"brightness", value] => {
				let shift = match *channel {
					b"red" => 16,
					b"green" => 8,
					b"blue" => 0,
					_ => return,
				};
				let Some(value) = std::str::from_utf8(value)
					.ok()
					.and_then(|value| value.parse::<u8>().ok())
				else {
					return;
				};
				let color = self.tab_style.color.unwrap_or(0) & !(0xff << shift);
				Some(color | u32::from(value) << shift)
			}
			_ => return,
		};
		self.restyle_tab(|style| style.color = color);
	}

	/// `OSC 21337;key=value;…` with `indicator`, `status` and
	/// `status-color`; an empty value clears a field.
	pub(super) fn osc_session_status(&mut self, params: &[&[u8]]) {
		let mut style = self.tab_style.clone();
		for field in params {
			let Some(at) = field.iter().position(|&byte| byte == b'=') else {
				continue;
			};
			let (key, value) = (&field[..at], &field[at + 1..]);
			match key {
				b"indicator" => {
					if let Some(color) = hex_color(value) {
						style.indicator = color;
					}
				}
				b"status-color" => {
					if let Some(color) = hex_color(value) {
						style.status_color = color;
					}
				}
				b"status" => {
					let text: String = String::from_utf8_lossy(value)
						.chars()
						.filter(|c| !c.is_control())
						.take(MAX_STATUS)
						.collect();
					style.status = (!text.is_empty()).then_some(text);
				}
				_ => {}
			}
		}
		self.restyle_tab(|current| *current = style);
	}

	/// Changes the tab style, and redraws the tab if it changed.
	fn restyle_tab(&mut self, change: impl FnOnce(&mut TabStyle)) {
		let before = self.tab_style.clone();
		change(&mut self.tab_style);
		if self.tab_style != before {
			self.title_generation = self.title_generation.wrapping_add(1);
		}
	}
}

/// `#rrggbb` as `Some(Some(color))`, empty as `Some(None)`, else `None`.
fn hex_color(value: &[u8]) -> Option<Option<u32>> {
	if value.is_empty() {
		return Some(None);
	}
	let digits = value
		.strip_prefix(b"#")
		.filter(|digits| digits.len() == 6 && digits.iter().all(u8::is_ascii_hexdigit))?;
	let digits = std::str::from_utf8(digits).ok()?;
	u32::from_str_radix(digits, 16).ok().map(Some)
}
