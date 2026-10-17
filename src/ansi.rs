//! Terminal grid, scrollback, cursor and screen state.
use crate::Theme;

use std::{
	collections::VecDeque,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(crate) struct Cell {
	pub(crate) attrs: u8,
	pub(crate) ch: char,
	pub(crate) combining: [char; 8],
	pub(crate) combining_len: u8,
	pub(crate) fg: u32,
	pub(crate) bg: u32,
	pub(crate) width: u8,
	pub(crate) hyperlink: u16,
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
	pub(crate) shell: crate::osc::ShellIntegration,
	pub(crate) history_base: usize,
	pub(crate) cols: usize,
	pub(crate) rows: usize,
	pub(crate) cells: Vec<Cell>,
	pub(crate) scrollback: VecDeque<Arc<Vec<Cell>>>,
	pub(crate) scrollback_limit: usize,
	pub(crate) scroll_offset: usize,
	pub(crate) primary_screen: Option<(Vec<Cell>, usize, usize, u32, u32)>,
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

	pub(crate) fn begin_sync(&mut self) {
		if self.synchronized.is_none() {
			let mut pending = self.clone();
			pending.responses.clear();
			self.synchronized = Some(Box::new(pending));
			self.sync_deadline = Some(Instant::now() + Duration::from_millis(250));
		}
	}

	pub(crate) fn finish_sync(&mut self) -> bool {
		if let Some(pending) = self.synchronized.take() {
			let responses = std::mem::take(&mut self.responses);
			*self = *pending;
			self.responses.extend(responses);
			self.sync_deadline = None;
			true
		} else {
			false
		}
	}

	pub(crate) fn finish_sync_if_expired(&mut self, now: Instant) -> bool {
		self.sync_deadline.is_some_and(|deadline| now >= deadline) && self.finish_sync()
	}

	#[inline(always)]
	pub(crate) fn pending(&mut self) -> &mut Self {
		match self.synchronized {
			Some(ref mut pending) => pending,
			None => self,
		}
	}

	#[inline(always)]
	pub(crate) fn row_cells(&self, row: usize) -> Option<&[Cell]> {
		if row < self.scrollback.len() {
			self.scrollback.get(row).map(|line| line.as_slice())
		} else {
			let row = row - self.scrollback.len();
			(row < self.rows).then(|| {
				let start = row * self.cols;
				&self.cells[start..start + self.cols]
			})
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
		recolor(&mut self.cells);
		for line in &mut self.scrollback {
			recolor(Arc::make_mut(line).as_mut_slice());
		}
		if let Some((cells, _, _, fg, bg)) = &mut self.primary_screen {
			recolor(cells);
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
			combining: ['\0'; 8],
			combining_len: 0,
			fg: self.fg,
			bg: self.bg,
			width: 1,
			hyperlink: 0,
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
			cells: vec![
				Cell {
					attrs: 0,
					ch: ' ',
					combining: ['\0'; 8],
					combining_len: 0,
					fg: theme.fg,
					bg: theme.bg,
					width: 1,
					hyperlink: 0,
				};
				cols * rows
			],
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

	pub(crate) fn resize(&mut self, cols: usize, rows: usize) {
		self.finish_sync();
		let cols = cols.max(1);
		let rows = rows.max(1);
		let blank = self.blank();
		let mut new = vec![blank; cols * rows];
		let (old_cols, old_rows) = (self.cols, self.rows);
		for y in 0..rows.min(old_rows) {
			for x in 0..cols.min(old_cols) {
				new[y * cols + x] = self.cells[y * old_cols + x];
			}
		}
		self.cells = new;
		self.tab_stops.resize_with(cols, || false);
		for (col, val) in self.tab_stops.iter_mut().enumerate().skip(old_cols) {
			*val = col > 0 && col % 8 == 0;
		}
		for line in &mut self.scrollback {
			let line = Arc::make_mut(line);
			line.resize(cols, blank);
			line.truncate(cols);
		}
		self.cols = cols;
		self.rows = rows;
		self.wrap_pending = false;
		self.scroll_top = 0;
		self.scroll_bottom = rows.saturating_sub(1);
		if let Some((primary, cx, cy, ..)) = &mut self.primary_screen {
			let mut resized = vec![blank; cols * rows];
			for y in 0..rows.min(old_rows) {
				for x in 0..cols.min(old_cols) {
					resized[y * cols + x] = primary[y * old_cols + x];
				}
			}
			*primary = resized;
			*cx = (*cx).min(cols - 1);
			*cy = (*cy).min(rows - 1);
		}
		self.cx = self.cx.min(cols - 1);
		self.cy = self.cy.min(rows - 1);
		if let Some(modes) = &mut self.primary_modes {
			modes.3 = false;
		}
		self.repair_wide_cells();
	}

	pub(crate) fn repair_wide_cells(&mut self) {
		for y in 0..self.rows {
			self.repair_wide_row(y);
		}
	}

	pub(crate) fn repair_wide_row(&mut self, y: usize) {
		if y >= self.rows {
			return;
		}
		let row_start = y * self.cols;
		let row = &mut self.cells[row_start..row_start + self.cols];
		for col in 0..row.len() {
			if (row[col].width == 0 && (col == 0 || row[col - 1].width != 2))
				|| (row[col].width == 2 && (col + 1 == row.len() || row[col + 1].width != 0))
			{
				row[col].ch = ' ';
				row[col].width = 1;
			}
		}
	}

	pub(crate) fn scroll_view(&mut self, lines: isize) {
		if self.primary_screen.is_some() {
			return;
		}
		self.scroll_offset = if lines > 0 {
			self
				.scroll_offset
				.saturating_add(lines as usize)
				.min(self.scrollback.len())
		} else {
			self.scroll_offset.saturating_sub(lines.unsigned_abs())
		};
	}

	pub(crate) fn alternate_screen(&mut self, enabled: bool) {
		if enabled && self.primary_screen.is_none() {
			self.primary_images = std::mem::take(&mut self.images);
			self.primary_modes = Some((
				self.scroll_top,
				self.scroll_bottom,
				self.origin_mode,
				self.wrap_pending,
			));
			self.scroll_top = 0;
			self.scroll_bottom = self.rows - 1;
			self.origin_mode = false;
			self.wrap_pending = false;
			let blank = self.blank();
			self.primary_screen = Some((
				std::mem::replace(&mut self.cells, vec![blank; self.cols * self.rows]),
				self.cx,
				self.cy,
				self.fg,
				self.bg,
			));
			self.cx = 0;
			self.cy = 0;
			self.scroll_offset = 0;
		} else if !enabled && let Some((cells, cx, cy, fg, bg)) = self.primary_screen.take() {
			self.images = std::mem::take(&mut self.primary_images);
			self.cells = cells;
			self.cx = cx;
			self.cy = cy;
			self.fg = fg;
			self.bg = bg;
			if let Some((top, bottom, origin, wrap)) = self.primary_modes.take() {
				self.scroll_top = top.min(self.rows - 1);
				self.scroll_bottom = bottom.min(self.rows - 1).max(self.scroll_top);
				self.origin_mode = origin;
				self.wrap_pending = wrap;
			}
			self.scroll_offset = 0;
			self.repair_wide_cells();
		}
	}

	#[inline(always)]
	pub(crate) fn linefeed(&mut self) {
		if self.cy == self.scroll_bottom {
			self.scroll_region_up(1);
		} else if self.cy + 1 < self.rows {
			self.cy += 1;
		}
	}

	pub(crate) fn scroll_region_up(&mut self, count: usize) {
		self.scroll_images(-(count.min(self.scroll_bottom - self.scroll_top + 1) as isize));
		let count = count.min(self.scroll_bottom - self.scroll_top + 1);
		let start = self.scroll_top * self.cols;
		let end = (self.scroll_bottom + 1) * self.cols;
		let history = if self.scroll_top == 0
			&& self.scroll_bottom + 1 == self.rows
			&& self.primary_screen.is_none()
		{
			(0..count)
				.map(|row| {
					let from = start + row * self.cols;
					Arc::new(self.cells[from..from + self.cols].to_vec())
				})
				.collect::<Vec<_>>()
		} else {
			Vec::new()
		};
		self
			.cells
			.copy_within(start + count * self.cols..end, start);
		let blank = self.blank();
		self.cells[end - count * self.cols..end].fill(blank);
		if self.scroll_offset > 0 {
			self.scroll_offset += history.len();
		}
		self.scrollback.extend(history);
		while self.scrollback.len() > self.scrollback_limit {
			self.scrollback.pop_front();
			self.history_base = self.history_base.saturating_add(1);
		}
		self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
	}

	pub(crate) fn scroll_region_down(&mut self, count: usize) {
		self.scroll_images(count.min(self.scroll_bottom - self.scroll_top + 1) as isize);
		let count = count.min(self.scroll_bottom - self.scroll_top + 1);
		let start = self.scroll_top * self.cols;
		let end = (self.scroll_bottom + 1) * self.cols;
		self
			.cells
			.copy_within(start..end - count * self.cols, start + count * self.cols);
		let blank = self.blank();
		self.cells[start..start + count * self.cols].fill(blank);
	}

	pub(crate) fn scroll_images(&mut self, amount: isize) {
		let top = self.scroll_top as isize;
		let bottom = self.scroll_bottom as isize;
		let cell_height = self.cell_height.max(1);
		self.images.retain_mut(|image| {
			let last_row = image.row + image.bitmap.height.div_ceil(cell_height) as isize - 1;
			if image.row > bottom || last_row < top {
				return true;
			}
			if image.row < top || last_row > bottom {
				return false;
			}
			image.row += amount;
			image.row >= top
				&& image.row + image.bitmap.height.div_ceil(cell_height) as isize - 1 <= bottom
		});
	}

	pub(crate) fn erase_images(&mut self, from: usize, to: usize) {
		if from >= to {
			return;
		}
		let cols = self.cols;
		let cw = self.cell_width.max(1);
		let ch = self.cell_height.max(1);
		self.images.retain(|image| {
			let first_row = image.row.max(0) as usize;
			let end_row = first_row + image.bitmap.height.div_ceil(ch);
			let end_col = (image.col + image.bitmap.width.div_ceil(cw)).min(cols);
			!(first_row..end_row).any(|row| from < row * cols + end_col && to > row * cols + image.col)
		});
	}

	pub(crate) fn finish_sixel(&mut self) {
		let Some(transfer) = self.sixel_transfer.take() else {
			return;
		};
		let Some(bitmap) = transfer.finish(self.bg, self.fg) else {
			return;
		};
		let rows = bitmap.height.div_ceil(self.cell_height.max(1));
		let (mut row, col) = if self.sixel_scrolling {
			(self.cy as isize, self.cx)
		} else {
			(0, 0)
		};
		let count = bitmap.pixels.len();
		while !self.images.is_empty()
			&& (self.images.len() >= 16
				|| self
					.images
					.iter()
					.map(|image| image.bitmap.pixels.len())
					.sum::<usize>()
					+ count
					> 4 * 1024 * 1024)
		{
			self.images.remove(0);
		}
		if self.sixel_scrolling {
			for _ in 0..rows {
				if self.cy == self.scroll_bottom {
					row -= 1;
				}
				self.linefeed();
			}
			self.cx = 0;
			self.wrap_pending = false;
		}
		self.images.push(crate::sixel::Placement {
			bitmap: Arc::new(bitmap),
			row,
			col,
		});
	}

	#[inline]
	pub(crate) fn clear_range(&mut self, from: usize, to: usize) {
		self.erase_images(from, to);
		let end = to.min(self.cells.len());
		if from < end {
			let blank = self.blank();
			self.cells[from..end].fill(blank);
		}
	}
}
