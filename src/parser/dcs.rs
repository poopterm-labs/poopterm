//! Answers to device control string requests: DECRQSS reports a setting
//! as the control sequence that would set it.
use crate::term::{
	CursorShape, Term,
	attr::*,
	dcs::{Kind, Request},
};

impl Term {
	/// Answers a finished request.
	pub(super) fn answer_dcs(&mut self, request: Request) {
		let data = if request.overflow {
			None
		} else {
			Some(&request.data[..])
		};
		let reply = match request.kind {
			Kind::Status => match data.and_then(|setting| self.setting(setting)) {
				Some(setting) => format!("\x1bP1$r{setting}\x1b\\"),
				None => "\x1bP0$r\x1b\\".to_owned(),
			},
			Kind::Capabilities => return,
		};
		self.responses.push(reply.into_bytes());
	}

	/// The control sequence (without `CSI`) that sets `setting`.
	fn setting(&self, setting: &[u8]) -> Option<String> {
		Some(match setting {
			b"m" => format!("{}m", self.rendition()),
			b"r" => format!("{};{}r", self.scroll_top + 1, self.scroll_bottom + 1),
			b" q" => match self.cursor_shape {
				CursorShape::Block => "2 q",
				CursorShape::Underline => "4 q",
				CursorShape::Bar => "6 q",
			}
			.to_owned(),
			// A VT220 (as primary DA says) with 7-bit controls.
			b"\"p" => "62;1\"p".to_owned(),
			// No characters are protected.
			b"\"q" => "0\"q".to_owned(),
			b"t" => format!("{}t", self.rows),
			b"$|" => format!("{}$|", self.cols),
			b"*|" => format!("{}*|", self.rows),
			_ => return None,
		})
	}

	/// The current SGR parameters, starting from a reset. Colors equal to
	/// their palette entry are given by number, others as RGB.
	fn rendition(&self) -> String {
		let mut parts = vec!["0".to_owned()];
		let flags = [
			(self.bold, "1"),
			(self.attrs & DIM != 0, "2"),
			(self.attrs & ITALIC != 0, "3"),
			(
				self.attrs & UNDERLINE != 0 && self.attrs & DOUBLE_UNDERLINE == 0,
				"4",
			),
			(self.attrs & BLINK != 0, "5"),
			(self.reverse, "7"),
			(self.attrs & CONCEAL != 0, "8"),
			(self.attrs & STRIKE != 0, "9"),
			(self.attrs & DOUBLE_UNDERLINE != 0, "21"),
			(self.attrs & OVERLINE != 0, "53"),
		];
		parts.extend(
			flags
				.iter()
				.filter(|(on, _)| *on)
				.map(|(_, code)| (*code).to_owned()),
		);
		let fg = if self.special_colors_active() {
			self.sgr_fg
		} else {
			self.fg
		};
		if fg != self.default_fg {
			let index = u8::try_from(self.fg_id)
				.ok()
				.filter(|&index| self.indexed_color(index) == fg);
			parts.push(match index {
				Some(index @ 0..8) => format!("{}", 30 + index),
				Some(index @ 8..16) => format!("{}", 90 + index - 8),
				Some(index) => format!("38:5:{index}"),
				None => format!("38:2::{}", rgb(fg)),
			});
		}
		if self.bg != self.default_bg {
			let index = (0..16).find(|&index| self.indexed_color(index) == self.bg);
			parts.push(match index {
				Some(index @ 0..8) => format!("{}", 40 + index),
				Some(index) => format!("{}", 100 + index - 8),
				None => format!("48:2::{}", rgb(self.bg)),
			});
		}
		parts.join(";")
	}
}

/// `r:g:b` in decimal.
fn rgb(color: u32) -> String {
	format!(
		"{}:{}:{}",
		color >> 16 & 0xff,
		color >> 8 & 0xff,
		color & 0xff
	)
}
