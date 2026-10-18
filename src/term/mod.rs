//! Terminal grid, scrollback, cursor and screen state.
mod extras;
mod grid;
mod images;
mod screen;
mod sync;
#[cfg(test)]
mod tests;

use crate::theme::Theme;
pub(crate) use extras::NONE as NO_EXTRA;
pub(crate) use grid::Grid;

use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::Instant};

/// Bit flags stored in [`Cell::attrs`] and [`Term::attrs`].
pub(crate) mod attr {
	pub const DIM: u8 = 1;
	pub const ITALIC: u8 = 2;
	pub const UNDERLINE: u8 = 4;
	pub const CONCEAL: u8 = 8;
	pub const STRIKE: u8 = 16;
	pub const BLINK: u8 = 32;
	pub const OVERLINE: u8 = 64;
	pub const DOUBLE_UNDERLINE: u8 = 128;
}

/// One grid cell (16 bytes). `width` is 1 for normal, 2 for the left half
/// of a wide character and 0 for its right-half spacer. Combining marks and
/// hyperlinks live in [`Term::extras`], referenced by `extra`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
	pub(crate) ch: char,
	pub(crate) fg: u32,
	pub(crate) bg: u32,
	pub(crate) attrs: u8,
	pub(crate) width: u8,
	pub(crate) extra: u16,
}

impl Default for Cell {
	fn default() -> Self {
		Self {
			attrs: 0,
			ch: ' ',
			fg: 0,
			bg: 0,
			width: 1,
			extra: extras::NONE,
		}
	}
}

#[derive(Clone, Copy)]
pub(crate) struct SavedCursor {
	x: usize,
	y: usize,
	fg: u32,
	bg: u32,
	attrs: u8,
	bold: bool,
	reverse: bool,
	origin: bool,
	wrap: bool,
	charsets: [bool; 2],
	active_charset: usize,
}

#[derive(Clone)]
pub(crate) struct Term {
	pub(crate) images: Vec<crate::sixel::Placement>,
	pub(crate) primary_images: Vec<crate::sixel::Placement>,
	pub(crate) sixel_transfer: Option<crate::sixel::Transfer>,
	pub(crate) sixel_scrolling: bool,
	pub(crate) cell_width: usize,
	pub(crate) cell_height: usize,
	pub(crate) synchronized: Option<Box<Term>>,
	pub(crate) sync_deadline: Option<Instant>,
	pub(crate) shell: crate::shell_integration::ShellIntegration,
	pub(crate) history_base: usize,
	pub(crate) cols: usize,
	pub(crate) rows: usize,
	/// The visible screen.
	pub(crate) grid: Grid,
	pub(crate) scrollback: VecDeque<Arc<Vec<Cell>>>,
	pub(crate) scrollback_limit: usize,
	pub(crate) scroll_offset: usize,
	pub(crate) primary_screen: Option<(Grid, usize, usize, u32, u32)>,
	pub(crate) primary_modes: Option<(usize, usize, bool, bool)>,
	pub(crate) cursor_visible: bool,
	pub(crate) default_fg: u32,
	pub(crate) default_bg: u32,
	pub(crate) palette: [u32; 8],
	pub(crate) color_overrides: [Option<u32>; 256],
	pub(crate) cx: usize,
	pub(crate) cy: usize,
	pub(crate) fg: u32,
	pub(crate) bg: u32,
	pub(crate) scroll_top: usize,
	pub(crate) scroll_bottom: usize,
	pub(crate) saved_cursor: Option<SavedCursor>,
	pub(crate) bracketed_paste: bool,
	pub(crate) mouse_tracking: u16,
	pub(crate) mouse_sgr: bool,
	pub(crate) application_cursor: bool,
	pub(crate) origin_mode: bool,
	pub(crate) auto_wrap: bool,
	pub(crate) insert_mode: bool,
	pub(crate) kitty_keyboard: bool,
	pub(crate) active_hyperlink: u16,
	/// `extra` id applied to printed cells while a hyperlink is active.
	pub(crate) active_extra: u16,
	/// Combining marks and hyperlinks referenced by [`Cell::extra`]. Shared
	/// copy-on-write with synchronized-output snapshots.
	pub(crate) extras: Arc<extras::Extras>,
	pub(crate) hyperlinks: Vec<String>,
	pub(crate) wrap_pending: bool,
	pub(crate) charsets: [bool; 2],
	pub(crate) active_charset: usize,
	pub(crate) tab_stops: Vec<bool>,
	pub(crate) attrs: u8,
	pub(crate) bold: bool,
	pub(crate) reverse: bool,
	pub(crate) responses: Vec<Vec<u8>>,
	pub(crate) clipboard_write: Option<String>,
	pub(crate) title: String,
	pub(crate) title_generation: u64,
	pub(crate) cwd: PathBuf,
	pub(crate) cwd_generation: u64,
	pub(crate) bell: bool,
}

impl Term {
	pub(crate) fn save_cursor(&mut self) {
		self.saved_cursor = Some(SavedCursor {
			x: self.cx,
			y: self.cy,
			fg: self.fg,
			bg: self.bg,
			attrs: self.attrs,
			bold: self.bold,
			reverse: self.reverse,
			origin: self.origin_mode,
			wrap: self.wrap_pending,
			charsets: self.charsets,
			active_charset: self.active_charset,
		});
	}

	pub(crate) fn restore_cursor(&mut self) {
		if let Some(saved) = self.saved_cursor {
			self.cx = saved.x.min(self.cols - 1);
			self.cy = saved.y.min(self.rows - 1);
			self.fg = saved.fg;
			self.bg = saved.bg;
			self.attrs = saved.attrs;
			self.bold = saved.bold;
			self.reverse = saved.reverse;
			self.origin_mode = saved.origin;
			self.wrap_pending = saved.wrap && self.cx == self.cols - 1;
			self.charsets = saved.charsets;
			self.active_charset = saved.active_charset;
		} else {
			self.cx = 0;
			self.cy = 0;
			self.wrap_pending = false;
		}
	}

	#[inline(always)]
	pub(crate) fn row_cells(&self, row: usize) -> Option<&[Cell]> {
		if row < self.scrollback.len() {
			self.scrollback.get(row).map(|line| line.as_slice())
		} else {
			let row = row - self.scrollback.len();
			(row < self.rows).then(|| self.grid[row].as_slice())
		}
	}

	pub(crate) fn set_theme(&mut self, theme: Theme) {
		self.finish_sync();
		let (old_fg, old_bg) = (self.default_fg, self.default_bg);
		let recolor = |cells: &mut [Cell]| {
			for cell in cells {
				if cell.fg == old_fg {
					cell.fg = theme.fg;
				}
				if cell.bg == old_bg {
					cell.bg = theme.bg;
				}
			}
		};
		for row in self.grid.iter_mut() {
			recolor(row);
		}
		for line in &mut self.scrollback {
			recolor(Arc::make_mut(line).as_mut_slice());
		}
		if let Some((grid, _, _, fg, bg)) = &mut self.primary_screen {
			for row in grid.iter_mut() {
				recolor(row);
			}
			if *fg == old_fg {
				*fg = theme.fg;
			}
			if *bg == old_bg {
				*bg = theme.bg;
			}
		}
		self.default_fg = theme.fg;
		self.default_bg = theme.bg;
		self.palette = theme.palette;
		self.color_overrides.fill(None);
		if self.fg == old_fg {
			self.fg = theme.fg;
		}
		if self.bg == old_bg {
			self.bg = theme.bg;
		}
	}

	#[inline(always)]
	pub(crate) fn blank(&self) -> Cell {
		Cell {
			attrs: self.attrs,
			ch: ' ',
			fg: self.fg,
			bg: self.bg,
			width: 1,
			extra: extras::NONE,
		}
	}

	pub(crate) fn new(cols: usize, rows: usize, theme: Theme, cwd: PathBuf) -> Self {
		let cols = cols.max(1);
		let rows = rows.max(1);
		Self {
			images: Vec::new(),
			primary_images: Vec::new(),
			sixel_transfer: None,
			sixel_scrolling: true,
			cell_width: 10,
			cell_height: 20,
			synchronized: None,
			sync_deadline: None,
			shell: Default::default(),
			history_base: 0,
			cols,
			rows,
			grid: Grid::new(
				cols,
				rows,
				Cell {
					fg: theme.fg,
					bg: theme.bg,
					..Cell::default()
				},
			),
			scrollback: VecDeque::new(),
			scrollback_limit: 5000,
			scroll_offset: 0,
			primary_screen: None,
			primary_modes: None,
			cursor_visible: true,
			cx: 0,
			cy: 0,
			default_fg: theme.fg,
			default_bg: theme.bg,
			palette: theme.palette,
			color_overrides: [None; 256],
			fg: theme.fg,
			bg: theme.bg,
			scroll_top: 0,
			scroll_bottom: rows.saturating_sub(1),
			saved_cursor: None,
			bracketed_paste: false,
			mouse_tracking: 0,
			mouse_sgr: false,
			application_cursor: false,
			origin_mode: false,
			auto_wrap: true,
			insert_mode: false,
			kitty_keyboard: false,
			active_hyperlink: 0,
			active_extra: extras::NONE,
			extras: Arc::default(),
			hyperlinks: vec![String::new()],
			wrap_pending: false,
			charsets: [false; 2],
			active_charset: 0,
			tab_stops: (0..cols).map(|col| col > 0 && col % 8 == 0).collect(),
			attrs: 0,
			bold: false,
			reverse: false,
			responses: Vec::new(),
			clipboard_write: None,
			title: "Shell".into(),
			title_generation: 0,
			cwd,
			cwd_generation: 0,
			bell: false,
		}
	}

	#[inline(always)]
	pub(crate) fn idx(&self, x: usize, y: usize) -> usize {
		y * self.cols + x
	}

	/// Combining marks attached to `cell`.
	pub(crate) fn marks(&self, cell: &Cell) -> &[char] {
		self.extras.marks(cell.extra)
	}

	/// The OSC 8 target of `cell`, if any.
	pub(crate) fn hyperlink_uri(&self, cell: &Cell) -> Option<&str> {
		let link = self.extras.hyperlink(cell.extra);
		(link != 0).then(|| self.hyperlinks.get(link as usize).map(String::as_str))?
	}
}
