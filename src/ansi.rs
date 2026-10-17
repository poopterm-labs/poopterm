//! SIMD-optimized ANSI parser and terminal grid, scrollback, cursor and screen state.
use crate::Theme;
use unicode_width::UnicodeWidthChar;

use std::{
    collections::VecDeque,
    path::PathBuf,
    simd::{prelude::*, u8x32, Simd},
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(crate) struct Cell {
    pub(crate) attrs: u8,
    pub(crate) ch: char,
    pub(crate) fg: u32,
    pub(crate) bg: u32,
    pub(crate) width: u8,
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
    pub(crate) wrap_pending: bool,
    pub(crate) charsets: [bool; 2],
    pub(crate) active_charset: usize,
    pub(crate) tab_stops: Vec<bool>,
    pub(crate) attrs: u8,
    pub(crate) bold: bool,
    pub(crate) reverse: bool,
    pub(crate) responses: Vec<Vec<u8>>,
    pub(crate) title: String,
    pub(crate) cwd: PathBuf,
    pub(crate) bell: bool,

    // Parser state machine internal buffers
    parser_state: ParserState,
    csi_params: Vec<i32>,
    csi_intermediates: Vec<u8>,
    osc_payload: Vec<u8>,
    dcs_payload: Vec<u8>,

    // UTF-8 accumulation buffer
    utf8_buf: [u8; 4],
    utf8_len: usize,
    utf8_expected: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParserState {
    Ground,
    Escape,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    OscString,
    DcsString,
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
                    fg: theme.fg,
                    bg: theme.bg,
                    width: 1,
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
            wrap_pending: false,
            charsets: [false; 2],
            active_charset: 0,
            tab_stops: (0..cols).map(|col| col > 0 && col % 8 == 0).collect(),
            attrs: 0,
            bold: false,
            reverse: false,
            responses: Vec::new(),
            title: "Shell".into(),
            cwd,
            bell: false,
            parser_state: ParserState::Ground,
            csi_params: Vec::new(),
            csi_intermediates: Vec::new(),
            osc_payload: Vec::new(),
            dcs_payload: Vec::new(),
            utf8_buf: [0; 4],
            utf8_len: 0,
            utf8_expected: 0,
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
        for row in self.cells.chunks_exact_mut(self.cols) {
            for col in 0..row.len() {
                if (row[col].width == 0 && (col == 0 || row[col - 1].width != 2))
                    || (row[col].width == 2 && (col + 1 == row.len() || row[col + 1].width != 0))
                {
                    row[col].ch = ' ';
                    row[col].width = 1;
                }
            }
        }
    }

    pub(crate) fn scroll_view(&mut self, lines: isize) {
        if self.primary_screen.is_some() {
            return;
        }
        self.scroll_offset = if lines > 0 {
            self.scroll_offset
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
        self.cells
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
        self.cells
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

    // =========================================================================
    // SIMD-Accelerated ANSI Stream Parsing & Execution Engine
    // =========================================================================

    /// Parse a stream of PTY bytes with SIMD-accelerated ASCII fast paths.
    pub(crate) fn parse_bytes(&mut self, input: &[u8]) {
        let mut i = 0;
        let len = input.len();

        while i < len {
            if self.parser_state == ParserState::Ground && self.utf8_expected == 0 && i + 32 <= len {
                let chunk = u8x32::from_slice(&input[i..i + 32]);
                let lt_space = chunk.simd_lt(Simd::splat(0x20));
                let is_esc = chunk.simd_eq(Simd::splat(0x1B));
                let gte_hi = chunk.simd_ge(Simd::splat(0x80));

                let control_mask = lt_space.to_bitmask() | is_esc.to_bitmask() | gte_hi.to_bitmask();

                if control_mask == 0 {
                    for &b in &input[i..i + 32] {
                        self.print_char(b as char);
                    }
                    i += 32;
                    continue;
                } else {
                    let safe_len = control_mask.trailing_zeros() as usize;
                    for &b in &input[i..i + safe_len] {
                        self.print_char(b as char);
                    }
                    i += safe_len;
                }
            }

            let byte = input[i];
            i += 1;
            self.feed_byte(byte);
        }
    }

    #[inline(always)]
    fn print_char(&mut self, c: char) {
        let width = c.width().unwrap_or(1).min(self.cols);
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
        let idx = self.idx(self.cx, self.cy);
        
        let mut cell_fg = self.fg;
        let mut cell_bg = self.bg;
        if self.bold {
            if let Some(index) = self.palette.iter().position(|&color| color == cell_fg) {
                cell_fg = bright_color(index);
            }
        }
        if self.reverse {
            std::mem::swap(&mut cell_fg, &mut cell_bg);
        }
        if self.attrs & 1 != 0 {
            cell_fg = blend_color(cell_bg, cell_fg, 160);
        }
        if self.attrs & 8 != 0 {
            cell_fg = cell_bg;
        }

        self.erase_images(idx, idx + width);
        if idx < self.cells.len() {
            self.cells[idx] = Cell {
                attrs: self.attrs,
                ch: c,
                fg: cell_fg,
                bg: cell_bg,
                width: width as u8,
            };
        }
        if width == 2 && self.cx + 1 < self.cols {
            self.cells[idx + 1] = Cell {
                attrs: self.attrs,
                ch: ' ',
                fg: cell_fg,
                bg: cell_bg,
                width: 0,
            };
        }
        self.wrap_pending = self.cx + width >= self.cols;
        self.cx = (self.cx + width).min(self.cols - 1);
    }

    fn feed_byte(&mut self, byte: u8) {
        match self.parser_state {
            ParserState::Ground => {
                if self.utf8_expected > 0 {
                    self.utf8_buf[self.utf8_len] = byte;
                    self.utf8_len += 1;
                    if self.utf8_len == self.utf8_expected {
                        if let Ok(s) = std::str::from_utf8(&self.utf8_buf[..self.utf8_len]) {
                            if let Some(ch) = s.chars().next() {
                                self.print_char(ch);
                            }
                        }
                        self.utf8_expected = 0;
                        self.utf8_len = 0;
                    }
                    return;
                }

                match byte {
                    0x1B => self.parser_state = ParserState::Escape,
                    b'\n' => {
                        self.linefeed();
                        self.wrap_pending = false;
                    }
                    b'\r' => {
                        self.cx = 0;
                        self.wrap_pending = false;
                    }
                    b'\t' => {
                        if let Some(next_stop) = self.tab_stops.iter().enumerate().skip(self.cx + 1).find(|&(_, &is_stop)| is_stop).map(|(idx, _)| idx) {
                            self.cx = next_stop.min(self.cols - 1);
                        } else {
                            self.cx = self.cols - 1;
                        }
                    }
                    0x08 => {
                        self.cx = self.cx.saturating_sub(1);
                        self.wrap_pending = false;
                    }
                    b if b >= 0x20 && b < 0x7F => self.print_char(b as char),
                    b if b >= 0x80 => {
                        if b & 0xE0 == 0xC0 {
                            self.utf8_expected = 2;
                        } else if b & 0xF0 == 0xE0 {
                            self.utf8_expected = 3;
                        } else if b & 0xF8 == 0xF0 {
                            self.utf8_expected = 4;
                        } else {
                            return;
                        }
                        self.utf8_buf[0] = b;
                        self.utf8_len = 1;
                    }
                    _ => {}
                }
            }
            ParserState::Escape => match byte {
                b'[' => {
                    self.parser_state = ParserState::CsiEntry;
                    self.csi_params.clear();
                    self.csi_intermediates.clear();
                    self.csi_params.push(0);
                }
                b']' => {
                    self.parser_state = ParserState::OscString;
                    self.osc_payload.clear();
                }
                b'P' => {
                    self.parser_state = ParserState::DcsString;
                    self.dcs_payload.clear();
                }
                _ => {
                    self.parser_state = ParserState::Ground;
                }
            },
            ParserState::CsiEntry => match byte {
                b'0'..=b'9' => {
                    self.parser_state = ParserState::CsiParam;
                    let val = (byte - b'0') as i32;
                    if let Some(last) = self.csi_params.last_mut() {
                        *last = val;
                    }
                }
                b';' => {
                    self.parser_state = ParserState::CsiParam;
                    self.csi_params.push(0);
                }
                b'?' | b'>' | b'!' => {
                    self.csi_intermediates.push(byte);
                }
                0x40..=0x7E => {
                    self.execute_csi(byte);
                    self.parser_state = ParserState::Ground;
                }
                _ => {
                    self.parser_state = ParserState::Ground;
                }
            },
            ParserState::CsiParam => match byte {
                b'0'..=b'9' => {
                    let val = (byte - b'0') as i32;
                    if let Some(last) = self.csi_params.last_mut() {
                        *last = last.saturating_mul(10).saturating_add(val);
                    }
                }
                b';' => {
                    self.csi_params.push(0);
                }
                0x40..=0x7E => {
                    self.execute_csi(byte);
                    self.parser_state = ParserState::Ground;
                }
                _ => {
                    self.parser_state = ParserState::Ground;
                }
            },
            ParserState::OscString => {
                if byte == 0x07 {
                    self.execute_osc();
                    self.parser_state = ParserState::Ground;
                } else if byte == 0x1B {
                    self.execute_osc();
                    self.parser_state = ParserState::Escape;
                } else {
                    self.osc_payload.push(byte);
                }
            }
            ParserState::DcsString => {
                if byte == 0x1B {
                    self.execute_dcs();
                    self.parser_state = ParserState::Escape;
                } else {
                    self.dcs_payload.push(byte);
                }
            }
            _ => {
                self.parser_state = ParserState::Ground;
            }
        }
    }

    fn execute_osc(&mut self) {
        let payload = String::from_utf8_lossy(&self.osc_payload);
        let mut parts = payload.splitn(2, ';');
        let code = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");

        match code {
            "0" | "2" => {
                self.title = value.to_string();
            }
            "4" => {
                let mut sub = value.split(';');
                if let (Some(idx_str), Some(col_spec)) = (sub.next(), sub.next()) {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if col_spec == "?" {
                            let color = self.palette.get(idx).copied().unwrap_or(self.default_fg);
                            let r = (color >> 16) & 0xff;
                            let g = (color >> 8) & 0xff;
                            let b = color & 0xff;
                            let resp = format!("\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x07", idx, r, g, b);
                            self.responses.push(resp.into_bytes());
                        } else if let Some(rgb) = parse_color_spec(col_spec) {
                            if idx < self.palette.len() {
                                self.palette[idx] = rgb;
                            }
                        }
                    }
                }
            }
            "7" => {
                if let Some(path) = crate::osc::decode_uri_path(value) {
                    self.cwd = path;
                }
            }
            _ => {}
        }
    }

    fn execute_dcs(&mut self) {
        if let Some(q_pos) = self.dcs_payload.iter().position(|&b| b == b'q') {
            let params = &self.dcs_payload[..q_pos];
            let data = &self.dcs_payload[q_pos + 1..];
            let mut macro_param = 0u16;
            let mut bg_param = 0u16;
            let mut p_iter = params.split(|&b| b == b';');
            if let Some(p1) = p_iter.next() {
                macro_param = std::str::from_utf8(p1).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            }
            if let Some(p2) = p_iter.next() {
                bg_param = std::str::from_utf8(p2).ok().and_then(|s| s.parse().ok()).unwrap_or(0);
            }

            let mut transfer = crate::sixel::Transfer::new(macro_param, bg_param);
            for &b in data {
                transfer.push(b);
            }
            self.sixel_transfer = Some(transfer);
            self.finish_sixel();
        }
    }

    fn execute_csi(&mut self, action: u8) {
        let p = |idx: usize, default: i32| -> i32 {
            self.csi_params.get(idx).copied().unwrap_or(default).max(0)
        };

        let is_private = self.csi_intermediates.contains(&b'?');

        match action {
            b'A' => {
                let count = p(0, 1) as usize;
                self.cy = self.cy.saturating_sub(count);
                self.wrap_pending = false;
            }
            b'B' => {
                let count = p(0, 1) as usize;
                self.cy = (self.cy + count).min(self.rows - 1);
                self.wrap_pending = false;
            }
            b'C' => {
                let count = p(0, 1) as usize;
                self.cx = (self.cx + count).min(self.cols - 1);
                self.wrap_pending = false;
            }
            b'D' => {
                let count = p(0, 1) as usize;
                self.cx = self.cx.saturating_sub(count);
                self.wrap_pending = false;
            }
            b'H' | b'f' => {
                let row = p(0, 1) as usize;
                let col = p(1, 1) as usize;
                self.cy = (row.saturating_sub(1)).min(self.rows - 1);
                self.cx = (col.saturating_sub(1)).min(self.cols - 1);
                self.wrap_pending = false;
            }
            b'J' => {
                let mode = p(0, 0);
                let idx = self.idx(self.cx, self.cy);
                match mode {
                    0 => self.clear_range(idx, self.cells.len()),
                    1 => self.clear_range(0, idx + 1),
                    2 => self.clear_range(0, self.cells.len()),
                    _ => {}
                }
            }
            b'K' => {
                let mode = p(0, 0);
                let start = self.idx(0, self.cy);
                let end = self.idx(self.cols, self.cy);
                let cur = self.idx(self.cx, self.cy);
                match mode {
                    0 => self.clear_range(cur, end),
                    1 => self.clear_range(start, cur + 1),
                    2 => self.clear_range(start, end),
                    _ => {}
                }
            }
            b'm' => {
                if self.csi_params.is_empty() {
                    self.attrs = 0;
                    self.bold = false;
                    self.reverse = false;
                    self.fg = self.default_fg;
                    self.bg = self.default_bg;
                } else {
                    let mut idx = 0;
                    while idx < self.csi_params.len() {
                        let param = self.csi_params[idx];
                        match param {
                            0 => {
                                self.attrs = 0;
                                self.bold = false;
                                self.reverse = false;
                                self.fg = self.default_fg;
                                self.bg = self.default_bg;
                            }
                            1 => self.bold = true,
                            2 => self.attrs |= 1,  // Dim
                            3 => self.attrs |= 2,  // Italic
                            4 => self.attrs |= 4,  // Underline
                            7 => self.reverse = true,
                            9 => self.attrs |= 16, // Strikethrough
                            22 => {
                                self.bold = false;
                                self.attrs &= !1;
                            }
                            23 => self.attrs &= !2,
                            24 => self.attrs &= !4,
                            27 => self.reverse = false,
                            29 => self.attrs &= !16,
                            30..=37 => {
                                let color_idx = (param - 30) as usize;
                                self.fg = self.palette.get(color_idx).copied().unwrap_or(self.default_fg);
                            }
                            38 => {
                                if let Some(&mode_type) = self.csi_params.get(idx + 1) {
                                    if mode_type == 5 {
                                        if let Some(&col) = self.csi_params.get(idx + 2) {
                                            self.fg = xterm_color(col as u8, self.palette);
                                            idx += 2;
                                        }
                                    } else if mode_type == 2 {
                                        if let (Some(&r), Some(&g), Some(&b)) = (
                                            self.csi_params.get(idx + 2),
                                            self.csi_params.get(idx + 3),
                                            self.csi_params.get(idx + 4),
                                        ) {
                                            self.fg = ((r.clamp(0, 255) as u32) << 16)
                                                | ((g.clamp(0, 255) as u32) << 8)
                                                | (b.clamp(0, 255) as u32);
                                            idx += 4;
                                        }
                                    }
                                }
                            }
                            39 => self.fg = self.default_fg,
                            40..=47 => {
                                let color_idx = (param - 40) as usize;
                                self.bg = self.palette.get(color_idx).copied().unwrap_or(self.default_bg);
                            }
                            48 => {
                                if let Some(&mode_type) = self.csi_params.get(idx + 1) {
                                    if mode_type == 5 {
                                        if let Some(&col) = self.csi_params.get(idx + 2) {
                                            self.bg = xterm_color(col as u8, self.palette);
                                            idx += 2;
                                        }
                                    } else if mode_type == 2 {
                                        if let (Some(&r), Some(&g), Some(&b)) = (
                                            self.csi_params.get(idx + 2),
                                            self.csi_params.get(idx + 3),
                                            self.csi_params.get(idx + 4),
                                        ) {
                                            self.bg = ((r.clamp(0, 255) as u32) << 16)
                                                | ((g.clamp(0, 255) as u32) << 8)
                                                | (b.clamp(0, 255) as u32);
                                            idx += 4;
                                        }
                                    }
                                }
                            }
                            49 => self.bg = self.default_bg,
                            90..=97 => {
                                let color_idx = (param - 90) as usize;
                                self.fg = bright_color(color_idx);
                            }
                            100..=107 => {
                                let color_idx = (param - 100) as usize;
                                self.bg = bright_color(color_idx);
                            }
                            _ => {}
                        }
                        idx += 1;
                    }
                }
            }
            b'h' | b'l' => {
                let enable = action == b'h';
                if is_private {
                    let params = self.csi_params.clone();
                    for param in params {
                        match param {
                            1 => self.application_cursor = enable,
                            7 => self.auto_wrap = enable,
                            25 => self.cursor_visible = enable,
                            1049 => self.alternate_screen(enable),
                            2004 => self.bracketed_paste = enable,
                            _ => {}
                        }
                    }
                }
            }
            b'n' => {
                let mode = p(0, 0);
                if mode == 6 {
                    let resp = format!("\x1b[{};{}R", self.cy + 1, self.cx + 1);
                    self.responses.push(resp.into_bytes());
                } else if mode == 5 {
                    self.responses.push(b"\x1b[0n".to_vec());
                }
            }
            b'c' => {
                let is_secondary = self.csi_intermediates.contains(&b'>');
                if is_secondary {
                    self.responses.push(b"\x1b[>0;95;0c".to_vec());
                } else {
                    self.responses.push(b"\x1b[?6c".to_vec());
                }
            }
            _ => {}
        }
    }
}

fn parse_color_spec(spec: &str) -> Option<u32> {
    if let Some(hex) = spec.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u32::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u32::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u32::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r << 16) | (g << 8) | b);
        }
    }
    None
}

fn bright_color(index: usize) -> u32 {
    [
        0x808080, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
    ][index]
}

fn xterm_color(index: u8, palette: [u32; 8]) -> u32 {
    if index < 8 {
        return palette[index as usize];
    }
    if index < 16 {
        return bright_color((index - 8) as usize);
    }
    if index < 232 {
        let n = index - 16;
        let component = |v: u8| if v == 0 { 0 } else { 55 + 40 * v as u32 };
        return (component(n / 36) << 16) | (component((n / 6) % 6) << 8) | component(n % 6);
    }
    let gray = 8 + 10 * (index as u32 - 232);
    (gray << 16) | (gray << 8) | gray
}

fn blend_color(bg: u32, fg: u32, a: u32) -> u32 {
    let fg_r = (fg >> 16) & 0xff;
    let bg_r = (bg >> 16) & 0xff;
    let r = (fg_r * a + bg_r * (255 - a)) / 255;

    let fg_g = (fg >> 8) & 0xff;
    let bg_g = (bg >> 8) & 0xff;
    let g = (fg_g * a + bg_g * (255 - a)) / 255;

    let fg_b = fg & 0xff;
    let bg_b = bg & 0xff;
    let b = (fg_b * a + bg_b * (255 - a)) / 255;

    (r << 16) | (g << 8) | b
}
