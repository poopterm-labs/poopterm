use std::{
    collections::{HashMap, VecDeque},
    env,
    error::Error,
    fs,
    io::{Read, Write},
    num::NonZeroU32,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Arc,
    thread,
};

use fontdue::{Font, FontSettings, Metrics};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use unicode_width::UnicodeWidthChar;
use vte::{Params, Parser, Perform};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
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
    MacOsDark,
    MacOsLight,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    Poop,
}

impl ThemeKind {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "macos" | "mac" | "macos-dark" => Some(Self::MacOsDark),
            "macos-light" | "light" => Some(Self::MacOsLight),
            "catppuccin" | "catppuccin-mocha" => Some(Self::CatppuccinMocha),
            "catppuccin-latte" => Some(Self::CatppuccinLatte),
            "catppuccin-frappe" => Some(Self::CatppuccinFrappe),
            "catppuccin-macchiato" => Some(Self::CatppuccinMacchiato),
            "poop" => Some(Self::Poop),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::MacOsDark => "macos-dark",
            Self::MacOsLight => "macos-light",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::CatppuccinFrappe => "catppuccin-frappe",
            Self::CatppuccinMacchiato => "catppuccin-macchiato",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::Poop => "poop",
        }
    }
    fn next(self) -> Self {
        match self {
            Self::MacOsDark => Self::MacOsLight,
            Self::MacOsLight => Self::CatppuccinLatte,
            Self::CatppuccinLatte => Self::CatppuccinFrappe,
            Self::CatppuccinFrappe => Self::CatppuccinMacchiato,
            Self::CatppuccinMacchiato => Self::CatppuccinMocha,
            Self::CatppuccinMocha => Self::Poop,
            Self::Poop => Self::MacOsDark,
        }
    }
}

impl Theme {
    fn from_kind(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::MacOsDark => Self {
                kind,
                fg: 0xE6E6E6,
                bg: 0x1E1E1E,
                border: 0x3A3A3C,
                palette: [
                    0x000000, 0xC23621, 0x25BC26, 0xADAD27, 0x492EE1, 0xD338D3, 0x33BBC8, 0xBBBBBB,
                ],
            },
            ThemeKind::MacOsLight => Self {
                kind,
                fg: 0x1D1D1F,
                bg: 0xF5F5F7,
                border: 0xD2D2D7,
                palette: [
                    0x6E6E73, 0xC41E3A, 0x248A3D, 0x9A6700, 0x0066CC, 0x8944AB, 0x007A82, 0x1D1D1F,
                ],
            },
            ThemeKind::CatppuccinLatte => Self {
                kind,
                fg: 0x4C4F69,
                bg: 0xEFF1F5,
                border: 0xCCD0DA,
                palette: [
                    0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE,
                ],
            },
            ThemeKind::CatppuccinFrappe => Self {
                kind,
                fg: 0xC6D0F5,
                bg: 0x303446,
                border: 0x414559,
                palette: [
                    0x51576D, 0xE78284, 0xA6D189, 0xE5C890, 0x8CAAEE, 0xCA9EE6, 0x81C8BE, 0xB5BFE2,
                ],
            },
            ThemeKind::CatppuccinMacchiato => Self {
                kind,
                fg: 0xCAD3F5,
                bg: 0x24273A,
                border: 0x363A4F,
                palette: [
                    0x494D64, 0xED8796, 0xA6DA95, 0xEED49F, 0x8AADF4, 0xF5BDE6, 0x8BD5CA, 0xB8C0E0,
                ],
            },
            ThemeKind::CatppuccinMocha => Self {
                kind,
                fg: 0xCDD6F4,
                bg: 0x1E1E2E,
                border: 0x45475A,
                palette: [
                    0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
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
    ThemeKind::parse(&read_config_value("theme")?)
}

// Session state is separate from user settings; hex encoding preserves unusual paths.
fn read_session_dirs() -> Vec<PathBuf> {
    fs::read_to_string(config_path().with_file_name("sessions.txt"))
        .unwrap_or_default()
        .lines()
        .filter_map(decode_session_path)
        .take(12)
        .collect()
}

fn decode_session_path(line: &str) -> Option<PathBuf> {
    if line.len() % 2 != 0 {
        return None;
    }
    let bytes = line
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        })
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

fn save_session_dirs(dirs: &[PathBuf]) -> std::io::Result<()> {
    let path = config_path().with_file_name("sessions.txt");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = dirs
        .iter()
        .take(12)
        .map(|dir| {
            dir.to_string_lossy()
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let temp = path.with_extension("tmp");
    fs::write(&temp, text)?;
    fs::rename(temp, path)
}

fn default_cwd() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn restored_session_dirs() -> Vec<PathBuf> {
    let dirs: Vec<_> = read_session_dirs()
        .into_iter()
        .filter(|dir| dir.is_dir())
        .collect();
    if dirs.is_empty() {
        vec![default_cwd()]
    } else {
        dirs
    }
}

fn read_config_value(key: &str) -> Option<String> {
    fs::read_to_string(config_path())
        .ok()?
        .lines()
        .find_map(|line| {
            let line = line.split('#').next()?.trim();
            let (found, value) = line.split_once('=')?;
            (found.trim() == key).then(|| value.trim().trim_matches('"').to_owned())
        })
}

fn save_config_theme(kind: ThemeKind) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let previous = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = previous
        .lines()
        .filter(|line| {
            line.split('#')
                .next()
                .and_then(|s| s.split_once('='))
                .is_none_or(|(key, _)| {
                    !matches!(
                        key.trim(),
                        "theme" | "discord_enabled" | "discord_client_id"
                    )
                })
        })
        .map(str::to_owned)
        .collect();
    lines.push(format!("theme = \"{}\"", kind.name()));
    fs::write(
        path,
        format!("# Poopterm configuration\n{}\n", lines.join("\n")),
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
    width: u8,
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
    scroll_top: usize,
    scroll_bottom: usize,
    saved_cursor: (usize, usize),
    bracketed_paste: bool,
    mouse_tracking: u16,
    mouse_sgr: bool,
    application_cursor: bool,
    origin_mode: bool,
    auto_wrap: bool,
    bold: bool,
    reverse: bool,
    responses: Vec<Vec<u8>>,
    title: String,
    cwd: PathBuf,
    bell: bool,
}

impl Term {
    fn row_cells(&self, row: usize) -> Option<&[Cell]> {
        if row < self.scrollback.len() {
            self.scrollback.get(row).map(Vec::as_slice)
        } else {
            let row = row - self.scrollback.len();
            (row < self.rows).then(|| {
                let start = row * self.cols;
                &self.cells[start..start + self.cols]
            })
        }
    }

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
            width: 1,
        }
    }
    fn new(cols: usize, rows: usize, theme: Theme, cwd: PathBuf) -> Self {
        Self {
            cols,
            rows,
            cells: vec![
                Cell {
                    ch: ' ',
                    fg: theme.fg,
                    bg: theme.bg,
                    width: 1,
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
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            saved_cursor: (0, 0),
            bracketed_paste: false,
            mouse_tracking: 0,
            mouse_sgr: false,
            application_cursor: false,
            origin_mode: false,
            auto_wrap: true,
            bold: false,
            reverse: false,
            responses: Vec::new(),
            title: "Shell".into(),
            cwd,
            bell: false,
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
        if self.cy == self.scroll_bottom {
            self.scroll_region_up(1);
        } else if self.cy + 1 < self.rows {
            self.cy += 1;
        } else {
            self.scroll_up();
        }
    }

    fn scroll_region_up(&mut self, count: usize) {
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
                    self.cells[from..from + self.cols].to_vec()
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        self.cells
            .copy_within(start + count * self.cols..end, start);
        let blank = self.blank();
        self.cells[end - count * self.cols..end].fill(blank);
        self.scrollback.extend(history);
        while self.scrollback.len() > 5000 {
            self.scrollback.pop_front();
        }
    }

    fn scroll_region_down(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom - self.scroll_top + 1);
        let start = self.scroll_top * self.cols;
        let end = (self.scroll_bottom + 1) * self.cols;
        self.cells
            .copy_within(start..end - count * self.cols, start + count * self.cols);
        let blank = self.blank();
        self.cells[start..start + count * self.cols].fill(blank);
    }

    fn clear_range(&mut self, from: usize, to: usize) {
        for i in from..to.min(self.cells.len()) {
            self.cells[i] = Cell {
                ch: ' ',
                fg: self.default_fg,
                bg: self.default_bg,
                width: 1,
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
                    self.bold = false;
                    self.reverse = false;
                }
                1 => self.bold = true,
                22 => self.bold = false,
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

impl Perform for Term {
    fn print(&mut self, c: char) {
        let width = c.width().unwrap_or(0);
        if width == 0 {
            return;
        }
        if self.cx + width > self.cols {
            if self.auto_wrap {
                self.cx = 0;
                self.linefeed();
            } else {
                self.cx = self.cols.saturating_sub(width);
            }
        }
        let i = self.idx(self.cx, self.cy);
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
        self.cells[i] = Cell {
            ch: c,
            fg,
            bg,
            width: width as u8,
        };
        if width == 2 && self.cx + 1 < self.cols {
            self.cells[i + 1] = Cell {
                ch: ' ',
                fg,
                bg,
                width: 0,
            };
        }
        self.cx += width;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            7 => self.bell = true,
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
                    1 => self.application_cursor = enabled,
                    6 => self.origin_mode = enabled,
                    7 => self.auto_wrap = enabled,
                    25 => self.cursor_visible = enabled,
                    1049 | 47 | 1047 => self.alternate_screen(enabled),
                    1000 | 1002 | 1003 => self.mouse_tracking = if enabled { p } else { 0 },
                    1006 => self.mouse_sgr = enabled,
                    2004 => self.bracketed_paste = enabled,
                    _ => {}
                }
            }
            return;
        }

        match action {
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
            'G' => self.cx = (p(0, 1) - 1).min(self.cols - 1),
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
                    self.cy = top;
                }
            }
            'S' => self.scroll_region_up(p(0, 1)),
            'T' => self.scroll_region_down(p(0, 1)),
            's' => self.saved_cursor = (self.cx, self.cy),
            'u' => {
                self.cx = self.saved_cursor.0.min(self.cols - 1);
                self.cy = self.saved_cursor.1.min(self.rows - 1);
            }
            'J' => match raw0 {
                0 => {
                    let from = self.idx(self.cx, self.cy);
                    self.clear_range(from, self.cells.len());
                }
                1 => self.clear_range(0, self.idx(self.cx + 1, self.cy)),
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
                    1 => self.clear_range(row_start, self.idx(self.cx + 1, self.cy)),
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
            'c' => self.responses.push(b"\x1b[?1;2c".to_vec()),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore || !intermediates.is_empty() {
            return;
        }
        match byte {
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
            b'7' => self.saved_cursor = (self.cx, self.cy),
            b'8' => {
                self.cx = self.saved_cursor.0.min(self.cols - 1);
                self.cy = self.saved_cursor.1.min(self.rows - 1);
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _: bool) {
        if params.len() >= 2 && matches!(params[0], b"0" | b"2") {
            let title = String::from_utf8_lossy(params[1]);
            self.title = title.chars().filter(|c| !c.is_control()).take(64).collect();
        } else if params.len() >= 2 && params[0] == b"7" {
            if let Ok(uri) = std::str::from_utf8(params[1]) {
                if let Some(path) = uri.strip_prefix("file://") {
                    let path = path.find('/').map(|index| &path[index..]).unwrap_or(path);
                    if !path.is_empty() {
                        self.cwd = PathBuf::from(path.replace("%20", " "));
                    }
                }
            }
        }
    }
}

// ---- Events from the PTY reader thread --------------------------------------

#[derive(Debug)]
enum UserEvent {
    Output(u64, Vec<u8>),
    Closed(u64),
}

struct Session {
    id: u64,
    term: Term,
    parser: Parser,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    _child: Box<dyn Child + Send + Sync>,
    activity: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellPoint {
    row: usize,
    col: usize,
}

#[derive(Clone, Copy)]
struct Selection {
    start: CellPoint,
    end: CellPoint,
}

struct SettingsEditor {
    theme: ThemeKind,
    status: String,
}

impl SettingsEditor {
    fn load() -> Self {
        Self {
            theme: read_config_theme().unwrap_or(ThemeKind::Poop),
            status: "Choose a theme and save your preference.".into(),
        }
    }
}

impl Selection {
    fn ordered(self) -> (CellPoint, CellPoint) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

struct TabLayout {
    start: usize,
    width: usize,
    gap: usize,
    first_visible: usize,
    visible_count: usize,
    left_arrow_x: usize,
    right_arrow_x: usize,
    arrow_w: usize,
    plus_x: usize,
    plus_w: usize,
    settings_x: usize,
    settings_w: usize,
    scale: usize,
}

fn tab_layout(
    width: usize,
    count: usize,
    scale: usize,
    left_reserve: usize,
    scroll: usize,
) -> TabLayout {
    let scale = scale.max(1);
    let arrow_w = 28 * scale;
    let left_arrow_x = left_reserve + 8 * scale;
    let start = left_arrow_x + arrow_w + 6 * scale;
    let plus_w = 52 * scale;
    let settings_w = 42 * scale;
    let controls_gap = 8 * scale;
    let right = 12 * scale;
    let settings_x = width.saturating_sub(right + settings_w);
    let plus_x = settings_x.saturating_sub(controls_gap + plus_w);
    let right_arrow_x = plus_x.saturating_sub(controls_gap + arrow_w);
    let viewport_end = right_arrow_x.saturating_sub(8 * scale);
    let gap = 6 * scale;
    let room = viewport_end.saturating_sub(start);
    let slots = ((room + gap) / (170 * scale + gap)).max(1);
    let tab_width = room
        .saturating_sub(slots.saturating_sub(1) * gap)
        .checked_div(slots)
        .unwrap_or(0)
        .min(220 * scale);
    let visible_count = ((room + gap) / (tab_width.max(1) + gap)).max(1);
    let first_visible = scroll.min(count.saturating_sub(visible_count));
    TabLayout {
        start,
        width: tab_width,
        gap,
        first_visible,
        visible_count,
        left_arrow_x,
        right_arrow_x,
        arrow_w,
        plus_x,
        plus_w,
        settings_x,
        settings_w,
        scale,
    }
}

fn spawn_session(
    id: u64,
    cols: usize,
    rows: usize,
    theme: Theme,
    proxy: EventLoopProxy<UserEvent>,
    cwd: PathBuf,
) -> Result<Session, Box<dyn Error>> {
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
    cmd.arg("-l");
    let cwd = if cwd.is_dir() { cwd } else { default_cwd() };
    cmd.cwd(&cwd);
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = proxy.send_event(UserEvent::Closed(id));
                    break;
                }
                Ok(n) => {
                    if proxy
                        .send_event(UserEvent::Output(id, buf[..n].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
    });

    Ok(Session {
        id,
        term: Term::new(cols, rows, theme, cwd),
        parser: Parser::new(),
        master: pair.master,
        writer,
        _child: child,
        activity: false,
    })
}

// ---- App ---------------------------------------------------------------------

struct App {
    theme: Theme,
    font: Font,
    glyphs: HashMap<(char, u32), (Metrics, Vec<u8>)>,
    px: f32,
    cw: usize,
    ch: usize,
    ascent: i32,
    margin: usize,
    top: usize,
    titlebar_height: usize,
    tab_height: usize,
    status_height: usize,
    border: usize,

    sessions: Vec<Session>,
    active: usize,
    tab_scroll: usize,
    next_session_id: u64,
    proxy: EventLoopProxy<UserEvent>,

    window: Option<Arc<Window>>,
    reported_title: String,
    _context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    settings_window: Option<Arc<Window>>,
    _settings_context: Option<softbuffer::Context<Arc<Window>>>,
    settings_surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    settings_width: u32,
    settings_height: u32,
    settings_cursor_pos: (f64, f64),
    settings_editor: SettingsEditor,
    width: u32,
    height: u32,
    ctrl: bool,
    shift: bool,
    super_key: bool,
    cursor_pos: (f64, f64),
    mouse_button_down: bool,
    selection: Option<Selection>,
    selecting: bool,
    search_active: bool,
    search_query: String,
    search_matches: Vec<Selection>,
    search_match_index: usize,
}

fn find_matches(term: &Term, query: &str) -> Vec<Selection> {
    let query = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    if !query.is_empty() {
        let needle: Vec<char> = query.chars().collect();
        let total_rows = term.scrollback.len() + term.rows;
        for row in 0..total_rows {
            let Some(cells) = term.row_cells(row) else {
                continue;
            };
            let chars: Vec<(char, usize)> = cells
                .iter()
                .enumerate()
                .filter(|(_, cell)| cell.width != 0)
                .map(|(col, cell)| (cell.ch.to_ascii_lowercase(), col))
                .collect();
            if needle.len() > chars.len() {
                continue;
            }
            for start in 0..=chars.len() - needle.len() {
                if chars[start..start + needle.len()]
                    .iter()
                    .map(|(ch, _)| *ch)
                    .eq(needle.iter().copied())
                {
                    matches.push(Selection {
                        start: CellPoint {
                            row,
                            col: chars[start].1,
                        },
                        end: CellPoint {
                            row,
                            col: chars[start + needle.len() - 1].1,
                        },
                    });
                }
            }
        }
    }
    matches
}

impl App {
    fn search(&mut self, direction: isize, advance: bool) {
        let matches = find_matches(self.active_term(), &self.search_query);
        self.search_matches = matches;
        if self.search_matches.is_empty() {
            self.search_match_index = 0;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }
        let len = self.search_matches.len();
        if advance {
            self.search_match_index =
                (self.search_match_index as isize + direction).rem_euclid(len as isize) as usize;
        } else {
            self.search_match_index = 0;
        }
        let match_range = self.search_matches[self.search_match_index];
        let term = &mut self.active_session_mut().term;
        let visible_top = match_range.start.row.saturating_sub(term.rows / 2);
        term.scroll_offset = term
            .scrollback
            .len()
            .saturating_sub(visible_top.min(term.scrollback.len()));
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn tab_left_reserve(&self) -> usize {
        if cfg!(target_os = "macos") {
            (80.0
                * self
                    .window
                    .as_ref()
                    .map_or(1.0, |window| window.scale_factor())) as usize
        } else {
            (112.0
                * self
                    .window
                    .as_ref()
                    .map_or(1.0, |window| window.scale_factor())) as usize
        }
    }

    fn tab_y(&self) -> usize {
        if cfg!(target_os = "macos") {
            self.titlebar_height.saturating_sub(self.tab_height) / 2
        } else {
            self.top.saturating_sub(self.tab_height)
        }
    }

    fn tab_layout(&self) -> TabLayout {
        tab_layout(
            self.width as usize,
            self.sessions.len(),
            (self.px / 18.0).round() as usize,
            self.tab_left_reserve(),
            self.tab_scroll,
        )
    }

    fn ensure_active_tab_visible(&mut self) {
        let layout = self.tab_layout();
        let mut first = layout.first_visible;
        if self.active < first {
            first = self.active;
        } else if self.active >= first + layout.visible_count {
            first = self.active + 1 - layout.visible_count;
        }
        self.tab_scroll = first;
    }

    fn open_settings_window(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.settings_window {
            window.focus_window();
            return;
        }
        self.settings_editor = SettingsEditor::load();
        let attributes = Window::default_attributes()
            .with_title("Poopterm Settings")
            .with_inner_size(LogicalSize::new(760.0, 560.0));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create settings window"),
        );
        let context = softbuffer::Context::new(window.clone()).expect("settings context");
        let mut surface =
            softbuffer::Surface::new(&context, window.clone()).expect("settings surface");
        let size = window.inner_size();
        if let (Some(width), Some(height)) = (
            NonZeroU32::new(size.width.max(1)),
            NonZeroU32::new(size.height.max(1)),
        ) {
            let _ = surface.resize(width, height);
        }
        self.settings_width = size.width;
        self.settings_height = size.height;
        self.settings_surface = Some(surface);
        self._settings_context = Some(context);
        self.settings_window = Some(window.clone());
        window.request_redraw();
    }

    fn save_settings_config(&mut self) {
        let path = config_path();
        let previous = fs::read_to_string(&path).unwrap_or_default();
        let mut lines: Vec<String> = previous
            .lines()
            .filter(|line| {
                let key = line.split('#').next().unwrap_or_default().split_once('=');
                key.is_none_or(|(key, _)| {
                    !matches!(
                        key.trim(),
                        "theme" | "discord_enabled" | "discord_client_id"
                    )
                })
            })
            .map(str::to_owned)
            .collect();
        lines.push(format!("theme = \"{}\"", self.settings_editor.theme.name()));
        let text = format!("# Poopterm configuration\n{}\n", lines.join("\n"));
        let result = path
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .and_then(|_| fs::write(&path, text));
        match result {
            Ok(()) => {
                self.apply_theme(self.settings_editor.theme);
                self.glyphs.clear();
                self.settings_editor = SettingsEditor::load();
                self.settings_editor.status = "Saved. Settings applied.".into();
            }
            Err(error) => {
                self.settings_editor.status = format!("Could not save: {error}");
            }
        }
        if let Some(window) = &self.settings_window {
            window.request_redraw();
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn draw_settings(&mut self) {
        let scale = self
            .settings_window
            .as_ref()
            .map_or(1, |window| window.scale_factor().round() as usize)
            .max(1);
        let Some(surface) = self.settings_surface.as_mut() else {
            return;
        };
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let pixels: &mut [u32] = &mut buffer;
        let (width, height) = (self.settings_width as usize, self.settings_height as usize);
        pixels.fill(self.theme.border);
        fill_rect(
            pixels,
            width,
            height,
            2 * scale,
            2 * scale,
            width.saturating_sub(4 * scale),
            height.saturating_sub(4 * scale),
            self.theme.bg,
        );
        let px = 18.0 * scale as f32;
        let ascent = self
            .font
            .horizontal_line_metrics(px)
            .map_or(self.ascent, |metrics| metrics.ascent.round() as i32);
        let cell_width = self.font.metrics('M', px).advance_width.ceil() as usize;
        let x = 24 * scale;
        let content_w = width.saturating_sub(48 * scale);
        let font = &self.font;
        let glyphs = &mut self.glyphs;
        draw_text(
            pixels,
            width,
            height,
            x,
            24 * scale,
            "Poopterm Settings",
            px,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        draw_text(
            pixels,
            width,
            height,
            x,
            62 * scale,
            "Appearance",
            px * 0.82,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        draw_text(
            pixels,
            width,
            height,
            x,
            112 * scale,
            "Color theme",
            px * 0.76,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        let theme_y = 138 * scale;
        let button_w = 48 * scale;
        let themes = [
            ThemeKind::MacOsDark,
            ThemeKind::MacOsLight,
            ThemeKind::CatppuccinLatte,
            ThemeKind::CatppuccinFrappe,
            ThemeKind::CatppuccinMacchiato,
            ThemeKind::CatppuccinMocha,
            ThemeKind::Poop,
        ];
        let index = themes
            .iter()
            .position(|theme| *theme == self.settings_editor.theme)
            .unwrap_or(0);
        for (bx, label) in [(x, "‹"), (x + content_w - button_w, "›")] {
            draw_kde_button(
                pixels,
                width,
                height,
                bx,
                theme_y,
                button_w,
                40 * scale,
                scale,
                self.theme.bg,
                self.theme.border,
                self.theme.palette[4],
                false,
            );
            draw_text(
                pixels,
                width,
                height,
                bx + 18 * scale,
                theme_y + 7 * scale,
                label,
                px,
                ascent,
                self.theme.fg,
                font,
                glyphs,
            );
        }
        let theme_x = x + button_w + 8 * scale;
        let theme_w = content_w.saturating_sub(2 * button_w + 16 * scale);
        draw_kde_button(
            pixels,
            width,
            height,
            theme_x,
            theme_y,
            theme_w,
            40 * scale,
            scale,
            self.theme.bg,
            self.theme.border,
            self.theme.palette[4],
            false,
        );
        let theme_name = self
            .settings_editor
            .theme
            .name()
            .replace("catppuccin-", "Catppuccin ")
            .replace("macos-", "macOS ");
        draw_text(
            pixels,
            width,
            height,
            theme_x + 12 * scale,
            theme_y + 9 * scale,
            &theme_name,
            px * 0.82,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        let _ = index;
        draw_text(
            pixels,
            width,
            height,
            x,
            210 * scale,
            "Theme changes are saved to your config file.",
            px * 0.72,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        let save_x = width.saturating_sub(140 * scale);
        let save_y = height.saturating_sub(52 * scale);
        let save_w = 120 * scale;
        let save_h = 36 * scale;
        let hovered = self.settings_cursor_pos.0 >= save_x as f64
            && self.settings_cursor_pos.0 < (save_x + save_w) as f64
            && self.settings_cursor_pos.1 >= save_y as f64
            && self.settings_cursor_pos.1 < (save_y + save_h) as f64;
        draw_kde_button(
            pixels,
            width,
            height,
            save_x,
            save_y,
            save_w,
            save_h,
            scale,
            self.theme.bg,
            self.theme.border,
            self.theme.palette[4],
            hovered,
        );
        draw_text(
            pixels,
            width,
            height,
            save_x + (save_w - 4 * cell_width) / 2,
            save_y + (save_h.saturating_sub(self.ch)) / 2,
            "Save",
            px,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        draw_text(
            pixels,
            width,
            height,
            x,
            height.saturating_sub(40 * scale),
            &self.settings_editor.status,
            px * 0.68,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        let _ = buffer.present();
    }

    fn settings_window_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.settings_window = None;
                self.settings_surface = None;
                self._settings_context = None;
            }
            WindowEvent::Resized(size) => {
                self.settings_width = size.width.max(1);
                self.settings_height = size.height.max(1);
                if let Some(surface) = self.settings_surface.as_mut() {
                    if let (Some(width), Some(height)) = (
                        NonZeroU32::new(self.settings_width),
                        NonZeroU32::new(self.settings_height),
                    ) {
                        let _ = surface.resize(width, height);
                    }
                }
                self.draw_settings();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.settings_cursor_pos = (position.x, position.y);
                if let Some(window) = &self.settings_window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let scale = self
                    .settings_window
                    .as_ref()
                    .map_or(1, |window| window.scale_factor().round() as usize)
                    .max(1);
                let (x, y) = (
                    self.settings_cursor_pos.0.max(0.0) as usize,
                    self.settings_cursor_pos.1.max(0.0) as usize,
                );
                let save_x = (self.settings_width as usize).saturating_sub(140 * scale);
                let save_y = (self.settings_height as usize).saturating_sub(52 * scale);
                if x >= save_x && x < save_x + 120 * scale && y >= save_y && y < save_y + 36 * scale
                {
                    self.save_settings_config();
                    return;
                }
                let content_x = 24 * scale;
                let content_w = (self.settings_width as usize).saturating_sub(48 * scale);
                let theme_y = 138 * scale;
                let button_w = 48 * scale;
                if y >= theme_y && y < theme_y + 40 * scale {
                    let themes = [
                        ThemeKind::MacOsDark,
                        ThemeKind::MacOsLight,
                        ThemeKind::CatppuccinLatte,
                        ThemeKind::CatppuccinFrappe,
                        ThemeKind::CatppuccinMacchiato,
                        ThemeKind::CatppuccinMocha,
                        ThemeKind::Poop,
                    ];
                    let index = themes
                        .iter()
                        .position(|theme| *theme == self.settings_editor.theme)
                        .unwrap_or(0);
                    if x < content_x + button_w {
                        self.settings_editor.theme =
                            themes[(index + themes.len() - 1) % themes.len()];
                    } else if x >= content_x + content_w - button_w {
                        self.settings_editor.theme = themes[(index + 1) % themes.len()];
                    }
                }
                if let Some(window) = &self.settings_window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let is_save = (self.ctrl || self.super_key)
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("s"));
                if is_save || matches!(&event.logical_key, Key::Named(NamedKey::Enter)) {
                    self.save_settings_config();
                    return;
                }
                if matches!(&event.logical_key, Key::Named(NamedKey::Escape)) {
                    self.settings_window = None;
                    self.settings_surface = None;
                    self._settings_context = None;
                    return;
                }
                let themes = [
                    ThemeKind::MacOsDark,
                    ThemeKind::MacOsLight,
                    ThemeKind::CatppuccinLatte,
                    ThemeKind::CatppuccinFrappe,
                    ThemeKind::CatppuccinMacchiato,
                    ThemeKind::CatppuccinMocha,
                    ThemeKind::Poop,
                ];
                let index = themes
                    .iter()
                    .position(|theme| *theme == self.settings_editor.theme)
                    .unwrap_or(0);
                match &event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        self.settings_editor.theme =
                            themes[(index + themes.len() - 1) % themes.len()]
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        self.settings_editor.theme = themes[(index + 1) % themes.len()]
                    }
                    _ => {}
                }
                if let Some(window) = &self.settings_window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.draw_settings(),
            _ => {}
        }
    }

    fn active_term(&self) -> &Term {
        &self.sessions[self.active].term
    }

    fn active_session_mut(&mut self) -> &mut Session {
        &mut self.sessions[self.active]
    }

    fn add_tab(&mut self) {
        self.refresh_directories();
        let id = self.next_session_id;
        self.next_session_id += 1;
        let cols = self.active_term().cols;
        let rows = self.active_term().rows;
        let cwd = self.active_term().cwd.clone();
        match spawn_session(id, cols, rows, self.theme, self.proxy.clone(), cwd) {
            Ok(session) => {
                self.sessions.push(session);
                self.active = self.sessions.len() - 1;
                self.ensure_active_tab_visible();
                self.selection = None;
                self.end_search();
                self.sessions[self.active].activity = false;
                self.persist_sessions();
                self.update_window_title();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            Err(error) => eprintln!("poopterm: could not open tab: {error}"),
        }
    }

    fn switch_tab(&mut self, offset: isize) {
        let count = self.sessions.len();
        if count == 0 {
            return;
        }
        let index = (self.active as isize + offset).rem_euclid(count as isize) as usize;
        self.activate_tab(index);
    }

    fn activate_tab(&mut self, index: usize) {
        if index >= self.sessions.len() {
            return;
        }
        self.active = index;
        self.sessions[self.active].activity = false;
        self.sessions[self.active].term.bell = false;
        self.ensure_active_tab_visible();
        self.selection = None;
        self.update_window_title();
        self.search_matches.clear();
        self.search_match_index = 0;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        if self.search_active {
            self.search(1, false);
        }
    }

    fn close_active_tab(&mut self, event_loop: &ActiveEventLoop) {
        self.close_tab(self.active, event_loop);
    }

    fn close_tab(&mut self, index: usize, event_loop: &ActiveEventLoop) {
        if index >= self.sessions.len() {
            return;
        }
        let _ = self.sessions[index]._child.kill();
        self.sessions.remove(index);
        self.persist_sessions();
        self.search_matches.clear();
        self.selection = None;
        if self.sessions.is_empty() {
            event_loop.exit();
            return;
        }
        if index < self.active {
            self.active -= 1;
        } else if index == self.active {
            self.active = self.active.min(self.sessions.len() - 1);
        }
        self.tab_scroll = self.tab_scroll.min(self.sessions.len().saturating_sub(1));
        self.ensure_active_tab_visible();
        self.persist_sessions();
        self.update_window_title();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_window_title(&mut self) {
        let title = self.active_term().title.clone();
        if self.reported_title == title {
            return;
        }
        self.reported_title.clone_from(&title);
        if let Some(window) = &self.window {
            window.set_title(&format!("{title} — poopterm"));
        }
    }

    fn refresh_directories(&mut self) {
        for session in &mut self.sessions {
            if let Some(pid) = session._child.process_id() {
                #[cfg(target_os = "linux")]
                if let Ok(path) = fs::read_link(format!("/proc/{pid}/cwd")) {
                    session.term.cwd = path;
                }
                #[cfg(target_os = "macos")]
                if let Ok(output) = Command::new("/usr/sbin/lsof")
                    .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
                    .output()
                {
                    if let Some(path) = String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .find_map(|line| line.strip_prefix('n'))
                    {
                        session.term.cwd = PathBuf::from(path);
                    }
                }
            }
        }
    }

    fn persist_sessions(&self) {
        let dirs: Vec<_> = self
            .sessions
            .iter()
            .map(|session| session.term.cwd.clone())
            .collect();
        if let Err(error) = save_session_dirs(&dirs) {
            eprintln!("poopterm: could not save sessions: {error}");
        }
    }

    fn point_at(&self, x: f64, y: f64) -> Option<CellPoint> {
        let term = self.active_term();
        if x < self.margin as f64
            || y < self.top as f64
            || y >= (self.height as usize).saturating_sub(self.status_height + self.margin) as f64
        {
            return None;
        }
        let col = ((x as usize - self.margin) / self.cw.max(1)).min(term.cols - 1);
        let row = ((y as usize - self.top) / self.ch.max(1)).min(term.rows - 1);
        Some(CellPoint {
            row: term.scrollback.len().saturating_sub(term.scroll_offset) + row,
            col,
        })
    }

    fn selected_text(&self) -> Option<String> {
        let selection = self.selection?;
        let (start, end) = selection.ordered();
        let term = self.active_term();
        let history_len = term.scrollback.len();
        let mut result = String::new();
        for row in start.row..=end.row {
            let cells = if row < history_len {
                term.scrollback.get(row)?.as_slice()
            } else {
                let screen_row = row - history_len;
                if screen_row >= term.rows {
                    break;
                }
                let begin = screen_row * term.cols;
                &term.cells[begin..begin + term.cols]
            };
            let from = if row == start.row { start.col } else { 0 };
            let to = if row == end.row {
                end.col
            } else {
                cells.len().saturating_sub(1)
            };
            for cell in cells.iter().take(to + 1).skip(from) {
                if cell.width != 0 {
                    result.push(cell.ch);
                }
            }
            if row != end.row {
                while result.ends_with(' ') {
                    result.pop();
                }
                result.push('\n');
            }
        }
        Some(result)
    }

    fn copy_selection(&self) {
        if let Some(text) = self.selected_text() {
            if let Err(error) = clipboard_write(&text) {
                eprintln!("poopterm: clipboard copy failed: {error}");
            }
        }
    }

    fn token_at(&self, point: CellPoint) -> Option<String> {
        let cells = self.active_term().row_cells(point.row)?;
        if point.col >= cells.len() || cells[point.col].ch.is_whitespace() {
            return None;
        }
        let is_token_char = |ch: char| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '<' | '>');
        let mut start = point.col;
        let mut end = point.col + 1;
        while start > 0 && is_token_char(cells[start - 1].ch) {
            start -= 1;
        }
        while end < cells.len() && is_token_char(cells[end].ch) {
            end += 1;
        }
        Some(
            cells[start..end]
                .iter()
                .filter(|cell| cell.width != 0)
                .map(|cell| cell.ch)
                .collect::<String>()
                .trim_matches(|ch: char| matches!(ch, ')' | ']' | '}' | ',' | '.' | ';' | ':'))
                .to_owned(),
        )
    }

    fn open_or_copy_token(&self, x: f64, y: f64) -> bool {
        let Some(point) = self.point_at(x, y) else {
            return false;
        };
        let Some(token) = self.token_at(point) else {
            return false;
        };
        if token.starts_with("https://") || token.starts_with("http://") {
            if let Err(error) = open_external(&token) {
                eprintln!("poopterm: could not open link: {error}");
            }
            return true;
        }
        if token.starts_with('/')
            || token.starts_with("~/")
            || token.starts_with("./")
            || token.starts_with("../")
            || token.contains('/')
        {
            if let Err(error) = clipboard_write(&token) {
                eprintln!("poopterm: could not copy path: {error}");
            }
            return true;
        }
        false
    }

    fn begin_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_index = 0;
        self.search(1, false);
    }

    fn end_search(&mut self) {
        self.search_active = false;
        self.search_matches.clear();
        self.search_query.clear();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn paste_clipboard(&mut self) {
        match clipboard_read() {
            Ok(text) => {
                let bracketed = self.active_term().bracketed_paste;
                let mut bytes = Vec::with_capacity(text.len() + 24);
                if bracketed {
                    bytes.extend_from_slice(b"\x1b[200~");
                }
                bytes.extend_from_slice(text.as_bytes());
                if bracketed {
                    bytes.extend_from_slice(b"\x1b[201~");
                }
                let session = self.active_session_mut();
                let _ = session.writer.write_all(&bytes);
                let _ = session.writer.flush();
            }
            Err(error) => eprintln!("poopterm: clipboard paste failed: {error}"),
        }
    }

    fn send_mouse_report(
        &mut self,
        button: u8,
        x: f64,
        y: f64,
        release: bool,
        motion: bool,
    ) -> bool {
        let Some(point) = self.point_at(x, y) else {
            return false;
        };
        let term = self.active_term();
        if term.mouse_tracking == 0 {
            return false;
        }
        if motion && term.mouse_tracking != 1002 && term.mouse_tracking != 1003 {
            return false;
        }
        let code = button + if motion { 32 } else { 0 };
        let visible_start = term.scrollback.len().saturating_sub(term.scroll_offset);
        let (col, row) = (point.col + 1, point.row.saturating_sub(visible_start) + 1);
        let payload = if term.mouse_sgr {
            format!(
                "\x1b[<{};{};{}{}",
                code,
                col,
                row,
                if release { "m" } else { "M" }
            )
        } else if col <= 223 && row <= 223 {
            format!(
                "\x1b[M{}{}{}",
                (if release { 3 } else { code }) + 32,
                col + 32,
                row + 32
            )
        } else {
            return true;
        };
        let session = self.active_session_mut();
        let _ = session.writer.write_all(payload.as_bytes());
        let _ = session.writer.flush();
        true
    }

    fn begin_selection(&mut self, x: f64, y: f64) {
        if let Some(point) = self.point_at(x, y) {
            self.selection = Some(Selection {
                start: point,
                end: point,
            });
            self.selecting = true;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn extend_selection(&mut self, x: f64, y: f64) {
        if !self.selecting {
            return;
        }
        if let Some(point) = self.point_at(x, y) {
            if let Some(selection) = &mut self.selection {
                selection.end = point;
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn cycle_theme(&mut self) {
        self.apply_theme(self.theme.kind.next());
    }

    fn apply_theme(&mut self, kind: ThemeKind) {
        self.theme = Theme::from_kind(kind);
        for session in &mut self.sessions {
            session.term.set_theme(self.theme);
        }
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
        self.ensure_active_tab_visible();

        let cols = ((self.width as usize).saturating_sub(2 * self.margin) / self.cw.max(1)).max(2);
        let rows = ((self.height as usize)
            .saturating_sub(self.top + self.margin + self.status_height)
            / self.ch.max(1))
        .max(2);

        for session in &mut self.sessions {
            if cols != session.term.cols || rows != session.term.rows {
                session.term.resize(cols, rows);
                let _ = session.master.resize(PtySize {
                    rows: rows as u16,
                    cols: cols as u16,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
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
                NamedKey::ArrowUp => Some(if self.active_term().application_cursor {
                    b"\x1bOA".to_vec()
                } else {
                    b"\x1b[A".to_vec()
                }),
                NamedKey::ArrowDown => Some(if self.active_term().application_cursor {
                    b"\x1bOB".to_vec()
                } else {
                    b"\x1b[B".to_vec()
                }),
                NamedKey::ArrowRight => Some(if self.active_term().application_cursor {
                    b"\x1bOC".to_vec()
                } else {
                    b"\x1b[C".to_vec()
                }),
                NamedKey::ArrowLeft => Some(if self.active_term().application_cursor {
                    b"\x1bOD".to_vec()
                } else {
                    b"\x1b[D".to_vec()
                }),
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
        let tab_y = self.tab_y();
        let scale = (self.px / 18.0).round() as usize;
        let left_reserve = self.tab_left_reserve();
        let session_count = self.sessions.len();
        let tab_scroll = self.tab_scroll;
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
        let term = &self.sessions[self.active].term;
        let selected = self.selection.map(Selection::ordered);
        let layout = tab_layout(w, session_count, scale, left_reserve, tab_scroll);
        let accent = self.theme.palette[4];
        fill_rect(pixels, w, h, 0, tab_y, w, self.tab_height, self.theme.bg);
        if !cfg!(target_os = "macos") {
            draw_text(
                pixels,
                w,
                h,
                16 * scale,
                tab_y + (self.tab_height.saturating_sub(ch)) / 2,
                "poopterm",
                px * 0.82,
                ascent,
                self.theme.fg,
                font,
                glyphs,
            );
        }
        fill_rect(
            pixels,
            w,
            h,
            0,
            tab_y + self.tab_height.saturating_sub(scale),
            w,
            scale,
            blend(self.theme.bg, self.theme.border, 90),
        );
        for index in
            layout.first_visible..session_count.min(layout.first_visible + layout.visible_count)
        {
            let session = &self.sessions[index];
            let local_index = index - layout.first_visible;
            let x = layout.start + local_index * (layout.width + layout.gap);
            let active = index == self.active;
            let hovered = self.cursor_pos.0 >= x as f64
                && self.cursor_pos.0 < (x + layout.width) as f64
                && self.cursor_pos.1 >= tab_y as f64
                && self.cursor_pos.1 < top as f64;
            if active || hovered {
                let color = if active {
                    blend(self.theme.bg, self.theme.border, 70)
                } else {
                    blend(self.theme.bg, self.theme.border, 38)
                };
                fill_round_rect(
                    pixels,
                    w,
                    h,
                    x,
                    tab_y + 4 * scale,
                    layout.width,
                    self.tab_height.saturating_sub(6 * scale),
                    5 * scale,
                    color,
                );
            }
            if active {
                fill_round_rect(
                    pixels,
                    w,
                    h,
                    x + 8 * scale,
                    tab_y + self.tab_height.saturating_sub(5 * scale),
                    layout.width.saturating_sub(16 * scale),
                    3 * scale,
                    scale,
                    accent,
                );
            }
            let title_chars = layout.width.saturating_sub(42 * scale) / self.cw.max(1);
            let mut label: String = session
                .term
                .title
                .chars()
                .take(title_chars.max(1))
                .collect();
            if session.term.title.chars().count() > title_chars.max(1) && title_chars > 1 {
                label.pop();
                label.push('…');
            }
            if session.activity || session.term.bell {
                draw_text(
                    pixels,
                    w,
                    h,
                    x + 10 * scale,
                    tab_y + (self.tab_height.saturating_sub(ch)) / 2,
                    if session.term.bell { "!" } else { "•" },
                    px * 0.62,
                    ascent,
                    accent,
                    font,
                    glyphs,
                );
            }
            draw_text(
                pixels,
                w,
                h,
                x + if session.activity {
                    22 * scale
                } else {
                    12 * scale
                },
                tab_y + (self.tab_height.saturating_sub(ch)) / 2,
                &label,
                px * 0.82,
                (ascent as f32 * 0.82) as i32,
                self.theme.fg,
                font,
                glyphs,
            );
            if self.sessions.len() > 1 {
                draw_text(
                    pixels,
                    w,
                    h,
                    x + layout.width.saturating_sub(28 * scale),
                    tab_y + (self.tab_height.saturating_sub(ch)) / 2,
                    "×",
                    px * 0.82,
                    (ascent as f32 * 0.82) as i32,
                    self.theme.fg,
                    font,
                    glyphs,
                );
            }
        }
        let arrow_y = tab_y + (self.tab_height.saturating_sub(30 * scale)) / 2;
        let arrow_h = 30 * scale;
        let can_scroll_left = layout.first_visible > 0;
        let can_scroll_right = layout.first_visible + layout.visible_count < session_count;
        for (x, label, enabled) in [
            (layout.left_arrow_x, "‹", can_scroll_left),
            (layout.right_arrow_x, "›", can_scroll_right),
        ] {
            if !enabled {
                continue;
            }
            let hovered = self.cursor_pos.0 >= x as f64
                && self.cursor_pos.0 < (x + layout.arrow_w) as f64
                && self.cursor_pos.1 >= arrow_y as f64
                && self.cursor_pos.1 < (arrow_y + arrow_h) as f64;
            draw_kde_button(
                pixels,
                w,
                h,
                x,
                arrow_y,
                layout.arrow_w,
                arrow_h,
                scale,
                self.theme.bg,
                self.theme.border,
                accent,
                hovered,
            );
            draw_text(
                pixels,
                w,
                h,
                x + (layout.arrow_w.saturating_sub(self.cw)) / 2,
                arrow_y + (arrow_h.saturating_sub(ch)) / 2,
                label,
                px * 0.82,
                (ascent as f32 * 0.82) as i32,
                self.theme.fg,
                font,
                glyphs,
            );
        }
        let button_y = tab_y + (self.tab_height.saturating_sub(34 * scale)) / 2;
        let button_h = 34 * scale;
        let plus_hovered = self.cursor_pos.0 >= layout.plus_x as f64
            && self.cursor_pos.0 < (layout.plus_x + layout.plus_w) as f64
            && self.cursor_pos.1 >= button_y as f64
            && self.cursor_pos.1 < (button_y + button_h) as f64;
        let settings_hovered = self.cursor_pos.0 >= layout.settings_x as f64
            && self.cursor_pos.0 < (layout.settings_x + layout.settings_w) as f64
            && self.cursor_pos.1 >= button_y as f64
            && self.cursor_pos.1 < (button_y + button_h) as f64;
        draw_kde_button(
            pixels,
            w,
            h,
            layout.plus_x,
            button_y,
            layout.plus_w,
            button_h,
            scale,
            self.theme.bg,
            self.theme.border,
            accent,
            plus_hovered,
        );
        draw_text(
            pixels,
            w,
            h,
            layout.plus_x + (layout.plus_w.saturating_sub(self.cw)) / 2,
            button_y + (button_h.saturating_sub(ch)) / 2,
            "+",
            px,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        draw_kde_button(
            pixels,
            w,
            h,
            layout.settings_x,
            button_y,
            layout.settings_w,
            button_h,
            scale,
            self.theme.bg,
            self.theme.border,
            accent,
            settings_hovered,
        );
        draw_text(
            pixels,
            w,
            h,
            layout.settings_x + (layout.settings_w.saturating_sub(self.cw)) / 2,
            button_y + (button_h.saturating_sub(ch)) / 2,
            "⚙",
            px,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        let history_len = term.scrollback.len();
        let screen_start = history_len.saturating_sub(term.scroll_offset);
        let active_search_match = self
            .search_matches
            .get(self.search_match_index)
            .map(|selection| selection.ordered());

        for row in 0..term.rows {
            let absolute_row = screen_start + row;
            let row_cells = if absolute_row < history_len {
                &term.scrollback[absolute_row]
            } else {
                let start = (absolute_row - history_len) * term.cols;
                &term.cells[start..start + term.cols]
            };
            for (col, &cell) in row_cells.iter().enumerate() {
                let is_selected = selected.is_some_and(|(start, end)| {
                    let point = CellPoint {
                        row: absolute_row,
                        col,
                    };
                    point >= start && point <= end
                });
                let is_search_match = active_search_match.is_some_and(|(start, end)| {
                    let point = CellPoint {
                        row: absolute_row,
                        col,
                    };
                    point >= start && point <= end
                });
                let is_cursor = term.cursor_visible
                    && term.scroll_offset == 0
                    && row == term.cy
                    && col == term.cx;
                let (mut fg, mut bg) = if is_cursor {
                    (cell.bg, cell.fg)
                } else {
                    (cell.fg, cell.bg)
                };
                if is_selected {
                    fg = 0x1E1E2E;
                    bg = 0x89B4FA;
                } else if is_search_match {
                    fg = self.theme.bg;
                    bg = self.theme.palette[3];
                }

                let x0 = m + col * cw;
                let y0 = top + row * ch;
                fill_rect(pixels, w, h, x0, y0, cw, ch, bg);

                if cell.ch == ' ' || cell.width == 0 {
                    continue;
                }
                let (gm, bitmap) = glyphs
                    .entry(glyph_cache_key(cell.ch, px))
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
                        if x < x0 as i32
                            || x >= x0 as i32 + cw as i32 * cell.width as i32
                            || y < 0
                            || x < 0
                            || x >= w as i32
                            || y >= h as i32
                        {
                            continue;
                        }
                        pixels[y as usize * w + x as usize] = blend(bg, fg, a);
                    }
                }
            }
        }

        if self.search_active {
            let box_w = (500 * scale).min(w.saturating_sub(24 * scale));
            let box_h = 42 * scale;
            let x = w.saturating_sub(box_w + 16 * scale);
            let y = top + 12 * scale;
            fill_round_rect(
                pixels,
                w,
                h,
                x,
                y,
                box_w,
                box_h,
                8 * scale,
                self.theme.border,
            );
            fill_round_rect(
                pixels,
                w,
                h,
                x + scale,
                y + scale,
                box_w.saturating_sub(2 * scale),
                box_h.saturating_sub(2 * scale),
                7 * scale,
                self.theme.bg,
            );
            let status = if self.search_matches.is_empty() {
                "0 matches".to_owned()
            } else {
                format!(
                    "{}/{}",
                    self.search_match_index + 1,
                    self.search_matches.len()
                )
            };
            let available = (box_w / cw.max(1)).saturating_sub(25);
            let query: String = self
                .search_query
                .chars()
                .rev()
                .take(available)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let label = format!("Find: {query}  {status}  Enter / Esc");
            draw_text(
                pixels,
                w,
                h,
                x + 12 * scale,
                y + (box_h.saturating_sub(ch)) / 2,
                &label,
                px * 0.82,
                ascent,
                self.theme.fg,
                font,
                glyphs,
            );
        }

        let status_y = h.saturating_sub(self.status_height + self.border);
        fill_rect(
            pixels,
            w,
            h,
            0,
            status_y,
            w,
            self.status_height,
            blend(self.theme.bg, self.theme.border, 28),
        );
        fill_rect(
            pixels,
            w,
            h,
            0,
            status_y,
            w,
            scale,
            blend(self.theme.bg, self.theme.border, 100),
        );
        let cwd = term
            .cwd
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("~");
        let status_left: String = cwd.chars().take(w / cw.max(1) / 3).collect();
        let status_right = format!(
            "{} × {}   ·   {}",
            term.cols,
            term.rows,
            self.theme.kind.name()
        );
        draw_text(
            pixels,
            w,
            h,
            16 * scale,
            status_y + (self.status_height.saturating_sub(ch)) / 2,
            &status_left,
            px * 0.72,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );
        let status_width = status_right.chars().count() * (cw * 3 / 4);
        draw_text(
            pixels,
            w,
            h,
            w.saturating_sub(status_width + 16 * scale),
            status_y + (self.status_height.saturating_sub(ch)) / 2,
            &status_right,
            px * 0.72,
            ascent,
            self.theme.fg,
            font,
            glyphs,
        );

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

fn fill_round_rect(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    radius: usize,
    color: u32,
) {
    let radius = radius.min(h / 2).min(w / 2);
    for row in 0..h {
        let dy = if row < radius {
            radius - row
        } else if row >= h - radius {
            row - (h - radius - 1)
        } else {
            0
        };
        let inset = if dy == 0 {
            0
        } else {
            radius.saturating_sub(
                radius
                    .saturating_mul(radius)
                    .saturating_sub(dy * dy)
                    .isqrt(),
            )
        };
        fill_rect(
            pixels,
            width,
            height,
            x + inset,
            y + row,
            w.saturating_sub(2 * inset),
            1,
            color,
        );
    }
}

fn draw_kde_button(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    scale: usize,
    background: u32,
    border: u32,
    accent: u32,
    hovered: bool,
) {
    let outline = if hovered {
        accent
    } else {
        blend(background, border, 90)
    };
    fill_round_rect(pixels, width, height, x, y, w, h, 7 * scale, outline);
    let face = if hovered {
        blend(background, accent, 52)
    } else {
        blend(background, border, 38)
    };
    fill_round_rect(
        pixels,
        width,
        height,
        x + scale,
        y + scale,
        w.saturating_sub(2 * scale),
        h.saturating_sub(2 * scale),
        6 * scale,
        face,
    );
}

fn draw_text(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    text: &str,
    px: f32,
    ascent: i32,
    color: u32,
    font: &Font,
    glyphs: &mut HashMap<(char, u32), (Metrics, Vec<u8>)>,
) {
    let mut cursor_x = x as i32;
    for ch in text.chars() {
        let (metrics, bitmap) = glyphs
            .entry(glyph_cache_key(ch, px))
            .or_insert_with(|| font.rasterize(ch, px));
        let gx0 = cursor_x + metrics.xmin;
        let gy0 = y as i32 + ascent - metrics.ymin - metrics.height as i32;
        for gy in 0..metrics.height {
            for gx in 0..metrics.width {
                let alpha = bitmap[gy * metrics.width + gx] as u32;
                let px_x = gx0 + gx as i32;
                let px_y = gy0 + gy as i32;
                if alpha == 0
                    || px_x < 0
                    || px_y < 0
                    || px_x >= width as i32
                    || px_y >= height as i32
                {
                    continue;
                }
                let index = px_y as usize * width + px_x as usize;
                pixels[index] = blend(pixels[index], color, alpha);
            }
        }
        cursor_x += metrics.advance_width.ceil() as i32;
        if cursor_x >= width as i32 {
            break;
        }
    }
}

fn clipboard_write_with(program: &str, args: &[&str], text: &str) -> std::io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{program} exited with {status}"
        )))
    }
}

fn clipboard_read_with(program: &str, args: &[&str]) -> std::io::Result<String> {
    let output = Command::new(program).args(args).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(std::io::Error::other(format!(
            "{program} exited with {}",
            output.status
        )))
    }
}

#[cfg(target_os = "macos")]
fn open_external(target: &str) -> std::io::Result<()> {
    Command::new("open").arg(target).spawn().map(|_| ())
}

#[cfg(not(target_os = "macos"))]
fn open_external(target: &str) -> std::io::Result<()> {
    Command::new("xdg-open").arg(target).spawn().map(|_| ())
}

#[cfg(target_os = "macos")]
fn clipboard_write(text: &str) -> std::io::Result<()> {
    clipboard_write_with("pbcopy", &[], text)
}

#[cfg(target_os = "macos")]
fn clipboard_read() -> std::io::Result<String> {
    clipboard_read_with("pbpaste", &[])
}

#[cfg(target_os = "linux")]
fn clipboard_write(text: &str) -> std::io::Result<()> {
    if env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Ok(()) = clipboard_write_with("wl-copy", &[], text) {
            return Ok(());
        }
    }
    clipboard_write_with("xclip", &["-selection", "clipboard"], text)
        .or_else(|_| clipboard_write_with("xsel", &["--clipboard", "--input"], text))
}

#[cfg(target_os = "linux")]
fn clipboard_read() -> std::io::Result<String> {
    if env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Ok(text) = clipboard_read_with("wl-paste", &["--no-newline"]) {
            return Ok(text);
        }
    }
    clipboard_read_with("xclip", &["-selection", "clipboard", "-o"])
        .or_else(|_| clipboard_read_with("xsel", &["--clipboard", "--output"]))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clipboard_write(_: &str) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "clipboard support is unavailable on this platform",
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn clipboard_read() -> std::io::Result<String> {
    Err(std::io::Error::other(
        "clipboard support is unavailable on this platform",
    ))
}

fn blend(bg: u32, fg: u32, a: u32) -> u32 {
    let mix = |shift: u32| {
        let f = (fg >> shift) & 0xff;
        let b = (bg >> shift) & 0xff;
        ((f * a + b * (255 - a)) / 255) << shift
    };
    mix(16) | mix(8) | mix(0)
}

fn glyph_cache_key(ch: char, px: f32) -> (char, u32) {
    (ch, px.to_bits())
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
        self.border = scale as usize;
        self.titlebar_height = if cfg!(target_os = "macos") {
            (44.0 * scale) as usize
        } else {
            0
        };
        self.tab_height = (if cfg!(target_os = "macos") {
            40.0
        } else {
            44.0
        } * scale) as usize;
        self.top = self.titlebar_height
            + if cfg!(target_os = "macos") {
                0
            } else {
                self.tab_height
            }
            + self.margin;
        self.status_height = (26.0 * scale) as usize;

        let lm = self
            .font
            .horizontal_line_metrics(self.px)
            .expect("font line metrics");
        self.ascent = lm.ascent.round() as i32;
        self.ch = lm.new_line_size.ceil() as usize;
        self.cw = self.font.metrics('M', self.px).advance_width.ceil() as usize;

        let size = PhysicalSize::new(
            (self.cw * self.active_term().cols + 2 * self.margin) as u32,
            (self.ch * self.active_term().rows + self.top + self.margin + self.status_height)
                as u32,
        );
        let _ = window.request_inner_size(size);

        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface =
            softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        self.surface = Some(surface);
        self._context = Some(context);
        self.window = Some(window);
        self.update_window_title();

        let s = self.window.as_ref().unwrap().inner_size();
        self.resize(s.width, s.height);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self
            .settings_window
            .as_ref()
            .is_some_and(|window| window.id() == id)
        {
            self.settings_window_event(event);
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.refresh_directories();
                self.persist_sessions();
                event_loop.exit();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.ctrl = m.state().control_key();
                self.shift = m.state().shift_key();
                self.super_key = m.state().super_key();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = self.cursor_pos;
                let scroll_up = match &delta {
                    MouseScrollDelta::LineDelta(_, n) => *n > 0.0,
                    MouseScrollDelta::PixelDelta(p) => p.y > 0.0,
                };
                if y >= self.tab_y() as f64 && y < self.top as f64 {
                    let layout = self.tab_layout();
                    if layout.first_visible > 0 && scroll_up {
                        self.tab_scroll -= 1;
                    } else if layout.first_visible + layout.visible_count < self.sessions.len()
                        && !scroll_up
                    {
                        self.tab_scroll += 1;
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                if self.send_mouse_report(if scroll_up { 64 } else { 65 }, x, y, false, false) {
                    return;
                }
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.round() as isize,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 24.0).round() as isize,
                };
                self.active_session_mut().term.scroll_view(lines);
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let search_shortcut = (self.super_key
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("f")))
                    || (self.ctrl
                        && self.shift
                        && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("f")));
                if search_shortcut {
                    self.begin_search();
                    return;
                }
                if self.search_active {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => self.end_search(),
                        Key::Named(NamedKey::Enter) => {
                            self.search(if self.shift { -1 } else { 1 }, true)
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.search_query.pop();
                            self.search(1, false);
                        }
                        Key::Character(value) if !self.ctrl && !self.super_key => {
                            if self.search_query.chars().count() < 128 {
                                self.search_query.push_str(value);
                                self.search(1, false);
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if self.ctrl
                    && self.shift
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("t"))
                {
                    self.cycle_theme();
                    return;
                }
                if self.super_key
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("t"))
                    || self.ctrl
                        && self.shift
                        && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("n"))
                {
                    self.add_tab();
                    return;
                }
                let copy_shortcut = self.super_key
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("c"))
                    || self.ctrl
                        && self.shift
                        && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("c"));
                if copy_shortcut {
                    if self.selection.is_some() {
                        self.copy_selection();
                    }
                    return;
                }
                let paste_shortcut = self.super_key
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("v"))
                    || self.ctrl
                        && self.shift
                        && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("v"));
                if paste_shortcut {
                    self.paste_clipboard();
                    return;
                }
                if matches!(&event.logical_key, Key::Named(NamedKey::Tab)) && self.ctrl {
                    self.switch_tab(if self.shift { -1 } else { 1 });
                    return;
                }
                if (self.super_key || self.ctrl) && !self.shift {
                    if let Key::Character(value) = &event.logical_key {
                        if value.len() == 1 {
                            if let Some(index) = value.chars().next().and_then(|ch| ch.to_digit(10))
                            {
                                if (1..=9).contains(&index)
                                    && (index as usize) <= self.sessions.len()
                                {
                                    self.activate_tab(index as usize - 1);
                                    return;
                                }
                            }
                        }
                    }
                }
                if (self.super_key || (self.ctrl && self.shift))
                    && matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("w"))
                {
                    self.close_active_tab(event_loop);
                    return;
                }
                if let Some(bytes) = self.key_bytes(&event.logical_key) {
                    let session = self.active_session_mut();
                    let _ = session.writer.write_all(&bytes);
                    let _ = session.writer.flush();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let previous = self.cursor_pos;
                self.cursor_pos = (position.x, position.y);
                if self.selecting {
                    self.extend_selection(position.x, position.y);
                } else if (self.active_term().mouse_tracking == 1003 || self.mouse_button_down)
                    && self.send_mouse_report(0, position.x, position.y, false, true)
                {
                    // Motion reporting is enabled by the running terminal application.
                }
                if previous.1 < self.top as f64 || position.y < self.top as f64 {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.cursor_pos;
                self.mouse_button_down = state == ElementState::Pressed;
                let tab_y = self.tab_y() as f64;
                if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && y < self.top as f64
                {
                    let layout = self.tab_layout();
                    let in_tabbar = y >= tab_y && y < self.top as f64;
                    if in_tabbar
                        && x >= layout.settings_x as f64
                        && x < (layout.settings_x + layout.settings_w) as f64
                    {
                        self.open_settings_window(event_loop);
                    } else if in_tabbar
                        && x >= layout.plus_x as f64
                        && x < (layout.plus_x + layout.plus_w) as f64
                    {
                        self.add_tab();
                    } else if in_tabbar {
                        if x >= layout.left_arrow_x as f64
                            && x < (layout.left_arrow_x + layout.arrow_w) as f64
                            && layout.first_visible > 0
                        {
                            self.tab_scroll = self.tab_scroll.saturating_sub(1);
                        } else if x >= layout.right_arrow_x as f64
                            && x < (layout.right_arrow_x + layout.arrow_w) as f64
                            && layout.first_visible + layout.visible_count < self.sessions.len()
                        {
                            self.tab_scroll += 1;
                        }
                        for local_index in 0..layout.visible_count {
                            let index = layout.first_visible + local_index;
                            if index >= self.sessions.len() {
                                break;
                            }
                            let left = layout.start + local_index * (layout.width + layout.gap);
                            if x >= left as f64 && x < (left + layout.width) as f64 {
                                if self.sessions.len() > 1
                                    && x >= (left + layout.width.saturating_sub(28 * layout.scale))
                                        as f64
                                {
                                    self.close_tab(index, event_loop);
                                } else {
                                    self.activate_tab(index);
                                }
                                break;
                            }
                        }
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                } else if state == ElementState::Pressed
                    && button == MouseButton::Left
                    && y < self.titlebar_height as f64
                {
                    if let Some(window) = &self.window {
                        let _ = window.drag_window();
                    }
                } else {
                    let code = match button {
                        MouseButton::Left => 0,
                        MouseButton::Middle => 1,
                        MouseButton::Right => 2,
                        _ => 0,
                    };
                    let handled =
                        self.send_mouse_report(code, x, y, state == ElementState::Released, false);
                    if state == ElementState::Pressed && button == MouseButton::Left && !handled {
                        let link_click = (self.ctrl || self.super_key)
                            && self.active_term().mouse_tracking == 0
                            && self.open_or_copy_token(x, y);
                        if !link_click {
                            self.begin_selection(x, y);
                        }
                    }
                    if state == ElementState::Released {
                        self.selecting = false;
                    }
                }
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Output(id, bytes) => {
                // Pass each PTY chunk to vte in one call instead of dispatching per byte.
                let mut active_output = false;
                let mut became_active = false;
                let mut cwd_changed = false;
                let active_id = self.sessions.get(self.active).map(|session| session.id);
                if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                    let previous_bell = session.term.bell;
                    let previous_cwd = session.term.cwd.clone();
                    session.parser.advance(&mut session.term, &bytes);
                    cwd_changed = session.term.cwd != previous_cwd;
                    became_active |= session.term.bell != previous_bell;
                    for response in std::mem::take(&mut session.term.responses) {
                        let _ = session.writer.write_all(&response);
                    }
                    let _ = session.writer.flush();
                    if Some(id) == active_id {
                        active_output = true;
                        session.activity = false;
                    } else if !session.activity {
                        session.activity = true;
                        became_active = true;
                    }
                }
                if cwd_changed {
                    self.persist_sessions();
                }
                if active_output {
                    if self.search_active {
                        self.search(1, false);
                    }
                    self.update_window_title();
                }
                if active_output || became_active {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                }
            }
            UserEvent::Closed(id) => {
                if let Some(index) = self.sessions.iter().position(|s| s.id == id) {
                    self.close_tab(index, event_loop);
                }
            }
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

    let theme = Theme::load();
    let mut sessions = Vec::new();
    for (index, cwd) in restored_session_dirs().into_iter().enumerate() {
        sessions.push(spawn_session(
            index as u64 + 1,
            100,
            30,
            theme,
            proxy.clone(),
            cwd,
        )?);
    }
    let next_session_id = sessions.len() as u64 + 1;
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
        titlebar_height: 0,
        tab_height: 46,
        status_height: 26,
        border: 1,
        sessions,
        active: 0,
        tab_scroll: 0,
        next_session_id,
        proxy,
        window: None,
        reported_title: String::new(),
        _context: None,
        surface: None,
        settings_window: None,
        _settings_context: None,
        settings_surface: None,
        settings_width: 0,
        settings_height: 0,
        settings_cursor_pos: (0.0, 0.0),
        settings_editor: SettingsEditor::load(),
        width: 0,
        height: 0,
        ctrl: false,
        shift: false,
        super_key: false,
        cursor_pos: (0.0, 0.0),
        mouse_button_down: false,
        selection: None,
        selecting: false,
        search_active: false,
        search_query: String::new(),
        search_matches: Vec::new(),
        search_match_index: 0,
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{glyph_cache_key, tab_layout};

    #[test]
    fn search_handles_wide_cells_and_long_queries() {
        let mut term = super::Term::new(
            20,
            2,
            super::Theme::from_kind(super::ThemeKind::Poop),
            "/tmp".into(),
        );
        let mut parser = super::Parser::new();
        parser.advance(&mut term, "猫 hello HELLO".as_bytes());
        let found = super::find_matches(&term, "hello");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].start.col, 3);
        assert_eq!(found[0].end.col, 7);
        assert!(super::find_matches(&term, &"x".repeat(100)).is_empty());
        assert!(super::find_matches(&term, "").is_empty());
    }

    #[test]
    fn session_paths_preserve_punctuation() {
        let path = "/tmp/a,b#c \"quoted\"/猫";
        let encoded = path
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(super::decode_session_path(&encoded), Some(path.into()));
        assert!(super::decode_session_path("xyz").is_none());
    }

    #[test]
    fn osc_directory_and_bell_are_tracked() {
        let mut term = super::Term::new(
            20,
            2,
            super::Theme::from_kind(super::ThemeKind::Poop),
            "/tmp".into(),
        );
        let mut parser = super::Parser::new();
        parser.advance(&mut term, b"\x1b]7;file://localhost/tmp/my%20dir\x07\x07");
        assert_eq!(term.cwd, std::path::PathBuf::from("/tmp/my dir"));
        assert!(term.bell);
    }

    #[test]
    fn glyphs_are_cached_separately_at_each_font_size() {
        assert_ne!(glyph_cache_key('a', 12.0), glyph_cache_key('a', 18.0));
        assert_eq!(glyph_cache_key('a', 12.0), glyph_cache_key('a', 12.0));
    }

    #[test]
    fn tab_strip_can_reach_older_tabs() {
        let first_page = tab_layout(980, 20, 1, 0, 0);
        let last_page = tab_layout(980, 20, 1, 0, 19);

        assert!(first_page.visible_count < 20);
        assert_eq!(first_page.first_visible, 0);
        assert!(last_page.first_visible > 0);
        assert!(last_page.first_visible + last_page.visible_count <= 20);
    }

    #[test]
    fn narrow_tab_strip_keeps_a_reachable_tab() {
        let layout = tab_layout(450, 20, 1, 0, 19);

        assert!(layout.visible_count >= 1);
        assert!(layout.first_visible < 20);
        assert!(layout.first_visible + layout.visible_count <= 20);
    }
}
