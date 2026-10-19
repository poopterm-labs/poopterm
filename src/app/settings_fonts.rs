//! The font list on the Settings window's Fonts page: entries, selection
//! and scrolling, independent of drawing.

/// Entry 0 is the bundled font (an empty `font_family`); the rest are the
/// installed monospace families.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FontPicker {
	pub(crate) families: Vec<String>,
	/// The `font_family` value being edited; empty for the bundled font.
	pub(crate) chosen: String,
	/// First entry shown.
	pub(crate) scroll: usize,
}

pub(super) const BUNDLED_LABEL: &str = "Bundled (DejaVu Sans Mono)";

impl FontPicker {
	pub(crate) fn new(families: Vec<String>, chosen: &str) -> Self {
		Self {
			families,
			chosen: chosen.trim().to_owned(),
			scroll: 0,
		}
	}

	pub(crate) fn len(&self) -> usize {
		self.families.len() + 1
	}

	pub(crate) fn label(&self, entry: usize) -> &str {
		match entry {
			0 => BUNDLED_LABEL,
			_ => self.families.get(entry - 1).map_or("", String::as_str),
		}
	}

	/// The chosen entry; `None` for a configured family that is not installed.
	pub(crate) fn selected(&self) -> Option<usize> {
		if self.chosen.is_empty() {
			return Some(0);
		}
		self
			.families
			.iter()
			.position(|family| family.eq_ignore_ascii_case(&self.chosen))
			.map(|index| index + 1)
	}

	/// Chooses `entry` if it exists.
	pub(crate) fn select(&mut self, entry: usize) {
		if entry < self.len() {
			self.chosen = match entry {
				0 => String::new(),
				_ => self.families[entry - 1].clone(),
			};
		}
	}

	/// Moves the choice by `step` entries, clamped, scrolling it into view
	/// of a list showing `visible` entries.
	pub(crate) fn step(&mut self, step: isize, visible: usize) {
		let from = self.selected().unwrap_or(0);
		let to = from.saturating_add_signed(step).min(self.len() - 1);
		self.select(to);
		if to < self.scroll {
			self.scroll = to;
		} else if visible > 0 && to >= self.scroll + visible {
			self.scroll = to + 1 - visible;
		}
	}

	/// Scrolls by `lines` entries, keeping a full list in view.
	pub(crate) fn scroll_by(&mut self, lines: isize, visible: usize) {
		let max = self.len().saturating_sub(visible);
		self.scroll = self.scroll.saturating_add_signed(lines).min(max);
	}

	/// The entry at visible row `row`, if there is one.
	pub(crate) fn entry_at(&self, row: usize) -> Option<usize> {
		let entry = self.scroll + row;
		(entry < self.len()).then_some(entry)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn picker(chosen: &str) -> FontPicker {
		let families = ["Fira Code", "Iosevka", "JetBrains Mono", "Roboto Mono"];
		FontPicker::new(families.map(str::to_owned).to_vec(), chosen)
	}

	#[test]
	fn the_bundled_font_comes_first_and_matches_an_empty_family() {
		let fonts = picker("");
		assert_eq!(fonts.len(), 5);
		assert_eq!(fonts.label(0), BUNDLED_LABEL);
		assert_eq!(fonts.label(2), "Iosevka");
		assert_eq!(fonts.selected(), Some(0));
		assert_eq!(picker(" iosevka ").selected(), Some(2));
		assert_eq!(picker("Not Installed").selected(), None);
	}

	#[test]
	fn selecting_sets_the_family_to_save() {
		let mut fonts = picker("Iosevka");
		fonts.select(4);
		assert_eq!(fonts.chosen, "Roboto Mono");
		fonts.select(0);
		assert_eq!(fonts.chosen, "");
		fonts.select(99);
		assert_eq!(fonts.chosen, "");
	}

	#[test]
	fn stepping_clamps_and_keeps_the_choice_visible() {
		let mut fonts = picker("");
		fonts.step(-1, 2);
		assert_eq!((fonts.selected(), fonts.scroll), (Some(0), 0));
		fonts.step(3, 2);
		assert_eq!((fonts.selected(), fonts.scroll), (Some(3), 2));
		fonts.step(10, 2);
		assert_eq!((fonts.selected(), fonts.scroll), (Some(4), 3));
		fonts.step(-4, 2);
		assert_eq!((fonts.selected(), fonts.scroll), (Some(0), 0));
		// An uninstalled family steps from the top.
		let mut missing = picker("Gone");
		missing.step(1, 2);
		assert_eq!(missing.selected(), Some(1));
	}

	#[test]
	fn scrolling_stays_within_the_list() {
		let mut fonts = picker("");
		fonts.scroll_by(-3, 2);
		assert_eq!(fonts.scroll, 0);
		fonts.scroll_by(10, 2);
		assert_eq!(fonts.scroll, 3);
		assert_eq!(fonts.entry_at(1), Some(4));
		assert_eq!(fonts.entry_at(2), None);
		fonts.scroll_by(10, 10);
		assert_eq!(fonts.scroll, 0, "a list that fits does not scroll");
	}
}
