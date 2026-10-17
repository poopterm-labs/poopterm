//! Escape-sequence handling using vte's incremental state machine.
use crate::{Cell, Term, Theme, ThemeKind, dec_graphic, decode_uri_path, xterm_color};
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

/// Stateful parser for one PTY stream. Keep it beside `Term` in a session so
/// escape sequences and UTF-8 codepoints split across reads remain intact.
pub(crate) struct Parser {
	inner: vte::Parser,
}

impl Parser {
	pub(crate) fn new() -> Self {
		Self {
			inner: vte::Parser::new(),
		}
	}

	pub(crate) fn advance(&mut self, term: &mut Term, bytes: &[u8]) {
		self.inner.advance(term, bytes);
	}
}

impl Term {
	pub(crate) fn sgr(&mut self, params: &Params) {
		// vte stores at most 32 parameters/subparameters in its parser state.
		// Keep group slices on the stack instead of allocating nested Vecs for
		// every SGR sequence (which is common in full-screen terminal apps).
		let mut groups = [None; 32];
		let mut group_count = 0;
		for group in params.iter() {
			groups[group_count] = Some(group);
			group_count += 1;
		}
		if group_count == 0 {
			self.fg = self.default_fg;
			self.bg = self.default_bg;
			self.attrs = 0;
			self.bold = false;
			self.reverse = false;
			return;
		}
		let mut i = 0;
		while i < group_count {
			let group = groups[i].unwrap_or_default();
			let Some(&code) = group.first() else {
				self.fg = self.default_fg;
				self.bg = self.default_bg;
				self.attrs = 0;
				self.bold = false;
				self.reverse = false;
				i += 1;
				continue;
			};
			if matches!(code, 38 | 48) {
				let foreground = code == 38;
				let (mode, color, consumed) = if group.len() > 1 {
					match group.get(1).copied() {
						Some(5) if group.len() >= 3 => {
							(5, Some(self.indexed_color(group[2].min(255) as u8)), 0)
						}
						Some(2) if group.len() >= 5 => {
							let start = if group.len() >= 6 && group[2] == 0 {
								3
							} else {
								2
							};
							let color = (group.len() >= start + 3)
								.then(|| rgb_color(group[start], group[start + 1], group[start + 2]));
							(2, color, 0)
						}
						_ => (0, None, 0),
					}
				} else if let Some(mode_group) = groups.get(i + 1).and_then(|group| *group) {
					let mode = mode_group.first().copied().unwrap_or(0);
					match mode {
						5 => (
							5,
							groups
								.get(i + 2)
								.and_then(|group| *group)
								.and_then(|g| g.first())
								.map(|v| self.indexed_color((*v).min(255) as u8)),
							2,
						),
						2 => {
							let color = groups
								.get(i + 2)
								.and_then(|group| *group)
								.and_then(|r| r.first())
								.zip(
									groups
										.get(i + 3)
										.and_then(|group| *group)
										.and_then(|g| g.first()),
								)
								.zip(
									groups
										.get(i + 4)
										.and_then(|group| *group)
										.and_then(|b| b.first()),
								)
								.map(|((r, g), b)| rgb_color(*r, *g, *b));
							(2, color, 4)
						}
						_ => (0, None, 0),
					}
				} else {
					(0, None, 0)
				};
				if let Some(color) = color {
					if foreground {
						self.fg = color
					} else {
						self.bg = color
					}
				}
				if mode != 0 {
					i += consumed;
				}
				i += 1;
				continue;
			}
			if code == 4 && group.len() > 1 {
				match group[1] {
					0 => self.attrs &= !(4 | 128),
					2 => self.attrs |= 4 | 128,
					_ => self.attrs |= 4,
				}
				i += 1;
				continue;
			}
			match code {
				0 => {
					self.fg = self.default_fg;
					self.bg = self.default_bg;
					self.attrs = 0;
					self.bold = false;
					self.reverse = false;
				}
				1 => self.bold = true,
				2 => self.attrs |= 1,
				3 => self.attrs |= 2,
				4 => self.attrs |= 4,
				21 => self.attrs |= 4 | 128,
				5 | 6 => self.attrs |= 32,
				7 => self.reverse = true,
				8 => self.attrs |= 8,
				9 => self.attrs |= 16,
				22 => {
					self.bold = false;
					self.attrs &= !1;
				}
				23 => self.attrs &= !2,
				24 => self.attrs &= !(4 | 128),
				25 => self.attrs &= !32,
				27 => self.reverse = false,
				28 => self.attrs &= !8,
				29 => self.attrs &= !16,
				53 => self.attrs |= 64,
				55 => self.attrs &= !64,
				30..=37 => self.fg = self.indexed_color((code - 30) as u8),
				39 => self.fg = self.default_fg,
				40..=47 => self.bg = self.indexed_color((code - 40) as u8),
				49 => self.bg = self.default_bg,
				90..=97 => self.fg = self.indexed_color((code - 90 + 8) as u8),
				100..=107 => self.bg = self.indexed_color((code - 100 + 8) as u8),
				_ => {}
			}
			i += 1;
		}
	}
}

impl Term {
	#[inline(always)]
	fn apply_print(&mut self, c: char) {
		let c = if self.charsets[self.active_charset] {
			dec_graphic(c)
		} else {
			c
		};
		let width = c.width().unwrap_or(0).min(self.cols);
		let mut previous = if self.wrap_pending {
			self.idx(self.cx, self.cy)
		} else if self.cx > 0 {
			self.idx(self.cx - 1, self.cy)
		} else {
			usize::MAX
		};
		if previous < self.cells.len() && self.cells[previous].width == 0 {
			previous = previous.saturating_sub(1);
		}
		let joins_previous = previous < self.cells.len()
			&& self.cells[previous].combining_len > 0
			&& self.cells[previous].combining[self.cells[previous].combining_len as usize - 1]
				== '\u{200d}';
		if (width == 0 || joins_previous) && previous < self.cells.len() {
			let cell = &mut self.cells[previous];
			if (width == 0 || joins_previous) && (cell.combining_len as usize) < cell.combining.len() {
				cell.combining[cell.combining_len as usize] = c;
				cell.combining_len += 1;
			}
			return;
		}
		if width == 0 {
			return;
		}
		if self.wrap_pending || self.cx + width > self.cols {
			if self.auto_wrap {
				self.cx = 0;
				self.linefeed();
			} else {
				self.cx = self.cols.saturating_sub(width);
			}
		}
		self.wrap_pending = false;
		let i = self.idx(self.cx, self.cy);
		if self.insert_mode {
			let count = width.min(self.cols - self.cx);
			let row_end = (self.cy + 1) * self.cols;
			self.cells.copy_within(i..row_end - count, i + count);
			self.clear_range(i, i + count);
			self.repair_wide_row(self.cy);
		}
		self.erase_images(i, i + width);
		// Remove both halves when overwriting a wide character.
		if self.cells[i].width == 0 && self.cx > 0 {
			self.cells[i - 1] = self.blank();
		}
		if self.cells[i].width == 2 && self.cx + 1 < self.cols {
			self.cells[i + 1] = self.blank();
		}
		if width == 2
			&& self.cx + 1 < self.cols
			&& self.cells[i + 1].width == 2
			&& self.cx + 2 < self.cols
		{
			self.cells[i + 2] = self.blank();
		}
		let mut fg = self.fg;
		let mut bg = self.bg;
		if self.bold
			&& let Some(index) = (0..8).find(|&index| self.indexed_color(index as u8) == fg)
		{
			fg = self.indexed_color((index + 8) as u8);
		}
		if self.reverse {
			std::mem::swap(&mut fg, &mut bg);
		}
		if self.attrs & 1 != 0 {
			fg = crate::blend(bg, fg, 160);
		}
		if self.attrs & 8 != 0 {
			fg = bg;
		}
		self.cells[i] = Cell {
			attrs: self.attrs,
			ch: c,
			combining: ['\0'; 8],
			combining_len: 0,
			fg,
			bg,
			width: width as u8,
			hyperlink: self.active_hyperlink,
		};
		if width == 2 && self.cx + 1 < self.cols {
			self.cells[i + 1] = Cell {
				attrs: self.attrs,
				ch: ' ',
				combining: ['\0'; 8],
				combining_len: 0,
				fg,
				bg,
				width: 0,
				hyperlink: self.active_hyperlink,
			};
		}
		self.wrap_pending = self.cx + width >= self.cols;
		self.cx = (self.cx + width).min(self.cols - 1);
	}

	#[inline]
	fn apply_execute(&mut self, byte: u8) {
		if matches!(byte, 8..=13) {
			self.wrap_pending = false;
		}
		match byte {
			0x0e => self.active_charset = 1,
			0x0f => self.active_charset = 0,
			7 => self.bell = true,
			0x84 => self.linefeed(),
			0x85 => {
				self.cx = 0;
				self.linefeed();
			}
			0x88 => self.tab_stops[self.cx] = true,
			0x8d => {
				if self.cy == self.scroll_top {
					self.scroll_region_down(1);
				} else {
					self.cy = self.cy.saturating_sub(1);
				}
			}
			b'\n' | 0x0b | 0x0c => self.linefeed(),
			b'\r' => self.cx = 0,
			0x08 => self.cx = self.cx.saturating_sub(1),
			b'\t' => {
				self.cx = (self.cx + 1..self.cols)
					.find(|&col| self.tab_stops[col])
					.unwrap_or(self.cols - 1)
			}
			_ => {}
		}
	}

	fn apply_csi_dispatch(
		&mut self,
		params: &Params,
		intermediates: &[u8],
		ignore: bool,
		action: char,
	) {
		if ignore {
			return;
		}
		let private = intermediates == b"?";
		if !intermediates.is_empty() && !private {
			if action == 'u' && intermediates == b">" {
				self.kitty_keyboard = params.iter().any(|p| p.first() == Some(&1));
				return;
			}
			if action == 'u' && intermediates == b"<" {
				self.kitty_keyboard = false;
				return;
			}
			return;
		}
		if !matches!(action, 'm' | 'n' | 'c' | 'h' | 'l' | 's' | 'u') {
			self.wrap_pending = false;
		}

		// Optimized parameter extraction helper avoiding heavy iterator re-allocations
		let p = |i: usize, d: u16| {
			params
				.iter()
				.nth(i)
				.and_then(|s| s.first().copied())
				.filter(|&v| v != 0)
				.unwrap_or(d) as usize
		};
		let raw0 = params
			.iter()
			.next()
			.and_then(|s| s.first().copied())
			.unwrap_or(0);

		if private && matches!(action, 'h' | 'l') {
			let enabled = action == 'h';
			for p in params.iter().filter_map(|s| s.first().copied()) {
				match p {
					1 => self.application_cursor = enabled,
					6 => {
						self.origin_mode = enabled;
						self.cx = 0;
						self.cy = if enabled { self.scroll_top } else { 0 };
						self.wrap_pending = false;
					}
					7 => self.auto_wrap = enabled,
					25 => self.cursor_visible = enabled,
					80 => self.sixel_scrolling = enabled,
					1048 => {
						if enabled {
							self.save_cursor();
						} else {
							self.restore_cursor();
						}
					}
					1049 | 47 | 1047 => self.alternate_screen(enabled),
					1000 | 1002 | 1003 => self.mouse_tracking = if enabled { p } else { 0 },
					1006 => self.mouse_sgr = enabled,
					2004 => self.bracketed_paste = enabled,
					_ => {}
				}
			}
			return;
		}
		if private && action == 'u' {
			self
				.responses
				.push(format!("\x1b[?{}u", if self.kitty_keyboard { 1 } else { 0 }).into_bytes());
			return;
		}

		if private {
			return;
		}
		if matches!(action, 'h' | 'l') {
			let enabled = action == 'h';
			for mode in params.iter().filter_map(|param| param.first().copied()) {
				if mode == 4 {
					self.insert_mode = enabled;
				}
			}
			return;
		}
		match action {
			'g' => match raw0 {
				0 => self.tab_stops[self.cx] = false,
				3 => self.tab_stops.fill(false),
				_ => {}
			},
			'I' | 'Z' => {
				for _ in 0..p(0, 1).min(self.cols) {
					self.cx = if action == 'I' {
						(self.cx + 1..self.cols)
							.find(|&col| self.tab_stops[col])
							.unwrap_or(self.cols - 1)
					} else {
						(0..self.cx)
							.rev()
							.find(|&col| self.tab_stops[col])
							.unwrap_or(0)
					};
				}
			}
			'd' => {
				self.cy = (p(0, 1) - 1 + if self.origin_mode { self.scroll_top } else { 0 }).min(
					if self.origin_mode {
						self.scroll_bottom
					} else {
						self.rows - 1
					},
				);
			}
			'E' | 'F' => {
				let top = if self.origin_mode { self.scroll_top } else { 0 };
				let bottom = if self.origin_mode {
					self.scroll_bottom
				} else {
					self.rows - 1
				};
				self.cy = if action == 'E' {
					(self.cy + p(0, 1)).min(bottom)
				} else {
					self.cy.saturating_sub(p(0, 1)).max(top)
				};
				self.cx = 0;
			}
			'A' => {
				let top = if self.origin_mode { self.scroll_top } else { 0 };
				self.cy = self.cy.saturating_sub(p(0, 1)).max(top);
			}
			'B' => {
				self.cy = (self.cy + p(0, 1)).min(if self.origin_mode {
					self.scroll_bottom
				} else {
					self.rows - 1
				})
			}
			'C' => self.cx = (self.cx + p(0, 1)).min(self.cols - 1),
			'D' => self.cx = self.cx.saturating_sub(p(0, 1)),
			'G' | '`' => self.cx = (p(0, 1) - 1).min(self.cols - 1),
			'H' | 'f' => {
				let row = p(0, 1) - 1 + if self.origin_mode { self.scroll_top } else { 0 };
				self.cy = row.min(if self.origin_mode {
					self.scroll_bottom
				} else {
					self.rows - 1
				});
				self.cx = (p(1, 1) - 1).min(self.cols - 1);
			}
			'r' => {
				let top = (p(0, 1) - 1).min(self.rows - 1);
				let bottom = (p(1, self.rows as u16) - 1).min(self.rows - 1);
				if top < bottom {
					self.scroll_top = top;
					self.scroll_bottom = bottom;
					self.cx = 0;
					self.cy = if self.origin_mode { top } else { 0 };
				}
			}
			'S' => self.scroll_region_up(p(0, 1)),
			'T' => self.scroll_region_down(p(0, 1)),
			's' => self.save_cursor(),
			'u' => self.restore_cursor(),
			'J' => match raw0 {
				0 => {
					let from = self.idx(self.cx, self.cy);
					self.clear_range(from, self.cells.len());
				}
				1 => self.clear_range(0, self.idx(self.cx + 1, self.cy)),
				2 => self.clear_range(0, self.cells.len()),
				3 => {
					self.history_base = self.history_base.saturating_add(self.scrollback.len());
					self.scrollback.clear();
					self.scroll_offset = 0;
				}
				_ => {}
			},
			'K' => {
				let row_start = self.idx(0, self.cy);
				let row_end = row_start + self.cols;
				match raw0 {
					0 => self.clear_range(self.idx(self.cx, self.cy), row_end),
					1 => self.clear_range(row_start, self.idx(self.cx + 1, self.cy)),
					2 => self.clear_range(row_start, row_end),
					_ => {}
				}
			}
			'@' => {
				self.erase_images(self.cy * self.cols, (self.cy + 1) * self.cols);
				let count = p(0, 1).min(self.cols - self.cx);
				let start = self.idx(self.cx, self.cy);
				let end = self.idx(self.cols - count, self.cy);
				self.cells.copy_within(start..end, start + count);
				self.clear_range(start, start + count);
			}
			'P' => {
				self.erase_images(self.cy * self.cols, (self.cy + 1) * self.cols);
				let count = p(0, 1).min(self.cols - self.cx);
				let start = self.idx(self.cx, self.cy);
				let end = self.idx(self.cols, self.cy);
				self.cells.copy_within(start + count..end, start);
				self.clear_range(end - count, end);
			}
			'X' => {
				let start = self.idx(self.cx, self.cy);
				self.clear_range(start, start + p(0, 1).min(self.cols - self.cx));
			}
			'L' | 'M' if self.cy >= self.scroll_top && self.cy <= self.scroll_bottom => {
				self.erase_images(self.cy * self.cols, (self.scroll_bottom + 1) * self.cols);
				let bottom = self.scroll_bottom.max(self.cy).min(self.rows - 1);
				let count = p(0, 1).min(bottom + 1 - self.cy);
				let start = self.cy * self.cols;
				let end = (bottom + 1) * self.cols;
				if action == 'L' {
					self
						.cells
						.copy_within(start..end - count * self.cols, start + count * self.cols);
					self.clear_range(start, start + count * self.cols);
				} else {
					self
						.cells
						.copy_within(start + count * self.cols..end, start);
					self.clear_range(end - count * self.cols, end);
				}
			}
			'm' => self.sgr(params),
			'n' => match raw0 {
				5 => self.responses.push(b"\x1b[0n".to_vec()),
				6 => self
					.responses
					.push(format!("\x1b[{};{}R", self.cy + 1, self.cx.min(self.cols - 1) + 1).into_bytes()),
				_ => {}
			},
			'c' => self.responses.push(b"\x1b[?62;4;6c".to_vec()),
			_ => {}
		}
		if matches!(action, '@' | 'P' | 'X' | 'J' | 'K') {
			self.repair_wide_cells();
		}
	}

	fn apply_esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
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
				let (cw, ch, limit) = (self.cell_width, self.cell_height, self.scrollback_limit);
				*self = Self::new(self.cols, self.rows, theme, self.cwd.clone());
				self.cell_width = cw;
				self.cell_height = ch;
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

	fn apply_osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
		let Some(command) = params.first().copied() else {
			return;
		};
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
				} else if uri.len() <= 2048
					&& let Ok(uri) = std::str::from_utf8(uri)
					&& !uri.chars().any(char::is_control)
					&& self.hyperlinks.len() < 4096
				{
					self.hyperlinks.push(uri.to_owned());
					self.active_hyperlink = (self.hyperlinks.len() - 1) as u16;
				}
			}
			b"133" => self.shell.record(
				params,
				self.history_base + self.scrollback.len() + self.cy,
				self.cx,
			),
			b"0" | b"1" | b"2" if params.len() >= 2 => {
				let title = String::from_utf8_lossy(params[1]);
				let title: String = title.chars().filter(|c| !c.is_control()).take(64).collect();
				if self.title != title {
					self.title = title;
					self.title_generation = self.title_generation.wrapping_add(1);
				}
			}
			b"4" => self.osc_palette(&params[1..]),
			b"104" => {
				if params.len() == 1 {
					self.color_overrides.fill(None);
				} else {
					for index in params[1..]
						.iter()
						.filter_map(|value| std::str::from_utf8(value).ok()?.parse::<usize>().ok())
					{
						if index < self.color_overrides.len() {
							self.color_overrides[index] = None;
						}
					}
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
			b"10" | b"11" if params.len() >= 2 => {
				let foreground = command == b"10";
				if params[1] == b"?" {
					let color = if foreground {
						self.default_fg
					} else {
						self.default_bg
					};
					self.responses.push(osc_color_reply(command, color));
				} else if let Some(color) = parse_osc_color(params[1]) {
					if foreground {
						self.default_fg = color;
						self.fg = color;
					} else {
						self.default_bg = color;
						self.bg = color;
					}
				}
			}
			_ => {}
		}
	}

	fn osc_palette(&mut self, params: &[&[u8]]) {
		let mut i = 0;
		while i + 1 < params.len() {
			let Ok(index) = std::str::from_utf8(params[i])
				.unwrap_or("")
				.parse::<usize>()
			else {
				i += 2;
				continue;
			};
			if index < self.color_overrides.len() {
				if params[i + 1] == b"?" {
					let color = self.indexed_color(index as u8);
					self.responses.push(
						format!(
							"\x1b]4;{index};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
							(color >> 16) & 0xff,
							(color >> 8) & 0xff,
							color & 0xff
						)
						.into_bytes(),
					);
				} else if let Some(color) = parse_osc_color(params[i + 1]) {
					self.color_overrides[index] = Some(color);
				}
			}
			i += 2;
		}
	}

	fn indexed_color(&self, index: u8) -> u32 {
		self.color_overrides[index as usize].unwrap_or_else(|| xterm_color(index, self.palette))
	}
}

fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
	const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;
	if input.len() > MAX_CLIPBOARD_BYTES.saturating_mul(4) / 3 + 4 || input.len() % 4 != 0 {
		return None;
	}
	let mut output = Vec::with_capacity(input.len() / 4 * 3);
	for (index, chunk) in input.as_chunks::<4>().0.iter().enumerate() {
		let last = index + 1 == input.len() / 4;
		let value = |byte| match byte {
			b'A'..=b'Z' => Some(byte - b'A'),
			b'a'..=b'z' => Some(byte - b'a' + 26),
			b'0'..=b'9' => Some(byte - b'0' + 52),
			b'+' => Some(62),
			b'/' => Some(63),
			_ => None,
		};
		let a = value(chunk[0])? as u32;
		let b = value(chunk[1])? as u32;
		let c = if chunk[2] == b'=' {
			if !last || chunk[3] != b'=' || b & 0x0f != 0 {
				return None;
			}
			None
		} else {
			Some(value(chunk[2])? as u32)
		};
		let d = if chunk[3] == b'=' {
			if !last || c.is_none() || c.is_some_and(|value| value & 0x03 != 0) {
				return None;
			}
			None
		} else {
			Some(value(chunk[3])? as u32)
		};
		let bits = (a << 18) | (b << 12) | (c.unwrap_or(0) << 6) | d.unwrap_or(0);
		output.push((bits >> 16) as u8);
		if c.is_some() {
			output.push((bits >> 8) as u8);
		}
		if d.is_some() {
			output.push(bits as u8);
		}
		if output.len() > MAX_CLIPBOARD_BYTES {
			return None;
		}
	}
	Some(output)
}

fn osc_color_reply(command: &[u8], color: u32) -> Vec<u8> {
	format!(
		"\x1b]{};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
		String::from_utf8_lossy(command),
		(color >> 16) & 0xff,
		(color >> 8) & 0xff,
		color & 0xff
	)
	.into_bytes()
}

fn parse_osc_color(value: &[u8]) -> Option<u32> {
	if let Ok(value) = std::str::from_utf8(value)
		&& let Some(hex) = value.strip_prefix('#')
		&& hex.len() == 6
	{
		return u32::from_str_radix(hex, 16).ok();
	}
	if let Ok(value) = std::str::from_utf8(value)
		&& let Some(rgb) = value.strip_prefix("rgb:")
	{
		let mut channels = rgb
			.split('/')
			.map(|part| u16::from_str_radix(part, 16).ok());
		let channel = |value: u16, digits: usize| -> u32 {
			let max = (1u32 << (digits.min(4) * 4)) - 1;
			((value as u32 * 255 + max / 2) / max).min(255)
		};
		let r = channels.next()??;
		let g = channels.next()??;
		let b = channels.next()??;
		return Some(
			(channel(r, rgb.split('/').next()?.len()) << 16)
				| (channel(g, rgb.split('/').nth(1)?.len()) << 8)
				| channel(b, rgb.split('/').nth(2)?.len()),
		);
	}
	None
}

impl Perform for Term {
	#[inline(always)]
	fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
		let target = self.pending();
		target.sixel_transfer = if !ignore && intermediates.is_empty() && action == 'q' {
			let mut values = params.iter().map(|p| p.first().copied().unwrap_or(0));
			Some(crate::sixel::Transfer::new(
				values.next().unwrap_or(0),
				values.next().unwrap_or(0),
			))
		} else {
			None
		};
	}
	#[inline(always)]
	fn put(&mut self, byte: u8) {
		if let Some(transfer) = &mut self.pending().sixel_transfer {
			transfer.push(byte);
		}
	}
	#[inline(always)]
	fn unhook(&mut self) {
		self.pending().finish_sixel();
	}

	#[inline(always)]
	fn print(&mut self, c: char) {
		self.pending().apply_print(c);
	}
	#[inline(always)]
	fn execute(&mut self, byte: u8) {
		self.pending().apply_execute(byte);
	}
	#[inline(always)]
	fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
		self
			.pending()
			.apply_esc_dispatch(intermediates, ignore, byte);
	}
	#[inline(always)]
	fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
		self.pending().apply_osc_dispatch(params, bell);
	}
	#[inline(always)]
	fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
		if ignore {
			return;
		}
		if intermediates == b"?$" && action == 'p' {
			for param in params.iter() {
				let mode = param[0];
				let status = if mode == 2026 {
					if self.synchronized.is_some() { 1 } else { 2 }
				} else {
					0
				};
				self
					.responses
					.push(format!("\x1b[?{mode};{status}$y").into_bytes());
			}
			return;
		}
		if intermediates == b"?"
			&& matches!(action, 'h' | 'l')
			&& params.iter().any(|p| p.first() == Some(&2026))
		{
			if action == 'h' {
				self.begin_sync();
			} else {
				self.finish_sync();
			}
		}
		self
			.pending()
			.apply_csi_dispatch(params, intermediates, false, action);
		if let Some(pending) = &mut self.synchronized {
			self.responses.append(&mut pending.responses);
		}
	}
}

fn rgb_color(r: u16, g: u16, b: u16) -> u32 {
	((r.min(255) as u32) << 16) | ((g.min(255) as u32) << 8) | b.min(255) as u32
}
