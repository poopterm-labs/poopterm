//! Two-byte `ESC` sequences: charsets, index, reset and cursor save.
use crate::{
	term::Term,
	theme::{Theme, ThemeKind},
};

impl Term {
	pub(super) fn apply_esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		if ignore {
			return;
		}
		if matches!(intermediates, b"(" | b")") {
			if matches!(byte, b'0' | b'B') {
				self.charsets[usize::from(intermediates == b")")] = byte == b'0';
			}
			return;
		}
		if !intermediates.is_empty() {
			return;
		}
		if !matches!(byte, b'7' | b'8') {
			self.wrap_pending = false;
		}
		match byte {
			b'H' => self.tab_stops[self.cx] = true,
			b'c' => {
				let theme = Theme {
					fg: self.default_fg,
					bg: self.default_bg,
					palette: self.palette,
					..Theme::from_kind(ThemeKind::Poop)
				};
				let (cw, ch, scale) = (self.cell_width, self.cell_height, self.scale);
				let limit = self.scrollback_limit;
				let contexts = std::mem::take(&mut self.contexts);
				*self = Self::new(self.cols, self.rows, theme, self.cwd.clone());
				self.contexts = contexts;
				self.cell_width = cw;
				self.cell_height = ch;
				self.scale = scale;
				self.scrollback_limit = limit;
			}
			b'M' => {
				if self.cy == self.scroll_top {
					self.scroll_region_down(1);
				} else {
					self.cy = self.cy.saturating_sub(1);
				}
			}
			b'D' => self.linefeed(),
			b'E' => {
				self.cx = 0;
				self.linefeed();
			}
			b'7' => self.save_cursor(),
			b'8' => self.restore_cursor(),
			_ => {}
		}
	}
}
