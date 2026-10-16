//! Escape-sequence handling using vte's incremental state machine.
use crate::{
    bright_color, dec_graphic, decode_uri_path, xterm_color, Cell, Term, Theme, ThemeKind,
};
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

impl Term {
    pub(crate) fn sgr(&mut self, params: &Params) {
        let mut values = Vec::new();
        for group in params.iter() {
            if group.len() > 1 && matches!(group[0], 38 | 48) {
                match group[1] {
                    2 if group.len() == 5 => values.extend_from_slice(group),
                    2 if group.len() >= 6 && group[2] == 0 => {
                        values.extend_from_slice(&group[..2]);
                        values.extend_from_slice(&group[3..6]);
                    }
                    5 if group.len() == 3 => values.extend_from_slice(group),
                    _ => {}
                }
            } else if group.first() == Some(&4) && group.len() > 1 {
                values.push(if group[1] == 0 { 24 } else { 4 });
            } else if group.len() == 1 {
                values.push(group[0]);
            }
        }
        if values.is_empty() {
            return;
        }
        let mut i = 0;
        while i < values.len() {
            let code = values[i];
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
                4 | 21 => self.attrs |= 4,
                8 => self.attrs |= 8,
                9 => self.attrs |= 16,
                22 => {
                    self.bold = false;
                    self.attrs &= !1;
                }
                23 => self.attrs &= !2,
                24 => self.attrs &= !4,
                28 => self.attrs &= !8,
                29 => self.attrs &= !16,
                7 => self.reverse = true,
                27 => self.reverse = false,
                30..=37 => self.fg = self.palette[(code - 30) as usize],
                39 => self.fg = self.default_fg,
                40..=47 => self.bg = self.palette[(code - 40) as usize],
                49 => self.bg = self.default_bg,
                90..=97 => self.fg = bright_color((code - 90) as usize),
                100..=107 => self.bg = bright_color((code - 100) as usize),
                38 | 48 if values.get(i + 1) == Some(&5) && values.len() > i + 2 => {
                    let color = xterm_color(values[i + 2] as u8, self.palette);
                    if code == 38 {
                        self.fg = color
                    } else {
                        self.bg = color
                    }
                    i += 2;
                }
                38 | 48 if values.get(i + 1) == Some(&2) && values.len() > i + 4 => {
                    let color = ((values[i + 2] as u32 & 255) << 16)
                        | ((values[i + 3] as u32 & 255) << 8)
                        | (values[i + 4] as u32 & 255);
                    if code == 38 {
                        self.fg = color
                    } else {
                        self.bg = color
                    }
                    i += 4;
                }
                _ => {}
            }
            i += 1;
        }
    }
}

impl Term {
    fn apply_print(&mut self, c: char) {
        let c = if self.charsets[self.active_charset] {
            dec_graphic(c)
        } else {
            c
        };
        let width = c.width().unwrap_or(0).min(self.cols);
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
        if self.bold {
            if let Some(index) = self.palette.iter().position(|&color| color == fg) {
                fg = bright_color(index);
            }
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
            fg,
            bg,
            width: width as u8,
        };
        if width == 2 && self.cx + 1 < self.cols {
            self.cells[i + 1] = Cell {
                attrs: self.attrs,
                ch: ' ',
                fg,
                bg,
                width: 0,
            };
        }
        self.wrap_pending = self.cx + width >= self.cols;
        self.cx = (self.cx + width).min(self.cols - 1);
    }

    fn apply_execute(&mut self, byte: u8) {
        if matches!(byte, 8..=13) {
            self.wrap_pending = false;
        }
        match byte {
            0x0e => self.active_charset = 1,
            0x0f => self.active_charset = 0,
            7 => self.bell = true,
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
            return;
        }
        if !matches!(action, 'm' | 'n' | 'c' | 'h' | 'l' | 's' | 'u') {
            self.wrap_pending = false;
        }
        // First value of the i-th param, with 0 treated as "default".
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

        if private {
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
                    self.cells
                        .copy_within(start..end - count * self.cols, start + count * self.cols);
                    self.clear_range(start, start + count * self.cols);
                } else {
                    self.cells
                        .copy_within(start + count * self.cols..end, start);
                    self.clear_range(end - count * self.cols, end);
                }
            }
            'm' => self.sgr(params),
            'n' => match raw0 {
                5 => self.responses.push(b"\x1b[0n".to_vec()),
                6 => self.responses.push(
                    format!("\x1b[{};{}R", self.cy + 1, self.cx.min(self.cols - 1) + 1)
                        .into_bytes(),
                ),
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
        if params.first() == Some(&b"133".as_slice()) {
            self.shell.record(
                params,
                self.history_base + self.scrollback.len() + self.cy,
                self.cx,
            );
        } else if params.len() >= 2 && matches!(params[0], b"0" | b"2") {
            let title = String::from_utf8_lossy(params[1]);
            self.title = title.chars().filter(|c| !c.is_control()).take(64).collect();
        } else if params.len() >= 2 && params[0] == b"7" {
            if let Ok(uri) = std::str::from_utf8(params[1]) {
                if let Some(path) = uri.strip_prefix("file://") {
                    if let Some(path) = path
                        .find('/')
                        .and_then(|index| decode_uri_path(&path[index..]))
                    {
                        self.cwd = path;
                    }
                }
            }
        }
    }
}

impl Perform for Term {
    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        let target = self.pending();
        target.sixel_transfer = if !ignore && intermediates.is_empty() && action == 'q' {
            let mut values = params.iter().map(|p| p[0]);
            Some(crate::sixel::Transfer::new(
                values.next().unwrap_or(0),
                values.next().unwrap_or(0),
            ))
        } else {
            None
        };
    }
    fn put(&mut self, byte: u8) {
        if let Some(transfer) = &mut self.pending().sixel_transfer {
            transfer.push(byte);
        }
    }
    fn unhook(&mut self) {
        self.pending().finish_sixel();
    }

    fn print(&mut self, c: char) {
        self.pending().apply_print(c);
    }
    fn execute(&mut self, byte: u8) {
        self.pending().apply_execute(byte);
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.pending()
            .apply_esc_dispatch(intermediates, ignore, byte);
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
        self.pending().apply_osc_dispatch(params, bell);
    }
    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }
        if intermediates == b"?$" && action == 'p' {
            for param in params.iter() {
                let mode = param[0];
                let status = if mode == 2026 {
                    if self.synchronized.is_some() {
                        1
                    } else {
                        2
                    }
                } else {
                    0
                };
                self.responses
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
        self.pending()
            .apply_csi_dispatch(params, intermediates, false, action);
        if let Some(pending) = &mut self.synchronized {
            self.responses.append(&mut pending.responses);
        }
    }
}
