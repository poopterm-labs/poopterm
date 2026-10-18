//! The main window's persistent back buffer, and deciding how much of it a
//! new frame has to redraw.
use crate::{
	render::damage::{GridDamage, grid_damage},
	term::Frame,
	ui::selection::CellPoint,
};

/// Drawing inputs besides the grid that change how the grid looks. Any
/// change redraws everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DrawKey {
	pub session: u64,
	pub focused: bool,
	pub selection: Option<(CellPoint, CellPoint)>,
	pub search_match: Option<(CellPoint, CellPoint)>,
	/// Search box or command palette, which cover part of the grid.
	pub overlay: bool,
}

/// How to produce the next frame.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plan {
	Full,
	Partial(GridDamage),
}

pub(super) struct Canvas {
	pub pixels: Vec<u32>,
	width: usize,
	height: usize,
	/// The grid and inputs the pixels show; `None` after [`invalidate`](Self::invalidate).
	shown: Option<(Frame, DrawKey)>,
}

impl Canvas {
	pub(super) fn new() -> Self {
		Self {
			pixels: Vec::new(),
			width: 0,
			height: 0,
			shown: None,
		}
	}

	/// Forces the next frame to be drawn in full, e.g. after a theme, font
	/// or wallpaper change.
	pub(super) fn invalidate(&mut self) {
		self.shown = None;
	}

	/// Decides how to draw `frame` at `width`×`height`, resizing the buffer
	/// when needed. `allow_scroll` permits shifting pixels (not with a
	/// wallpaper, which stays put while text scrolls).
	pub(super) fn plan(
		&mut self,
		frame: &Frame,
		key: DrawKey,
		(width, height): (usize, usize),
		allow_scroll: bool,
	) -> Plan {
		if (self.width, self.height) != (width, height) {
			self.width = width;
			self.height = height;
			self.pixels.clear();
			self.pixels.resize(width * height, 0);
			self.shown = None;
		}
		let partial = match &self.shown {
			Some((shown, shown_key))
				if *shown_key == key
					&& !key.overlay
					&& key.selection.is_none()
					&& key.search_match.is_none() =>
			{
				grid_damage(shown, frame, allow_scroll)
			}
			_ => None,
		};
		partial.map_or(Plan::Full, Plan::Partial)
	}

	/// Records that the pixels now show `frame`; hands back the previous
	/// frame's buffers for reuse.
	pub(super) fn remember(&mut self, frame: &mut Frame, key: DrawKey) {
		match &mut self.shown {
			Some((shown, shown_key)) => {
				std::mem::swap(shown, frame);
				*shown_key = key;
			}
			None => self.shown = Some((std::mem::take(frame), key)),
		}
	}
}

/// Merges sorted, possibly adjacent pixel-row ranges.
pub(super) fn merge_bands(bands: &mut Vec<(usize, usize)>) {
	bands.sort_unstable();
	let mut merged: Vec<(usize, usize)> = Vec::with_capacity(bands.len());
	for &(start, end) in bands.iter() {
		match merged.last_mut() {
			Some(last) if start <= last.1 => last.1 = last.1.max(end),
			_ => merged.push((start, end)),
		}
	}
	*bands = merged;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		parser::Parser,
		term::Term,
		theme::{Theme, ThemeKind},
	};

	const KEY: DrawKey = DrawKey {
		session: 1,
		focused: true,
		selection: None,
		search_match: None,
		overlay: false,
	};

	fn frame(input: &str) -> Frame {
		let mut term = Term::new(4, 2, Theme::from_kind(ThemeKind::Poop), "/".into());
		Parser::new().advance(&mut term, input.as_bytes());
		let mut frame = Frame::default();
		term.snapshot(&mut frame);
		frame
	}

	#[test]
	fn the_first_frame_and_frames_after_invalidation_are_full() {
		let mut canvas = Canvas::new();
		let mut shown = frame("a");
		assert_eq!(canvas.plan(&shown, KEY, (10, 10), true), Plan::Full);
		assert_eq!(canvas.pixels.len(), 100);
		canvas.remember(&mut shown, KEY);
		assert!(matches!(
			canvas.plan(&frame("a"), KEY, (10, 10), true),
			Plan::Partial(damage) if damage.is_empty()
		));
		canvas.invalidate();
		assert_eq!(canvas.plan(&frame("a"), KEY, (10, 10), true), Plan::Full);
	}

	#[test]
	fn other_inputs_or_sizes_force_a_full_frame() {
		let mut canvas = Canvas::new();
		canvas.plan(&frame("a"), KEY, (10, 10), true);
		canvas.remember(&mut frame("a"), KEY);
		let unfocused = DrawKey {
			focused: false,
			..KEY
		};
		assert_eq!(
			canvas.plan(&frame("a"), unfocused, (10, 10), true),
			Plan::Full
		);
		let overlay = DrawKey {
			overlay: true,
			..KEY
		};
		canvas.remember(&mut frame("a"), overlay);
		assert_eq!(
			canvas.plan(&frame("a"), overlay, (10, 10), true),
			Plan::Full
		);
		canvas.remember(&mut frame("a"), KEY);
		assert_eq!(canvas.plan(&frame("a"), KEY, (11, 10), true), Plan::Full);
	}

	#[test]
	fn bands_are_merged() {
		let mut bands = vec![(10, 20), (0, 5), (20, 30), (25, 26), (40, 41)];
		merge_bands(&mut bands);
		assert_eq!(bands, [(0, 5), (10, 30), (40, 41)]);
	}
}
