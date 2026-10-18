//! Select Graphic Rendition (`CSI … m`): colors and text attributes.
use crate::{term::Term, theme::xterm_color};
use vte::Params;

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
	pub(crate) fn indexed_color(&self, index: u8) -> u32 {
		self.color_overrides[index as usize].unwrap_or_else(|| xterm_color(index, self.palette))
	}
}

fn rgb_color(r: u16, g: u16, b: u16) -> u32 {
	((r.min(255) as u32) << 16) | ((g.min(255) as u32) << 8) | b.min(255) as u32
}
