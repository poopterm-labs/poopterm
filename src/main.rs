use std::{
    collections::{HashMap, VecDeque},
    env,
    error::Error,
    fs,
    io::{Read, Write},
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    thread,
};

use fontdue::{Font, FontSettings, Metrics};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use vte::{Params, Parser, Perform};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;

// ---- Built-in themes --------------------------------------------------------

const DEFAULT_FG: u32 = 0xE8D5B0; // cream
const DEFAULT_BG: u32 = 0x24170B; // dark chocolate
const BORDER_COL: u32 = 0x7A4E1F; // caramel rim
const PALETTE: [u32; 8] = [
    0x1E1409, // black
    0xD9534F, // red
    0x8BC34A, // green
    0xE0B040, // yellow
    0x5C9BD6, // blue
    0xB06AC8, // magenta
    0x4FB6B0, // cyan
    0xE8D5B0, // white
];

#[derive(Clone, Copy)]
struct Theme {
    kind: ThemeKind,
    fg: u32,
    bg: u32,
    border: u32,
    palette: [u32; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThemeKind {
    MacOs,
    Catppuccin,
    RosePine,
    Poop,
}

impl ThemeKind {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "macos" | "mac" => Some(Self::MacOs),
            "catppuccin" | "catppuccin-mocha" => Some(Self::Catppuccin),
            "rosepine" | "rose-pine" => Some(Self::RosePine),
            "poop" => Some(Self::Poop),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::MacOs => "macos",
            Self::Catppuccin => "catppuccin",
            Self::RosePine => "rosepine",
            Self::Poop => "poop",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::MacOs => Self::Catppuccin,
            Self::Catppuccin => Self::RosePine,
            Self::RosePine => Self::Poop,
            Self::Poop => Self::MacOs,
        }
    }
}

impl Theme {
    fn from_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::MacOs => Self {
                kind,
                fg: 0xE6E6E6,
                bg: 0x1E1E1E,
                border: 0x3A3A3C,
                palette: [
                    0x000000, 0xC23621, 0x25BC26, 0xADAD27, 0x492EE1, 0xD338D3, 0x33BBC8, 0xBBBBBB,
                ],
            },
            ThemeKind::Catppuccin => Self {
                kind,
                fg: 0xCDD6F4,
                bg: 0x1E1E2E,
                border: 0x45475A,
                palette: [
                    0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
                ],
            },
            ThemeKind::RosePine => Self {
                kind,
                fg: 0xE0DEF4,
                bg: 0x191724,
                border: 0x26233A,
                palette: [
                    0x26233A, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0x9CCFD8, 0xE0DEF4,
                ],
            },
            ThemeKind::Poop => Self {
                kind,
                fg: DEFAULT_FG,
                bg: DEFAULT_BG,
                border: BORDER_COL,
                palette: PALETTE,
            },
        }
    }

    fn load() -> Self {
        let kind = env::var("POOPTERM_THEME")
            .ok()
            .and_then(|s| ThemeKind::parse(&s))
            .or_else(|| read_config_theme())
            .unwrap_or(ThemeKind::Poop);
        Self::from_kind(kind)
    }
}

fn config_path() -> PathBuf {
    if let Some(path) = env::var_os("POOPTERM_CONFIG") {
        return PathBuf::from(path);
    }
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("poopterm/config.toml");
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/poopterm/config.toml")
    } else {
        home.join(".config/poopterm/config.toml")
    }
}

fn read_config_theme() -> Option<ThemeKind> {
    let text = fs::read_to_string(config_path()).ok()?;
    text.lines().find_map(|line| {
        let line = line.split('#').next()?.trim();
        let (key, value) = line.split_once('=')?;
        (key.trim() == "theme")
            .then(|| ThemeKind::parse(value))
            .flatten()
    })
}

fn save_config_theme(kind: ThemeKind) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format!("# Poopterm configuration\ntheme = \"{}\"\n", kind.name()),
    )
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

// ---- Terminal state --------------------------------------------------------

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    fg: u32,
    bg: u32,
}

struct Term {
    cols: usize,
    rows: usize,
    cells: Vec<Cell>,
    scrollback: VecDeque<Vec<Cell>>,
    scroll_offset: usize,
    primary_screen: Option<(Vec<Cell>, usize, usize, u32, u32)>,
    cursor_visible: bool,
    default_fg: u32,
    default_bg: u32,
    palette: [u32; 8],
    cx: usize,
    cy: usize,
    fg: u32,
    bg: u32,
}

impl Term {
    fn set_theme(&mut self, theme: Theme) {
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
            recolor(line);
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

    fn blank(&self) -> Cell {
        Cell {
            ch: ' ',
            fg: self.default_fg,
            bg: self.default_bg,
        }
    }
    fn new(cols: usize, rows: usize, theme: Theme) -> Self {
        Self {
            cols,
            rows,
            cells: vec![
                Cell {
                    ch: ' ',
                    fg: theme.fg,
                    bg: theme.bg
                };
                cols * rows
            ],
            scrollback: VecDeque::new(),
            scroll_offset: 0,
            primary_screen: None,
            cursor_visible: true,
            cx: 0,
            cy: 0,
            default_fg: theme.fg,
            default_bg: theme.bg,
            palette: theme.palette,
            fg: theme.fg,
            bg: theme.bg,
        }
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.cols + x
    }

    fn resize(&mut self, cols: usize, rows: usize) {
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
        for line in &mut self.scrollback {
            line.resize(cols, blank);
            line.truncate(cols);
        }
        self.cols = cols;
        self.rows = rows;
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
    }

    fn scroll_up(&mut self) {
        if self.primary_screen.is_none() {
            let first_row = self.cells[..self.cols].to_vec();
            self.scrollback.push_back(first_row);
            if self.scrollback.len() > 5000 {
                self.scrollback.pop_front();
            }
        }
        self.cells.copy_within(self.cols.., 0);
        let end = self.cells.len();
        let blank = self.blank();
        self.cells[end - self.cols..].fill(blank);
        if self.scroll_offset > 0 {
            self.scroll_offset = (self.scroll_offset + 1).min(self.scrollback.len());
        }
    }

    fn scroll_view(&mut self, lines: isize) {
        self.scroll_offset = if lines > 0 {
            self.scroll_offset
                .saturating_add(lines as usize)
                .min(self.scrollback.len())
        } else {
            self.scroll_offset.saturating_sub(lines.unsigned_abs())
        };
    }

    fn alternate_screen(&mut self, enabled: bool) {
        if enabled && self.primary_screen.is_none() {
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
        } else if !enabled {
            if let Some((cells, cx, cy, fg, bg)) = self.primary_screen.take() {
                self.cells = cells;
                self.cx = cx;
                self.cy = cy;
                self.fg = fg;
                self.bg = bg;
                self.scroll_offset = 0;
            }
        }
    }

    fn linefeed(&mut self) {
        if self.cy + 1 < self.rows {
            self.cy += 1;
        } else {
            self.scroll_up();
        }
    }

    fn clear_range(&mut self, from: usize, to: usize) {
        for i in from..to.min(self.cells.len()) {
            self.cells[i] = Cell {
                ch: ' ',
                fg: self.default_fg,
                bg: self.default_bg,
            };
        }
    }

    fn sgr(&mut self, params: &Params) {
        let values: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        if values.is_empty() {
            self.fg = self.default_fg;
            self.bg = self.default_bg;
            return;
        }
        let mut i = 0;
        while i < values.len() {
            let code = values[i];
            match code {
                0 => {
                    self.fg = self.default_fg;
                    self.bg = self.default_bg;
                }
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

impl Perform for Term {
    fn print(&mut self, c: char) {
        if self.cx >= self.cols {
            self.cx = 0;
            self.linefeed();
        }
        let i = self.idx(self.cx, self.cy);
        self.cells[i] = Cell {
            ch: c,
            fg: self.fg,
            bg: self.bg,
        };
        self.cx += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => self.linefeed(),
            b'\r' => self.cx = 0,
            0x08 => self.cx = self.cx.saturating_sub(1),
            b'\t' => self.cx = (((self.cx / 8) + 1) * 8).min(self.cols - 1),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _: &[u8], private: bool, action: char) {
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
                    25 => self.cursor_visible = enabled,
                    1049 | 47 | 1047 => self.alternate_screen(enabled),
                    _ => {}
                }
            }
            return;
        }

        match action {
            'A' => self.cy = self.cy.saturating_sub(p(0, 1)),
            'B' => self.cy = (self.cy + p(0, 1)).min(self.rows - 1),
            'C' => self.cx = (self.cx + p(0, 1)).min(self.cols - 1),
            'D' => self.cx = self.cx.saturating_sub(p(0, 1)),
            'G' => self.cx = (p(0, 1) - 1).min(self.cols - 1),
            'H' | 'f' => {
                self.cy = (p(0, 1) - 1).min(self.rows - 1);
                self.cx = (p(1, 1) - 1).min(self.cols - 1);
            }
            'J' => match raw0 {
                0 => {
                    let from = self.idx(self.cx, self.cy);
                    self.clear_range(from, self.cells.len());
                }
                2 => self.clear_range(0, self.cells.len()),
                3 => {
                    self.clear_range(0, self.cells.len());
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
                    2 => self.clear_range(row_start, row_end),
                    _ => {}
                }
            }
            '@' => {
                let count = p(0, 1).min(self.cols - self.cx);
                let start = self.idx(self.cx, self.cy);
                let end = self.idx(self.cols - count, self.cy);
                self.cells.copy_within(start..end, start + count);
                self.clear_range(start, start + count);
            }
            'P' => {
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
            'L' | 'M' if self.cy < self.rows => {
                let count = p(0, 1).min(self.rows - self.cy);
                let start = self.cy * self.cols;
                let end = self.cells.len();
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
            _ => {}
        }
    }
}

// ---- Events from the PTY reader thread --------------------------------------

#[derive(Debug)]
enum UserEvent {
    Output(Vec<u8>),
    Closed,
}

// ---- App ---------------------------------------------------------------------

struct App {
    theme: Theme,
    font: Font,
    glyphs: HashMap<char, (Metrics, Vec<u8>)>,
    px: f32,
    cw: usize,
    ch: usize,
    ascent: i32,
    margin: usize,
    top: usize,
    border: usize,

    term: Term,
    parser: Parser,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,

    window: Option<Arc<Window>>,
    _context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    width: u32,
    height: u32,
    ctrl: bool,
    shift: bool,
}

impl App {
    fn cycle_theme(&mut self) {
        self.theme = Theme::from_kind(self.theme.kind.next());
        self.term.set_theme(self.theme);
        if let Err(error) = save_config_theme(self.theme.kind) {
            eprintln!("poopterm: could not save theme config: {error}");
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        self.width = w.max(1);
        self.height = h.max(1);

        let cols = ((self.width as usize).saturating_sub(2 * self.margin) / self.cw.max(1)).max(2);
        let rows =
            ((self.height as usize).saturating_sub(self.top + self.margin) / self.ch.max(1)).max(2);

        if cols != self.term.cols || rows != self.term.rows {
            self.term.resize(cols, rows);
            let _ = self.master.resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }

        if let Some(s) = self.surface.as_mut() {
            if let (Some(nw), Some(nh)) =
                (NonZeroU32::new(self.width), NonZeroU32::new(self.height))
            {
                let _ = s.resize(nw, nh);
            }
        }
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }

    fn key_bytes(&self, key: &Key) -> Option<Vec<u8>> {
        match key {
            Key::Named(n) => match n {
                NamedKey::Enter => Some(b"\r".to_vec()),
                NamedKey::Backspace => Some(vec![0x7f]),
                NamedKey::Tab => Some(b"\t".to_vec()),
                NamedKey::Escape => Some(vec![0x1b]),
                NamedKey::Space => Some(b" ".to_vec()),
                NamedKey::ArrowUp => Some(b"\x1b[A".to_vec()),
                NamedKey::ArrowDown => Some(b"\x1b[B".to_vec()),
                NamedKey::ArrowRight => Some(b"\x1b[C".to_vec()),
                NamedKey::ArrowLeft => Some(b"\x1b[D".to_vec()),
                NamedKey::Home => Some(b"\x1b[H".to_vec()),
                NamedKey::End => Some(b"\x1b[F".to_vec()),
                NamedKey::PageUp => Some(b"\x1b[5~".to_vec()),
                NamedKey::PageDown => Some(b"\x1b[6~".to_vec()),
                NamedKey::Insert => Some(b"\x1b[2~".to_vec()),
                NamedKey::Delete => Some(b"\x1b[3~".to_vec()),
                NamedKey::F1 => Some(b"\x1bOP".to_vec()),
                NamedKey::F2 => Some(b"\x1bOQ".to_vec()),
                NamedKey::F3 => Some(b"\x1bOR".to_vec()),
                NamedKey::F4 => Some(b"\x1bOS".to_vec()),
                NamedKey::F5 => Some(b"\x1b[15~".to_vec()),
                NamedKey::F6 => Some(b"\x1b[17~".to_vec()),
                NamedKey::F7 => Some(b"\x1b[18~".to_vec()),
                NamedKey::F8 => Some(b"\x1b[19~".to_vec()),
                NamedKey::F9 => Some(b"\x1b[20~".to_vec()),
                NamedKey::F10 => Some(b"\x1b[21~".to_vec()),
                NamedKey::F11 => Some(b"\x1b[23~".to_vec()),
                NamedKey::F12 => Some(b"\x1b[24~".to_vec()),
                _ => None,
            },
            Key::Character(s) => {
                let s = s.as_str();
                if self.ctrl && s.len() == 1 {
                    let c = s.as_bytes()[0];
                    if c.is_ascii_alphabetic() {
                        return Some(vec![c.to_ascii_lowercase() & 0x1f]);
                    }
                    return None;
                }
                Some(s.as_bytes().to_vec())
            }
            _ => None,
        }
    }

    fn draw(&mut self) {
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let pixels: &mut [u32] = &mut buf;
        let (w, h) = (self.width as usize, self.height as usize);

        // Caramel rim, then chocolate interior.
        pixels.fill(self.theme.border);
        fill_rect(
            pixels,
            w,
            h,
            self.border,
            self.border,
            w.saturating_sub(2 * self.border),
            h.saturating_sub(2 * self.border),
            self.theme.bg,
        );

        let (cw, ch, m, top, px, ascent) = (
            self.cw,
            self.ch,
            self.margin,
            self.top,
            self.px,
            self.ascent,
        );
        let font = &self.font;
        let glyphs = &mut self.glyphs;
        let term = &self.term;
        let history_len = term.scrollback.len();
        let screen_start = history_len.saturating_sub(term.scroll_offset);

        for row in 0..term.rows {
            let absolute_row = screen_start + row;
            let row_cells = if absolute_row < history_len {
                &term.scrollback[absolute_row]
            } else {
                let start = (absolute_row - history_len) * term.cols;
                &term.cells[start..start + term.cols]
            };
            for (col, &cell) in row_cells.iter().enumerate() {
                let is_cursor = term.cursor_visible
                    && term.scroll_offset == 0
                    && row == term.cy
                    && col == term.cx;
                let (fg, bg) = if is_cursor {
                    (cell.bg, cell.fg)
                } else {
                    (cell.fg, cell.bg)
                };

                let x0 = m + col * cw;
                let y0 = top + row * ch;
                fill_rect(pixels, w, h, x0, y0, cw, ch, bg);

                if cell.ch == ' ' {
                    continue;
                }
                let (gm, bitmap) = glyphs
                    .entry(cell.ch)
                    .or_insert_with(|| font.rasterize(cell.ch, px));

                let gx0 = x0 as i32 + gm.xmin;
                let gy0 = y0 as i32 + ascent - gm.ymin - gm.height as i32;
                for gy in 0..gm.height {
                    for gx in 0..gm.width {
                        let a = bitmap[gy * gm.width + gx] as u32;
                        if a == 0 {
                            continue;
                        }
                        let x = gx0 + gx as i32;
                        let y = gy0 + gy as i32;
                        if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                            continue;
                        }
                        pixels[y as usize * w + x as usize] = blend(bg, fg, a);
                    }
                }
            }
        }

        let _ = buf.present();
    }
}

fn fill_rect(
    px: &mut [u32],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
    rw: usize,
    rh: usize,
    color: u32,
) {
    let x_end = x.saturating_add(rw).min(w);
    if x >= x_end {
        return;
    }
    for yy in y..y.saturating_add(rh).min(h) {
        let row_start = yy * w + x;
        let row_end = yy * w + x_end;
        px[row_start..row_end].fill(color);
    }
}

fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    let mix = |shift: u32| {
        let f = (fg >> shift) & 0xff;
        let b = (bg >> shift) & 0xff;
        ((f * a + b * (255 - a)) / 255) << shift
    };
    mix(16) | mix(8) | mix(0)
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("💩 poopterm")
            .with_inner_size(LogicalSize::new(900.0, 560.0));

        // On macOS, keep the native frame so the traffic lights exist,
        // but make the title bar transparent and hide the title text.
        #[cfg(target_os = "macos")]
        let attrs = attrs
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true);

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

        let scale = window.scale_factor() as f32;
        self.px = 18.0 * scale;
        self.margin = (8.0 * scale) as usize;
        self.border = (scale * 3.0) as usize;
        self.top = if cfg!(target_os = "macos") {
            (28.0 * scale) as usize // space for the traffic lights
        } else {
            self.margin
        };

        let lm = self
            .font
            .horizontal_line_metrics(self.px)
            .expect("font line metrics");
        self.ascent = lm.ascent.round() as i32;
        self.ch = lm.new_line_size.ceil() as usize;
        self.cw = self.font.metrics('M', self.px).advance_width.ceil() as usize;

        let size = PhysicalSize::new(
            (self.cw * self.term.cols + 2 * self.margin) as u32,
            (self.ch * self.term.rows + self.top + self.margin) as u32,
        );
        let _ = window.request_inner_size(size);

        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        self.surface = Some(surface);
        self._context = Some(context);
        self.window = Some(window);

        let s = self.window.as_ref().unwrap().inner_size();
        self.resize(s.width, s.height);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => {
                self.ctrl = m.state().control_key();
                self.shift = m.state().shift_key();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as isize,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 24.0).round() as isize,
                };
                self.term.scroll_view(lines);
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if self.ctrl
                    && self.shift
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("t"))
                {
                    self.cycle_theme();
                    return;
                }
                if let Some(bytes) = self.key_bytes(&event.logical_key) {
                    let _ = self.writer.write_all(&bytes);
                    let _ = self.writer.flush();
                }
            }
            // No title bar, so click-drag anywhere moves the window.
            WindowEvent::MouseInput { state, button, .. }
                if state == ElementState::Pressed && button == MouseButton::Left =>
            {
                if let Some(win) = &self.window {
                    let _ = win.drag_window();
                }
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Output(bytes) => {
                // vte's Parser consumes one byte at a time.
                for &b in bytes.iter() {
                    self.parser.advance(&mut self.term, b);
                }
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            UserEvent::Closed => event_loop.exit(),
        }
    }
}

// ---- main --------------------------------------------------------------------

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let font = Font::from_bytes(
        include_bytes!("../assets/font.ttf") as &[u8],
        FontSettings::default(),
    )?;

    let (cols, rows) = (100usize, 30usize);
    let pty = native_pty_system();
    let pair = pty.openpty(PtySize {
        rows: rows as u16,
        cols: cols as u16,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut cmd = CommandBuilder::new(shell);
    cmd.env("TERM", "xterm-256color");
    #[cfg(target_os = "macos")]
    cmd.arg("-l");
    let _child = pair.slave.spawn_command(cmd)?;
    // The slave must be closed in the parent for EOF to arrive.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;

    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = proxy.send_event(UserEvent::Closed);
                    break;
                }
                Ok(n) => {
                    if proxy
                        .send_event(UserEvent::Output(buf[..n].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    let theme = Theme::load();
    let mut app = App {
        theme,
        font,
        glyphs: HashMap::new(),
        px: 18.0,
        cw: 10,
        ch: 20,
        ascent: 15,
        margin: 8,
        top: 8,
        border: 1,
        term: Term::new(cols, rows, theme),
        parser: Parser::new(),
        master: pair.master,
        writer,
        window: None,
        _context: None,
        surface: None,
        width: 0,
        height: 0,
        ctrl: false,
        shift: false,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}
