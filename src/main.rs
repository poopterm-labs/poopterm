#![feature(portable_simd)]

mod config;
mod osc;
mod parser;
use osc::decode_uri_path;
use parser::Parser;
mod ansi;
mod sixel;
use ansi::{Cell, Term};
use config::Preferences;

use std::{
	collections::HashMap,
	env,
	error::Error,
	fs,
	io::{Read, Write},
	num::NonZeroU32,
	path::PathBuf,
	process::{Command, Stdio},
	sync::{Arc, mpsc},
	thread,
};

use fontdue::{Font, FontSettings, Metrics};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::simd::u32x4;
use winit::{
	application::ApplicationHandler,
	dpi::{LogicalSize, PhysicalSize},
	event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
	event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
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
	Custom,
}

impl ThemeKind {
	fn parse(value: &str) -> Option<Self> {
		match value.trim().trim_matches('"').to_ascii_lowercase().as_str() {
			"darkmode" | "macos" | "mac" | "macos-dark" => Some(Self::MacOsDark),
			"light-mode" | "light mode" | "macos-light" | "light" => Some(Self::MacOsLight),
			"catppuccin" | "catppuccin-mocha" => Some(Self::CatppuccinMocha),
			"catppuccin-latte" => Some(Self::CatppuccinLatte),
			"catppuccin-frappe" => Some(Self::CatppuccinFrappe),
			"catppuccin-macchiato" => Some(Self::CatppuccinMacchiato),
			"poop" => Some(Self::Poop),
			"custom" => Some(Self::Custom),
			_ => None,
		}
	}
	fn name(self) -> &'static str {
		match self {
			Self::MacOsDark => "darkmode",
			Self::MacOsLight => "light-mode",
			Self::CatppuccinLatte => "catppuccin-latte",
			Self::CatppuccinFrappe => "catppuccin-frappe",
			Self::CatppuccinMacchiato => "catppuccin-macchiato",
			Self::CatppuccinMocha => "catppuccin-mocha",
			Self::Poop => "poop",
			Self::Custom => "custom",
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
			Self::Custom => Self::MacOsDark,
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
			ThemeKind::Custom => {
				let mut theme = Self::from_kind(ThemeKind::Poop);
				theme.kind = ThemeKind::Custom;
				theme
			}
		}
	}

	fn load() -> Self {
		let kind = env::var("POOPTERM_THEME")
			.ok()
			.and_then(|s| ThemeKind::parse(&s))
			.or_else(read_config_theme)
			.unwrap_or(ThemeKind::Poop);
		let mut theme = Self::from_kind(kind);
		for (key, target) in [
			("foreground", &mut theme.fg),
			("background", &mut theme.bg),
			("border", &mut theme.border),
		] {
			if let Some(color) = read_config_value(key).and_then(|value| parse_color(&value)) {
				*target = color;
			}
		}
		for (index, color) in theme.palette.iter_mut().enumerate() {
			if let Some(value) =
				read_config_value(&format!("color{index}")).and_then(|value| parse_color(&value))
			{
				*color = value;
			}
		}
		theme
	}
}

fn parse_color(value: &str) -> Option<u32> {
	let value = value.trim().trim_matches('"');
	let value = value
		.strip_prefix('#')
		.or_else(|| value.strip_prefix("0x"))?;
	(value.len() == 6)
		.then(|| u32::from_str_radix(value, 16).ok())
		.flatten()
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
	if !line.len().is_multiple_of(2) {
		return None;
	}
	let bytes = line
		.as_bytes()
		.as_chunks::<2>()
		.0
		.iter()
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
			dir
				.to_string_lossy()
				.as_bytes()
				.iter()
				.map(|b| format!("{b:02x}"))
				.collect::<String>()
		})
		.collect::<Vec<_>>()
		.join("\n");
	let temp = path.with_extension(format!("{}.tmp", std::process::id()));
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
	if !Preferences::load().restore_sessions {
		return vec![default_cwd()];
	}
	let dirs: Vec<_> = read_session_dirs()
		.into_iter()
		.map(|dir| if dir.is_dir() { dir } else { default_cwd() })
		.collect();
	if dirs.is_empty() {
		vec![default_cwd()]
	} else {
		dirs
	}
}

fn read_config_value(key: &str) -> Option<String> {
	config::value(&fs::read_to_string(config_path()).ok()?, key)
}

fn load_fonts() -> (Font, Option<Font>) {
	let bundled = Font::from_bytes(
		include_bytes!("../assets/font.ttf") as &[u8],
		FontSettings::default(),
	)
	.expect("bundled font is valid");
	let configured = env::var("POOPTERM_FONT")
		.ok()
		.or_else(|| read_config_value("font_path"));
	let Some(path) = configured else {
		return (bundled, None);
	};
	let path = path
		.strip_prefix("~/")
		.map_or(PathBuf::from(&path), |tail| default_cwd().join(tail));
	match fs::read(&path)
		.map_err(|error| error.to_string())
		.and_then(|bytes| {
			Font::from_bytes(bytes, FontSettings::default()).map_err(|error| error.to_string())
		}) {
		Ok(font) => (font, Some(bundled)),
		Err(error) => {
			eprintln!(
				"poopterm: couldn't load font {}: {error}; using bundled font",
				path.display()
			);
			(bundled, None)
		}
	}
}

fn save_config_theme(kind: ThemeKind) -> std::io::Result<()> {
	config::save(&config_path(), &[("theme", format!("\"{}\"", kind.name()))])
}

fn dec_graphic(c: char) -> char {
	match c {
		'`' => '◆',
		'a' => '▒',
		'f' => '°',
		'g' => '±',
		'j' => '┘',
		'k' => '┐',
		'l' => '┌',
		'm' => '└',
		'n' => '┼',
		'o' => '⎺',
		'p' => '⎻',
		'q' => '─',
		'r' => '⎼',
		's' => '⎽',
		't' => '├',
		'u' => '┤',
		'v' => '┴',
		'w' => '┬',
		'x' => '│',
		'y' => '≤',
		'z' => '≥',
		'{' => 'π',
		'|' => '≠',
		'}' => '£',
		'~' => '·',
		_ => c,
	}
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

// ---- Events from the PTY reader thread --------------------------------------

#[derive(Debug)]
enum UserEvent {
	Output(u64, Vec<u8>, mpsc::SyncSender<Vec<u8>>),
	Closed(u64),
}

struct Session {
	id: u64,
	term: Term,
	parser: Parser,
	custom_title: Option<String>,
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
	preferences: Preferences,
	status: String,
	about: bool,
}

impl SettingsEditor {
	fn load() -> Self {
		Self {
			theme: Theme::load().kind,
			preferences: Preferences::load(),
			status: "Change preferences, then click Save.".into(),
			about: false,
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
		let mut buf = vec![0u8; 16384];
		loop {
			match reader.read(&mut buf) {
				Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
				Ok(0) | Err(_) => {
					let _ = proxy.send_event(UserEvent::Closed(id));
					break;
				}
				Ok(n) => {
					buf.truncate(n);
					let (ack, received) = mpsc::sync_channel(1);
					if proxy.send_event(UserEvent::Output(id, buf, ack)).is_err() {
						break;
					}
					let Ok(mut returned) = received.recv() else {
						break;
					};
					returned.resize(16384, 0);
					buf = returned;
				}
			}
		}
	});

	let mut term = Term::new(cols, rows, theme, cwd);
	term.scrollback_limit = Preferences::load().scrollback_lines;
	Ok(Session {
		id,
		term,
		parser: Parser::new(),
		custom_title: None,
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
	fallback_font: Option<Font>,
	glyphs: HashMap<(char, u32, bool), (Metrics, Vec<u8>)>,
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
	alt: bool,
	super_key: bool,
	cursor_pos: (f64, f64),
	mouse_button_down: bool,
	selection: Option<Selection>,
	selecting: bool,
	search_active: bool,
	search_dirty: bool,
	search_query: String,
	search_matches: Vec<Selection>,
	search_match_index: usize,
	command_palette: bool,
	command_query: String,
	renaming_tab: bool,
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
					if matches.len() >= 10_000 {
						return matches;
					}
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
		self.search_dirty = false;
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
			self
				.window
				.as_ref()
				.map_or(1.0, |w| w.scale_factor())
				.round()
				.max(1.0) as usize,
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
			.with_inner_size(LogicalSize::new(760.0, 560.0))
			.with_min_inner_size(LogicalSize::new(680.0, 520.0));
		let window = Arc::new(
			event_loop
				.create_window(attributes)
				.expect("failed to create settings window"),
		);
		let context = softbuffer::Context::new(window.clone()).expect("settings context");
		let mut surface = softbuffer::Surface::new(&context, window.clone()).expect("settings surface");
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
		let preferences = self.settings_editor.preferences;
		let result = config::save(
			&config_path(),
			&[
				(
					"theme",
					format!("\"{}\"", self.settings_editor.theme.name()),
				),
				("font_size", preferences.font_size.to_string()),
				("scrollback_lines", preferences.scrollback_lines.to_string()),
				("restore_sessions", preferences.restore_sessions.to_string()),
				("opacity", preferences.opacity.to_string()),
			],
		);
		match result {
			Ok(()) => {
				self.theme = Theme::load();
				#[cfg(target_os = "macos")]
				if let Some(window) = &self.window {
					set_window_opacity(window, preferences.opacity);
				}
				for session in &mut self.sessions {
					session.term.set_theme(self.theme);
					session.term.scrollback_limit = preferences.scrollback_lines;
					while session.term.scrollback.len() > preferences.scrollback_lines {
						session.term.scrollback.pop_front();
						session.term.history_base = session.term.history_base.saturating_add(1);
					}
					session.term.scroll_offset = session
						.term
						.scroll_offset
						.min(session.term.scrollback.len());
				}
				let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor()) as f32;
				self.px = preferences.font_size as f32 * scale;
				if let Some(metrics) = self.font.horizontal_line_metrics(self.px) {
					self.ascent = metrics.ascent.round() as i32;
					self.ch = metrics.new_line_size.ceil() as usize;
				}
				self.cw = self.font.metrics('M', self.px).advance_width.ceil() as usize;
				self.selection = None;
				self.end_search();
				self.resize(self.width, self.height);
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
		if self.settings_editor.about {
			draw_text(
				pixels,
				width,
				height,
				x,
				28 * scale,
				"About poopterm",
				px * 1.3,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
			for (line, text) in [
				(104, concat!("Version ", env!("CARGO_PKG_VERSION"))),
				(148, "A small, native terminal emulator written in Rust."),
				(184, "Copyright © 2026 poopware-labs"),
				(220, "Licensed under the MIT License."),
				(256, "Source: github.com/poopterm-labs/poopterm"),
				(292, "Built with Rust, winit, softbuffer, fontdue, and vte."),
			] {
				draw_text(
					pixels,
					width,
					height,
					x,
					line * scale,
					text,
					px * 0.82,
					ascent,
					self.theme.fg,
					font,
					glyphs,
				);
			}
			let back_x = width.saturating_sub(164 * scale);
			draw_kde_button(
				pixels,
				width,
				height,
				back_x,
				20 * scale,
				140 * scale,
				36 * scale,
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
				back_x + 22 * scale,
				27 * scale,
				"Settings",
				px * 0.8,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
			let _ = buffer.present();
			return;
		}
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
		let about_x = width.saturating_sub(140 * scale);
		draw_kde_button(
			pixels,
			width,
			height,
			about_x,
			20 * scale,
			116 * scale,
			36 * scale,
			scale,
			self.theme.bg,
			self.theme.border,
			self.theme.palette[4],
			self.settings_cursor_pos.0 >= about_x as f64
				&& self.settings_cursor_pos.0 < (about_x + 116 * scale) as f64
				&& self.settings_cursor_pos.1 >= (20 * scale) as f64
				&& self.settings_cursor_pos.1 < (56 * scale) as f64,
		);
		draw_text(
			pixels,
			width,
			height,
			about_x + 25 * scale,
			27 * scale,
			"About",
			px * 0.8,
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
			ThemeKind::Custom,
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
		let theme_name = match self.settings_editor.theme {
			ThemeKind::MacOsDark => "Dark Mode".to_owned(),
			ThemeKind::MacOsLight => "Light Mode".to_owned(),
			ThemeKind::Custom => "Custom".to_owned(),
			_ => self
				.settings_editor
				.theme
				.name()
				.replace("catppuccin-", "Catppuccin "),
		};
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
			"Preferences are saved to your config file.",
			px * 0.72,
			ascent,
			self.theme.fg,
			font,
			glyphs,
		);
		for (row, label, value) in [
			(
				270,
				"Font size",
				self.settings_editor.preferences.font_size.to_string(),
			),
			(
				326,
				"Scrollback lines",
				self
					.settings_editor
					.preferences
					.scrollback_lines
					.to_string(),
			),
			(
				382,
				"Restore tabs on launch",
				if self.settings_editor.preferences.restore_sessions {
					"On".into()
				} else {
					"Off".into()
				},
			),
		] {
			draw_text(
				pixels,
				width,
				height,
				x,
				(row + 8) * scale,
				label,
				px * 0.82,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
			if row == 382 {
				let bx = width.saturating_sub(240 * scale);
				draw_kde_button(
					pixels,
					width,
					height,
					bx,
					row * scale,
					216 * scale,
					40 * scale,
					scale,
					self.theme.bg,
					self.theme.border,
					self.theme.palette[4],
					self.settings_editor.preferences.restore_sessions,
				);
				draw_text(
					pixels,
					width,
					height,
					bx + 16 * scale,
					(row + 8) * scale,
					if self.settings_editor.preferences.restore_sessions {
						"Enabled"
					} else {
						"Disabled"
					},
					px * 0.82,
					ascent,
					self.theme.fg,
					font,
					glyphs,
				);
				continue;
			}
			for (bx, text) in [
				(width.saturating_sub(240 * scale), "−"),
				(width.saturating_sub(72 * scale), "+"),
			] {
				draw_kde_button(
					pixels,
					width,
					height,
					bx,
					row * scale,
					48 * scale,
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
					bx + 16 * scale,
					(row + 7) * scale,
					text,
					px,
					ascent,
					self.theme.fg,
					font,
					glyphs,
				);
			}
			draw_text(
				pixels,
				width,
				height,
				width.saturating_sub(180 * scale),
				(row + 8) * scale,
				&value,
				px * 0.82,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
		}
		#[cfg(target_os = "macos")]
		{
			let row = 438;
			draw_text(
				pixels,
				width,
				height,
				x,
				(row + 8) * scale,
				"Window opacity",
				px * 0.82,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
			for (bx, text) in [
				(width.saturating_sub(240 * scale), "−"),
				(width.saturating_sub(72 * scale), "+"),
			] {
				draw_kde_button(
					pixels,
					width,
					height,
					bx,
					row * scale,
					48 * scale,
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
					bx + 16 * scale,
					(row + 7) * scale,
					text,
					px,
					ascent,
					self.theme.fg,
					font,
					glyphs,
				);
			}
			draw_text(
				pixels,
				width,
				height,
				width.saturating_sub(180 * scale),
				(row + 8) * scale,
				&format!("{}%", self.settings_editor.preferences.opacity),
				px * 0.82,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
		}
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
			save_y + 6 * scale,
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
				if let Some(surface) = self.settings_surface.as_mut()
					&& let (Some(width), Some(height)) = (
						NonZeroU32::new(self.settings_width),
						NonZeroU32::new(self.settings_height),
					) {
					let _ = surface.resize(width, height);
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
				let about_x = (self.settings_width as usize).saturating_sub(140 * scale);
				if (20 * scale..56 * scale).contains(&y)
					&& x >= about_x
					&& x
						< about_x
							+ if self.settings_editor.about {
								140 * scale
							} else {
								116 * scale
							} {
					self.settings_editor.about = !self.settings_editor.about;
					if let Some(window) = &self.settings_window {
						window.request_redraw();
					}
					return;
				}
				let save_x = (self.settings_width as usize).saturating_sub(140 * scale);
				let save_y = (self.settings_height as usize).saturating_sub(52 * scale);
				if x >= save_x && x < save_x + 120 * scale && y >= save_y && y < save_y + 36 * scale {
					self.save_settings_config();
					return;
				}
				let width = self.settings_width as usize;
				if (382 * scale..422 * scale).contains(&y)
					&& x >= width.saturating_sub(240 * scale)
					&& x < width.saturating_sub(24 * scale)
				{
					self.settings_editor.preferences.restore_sessions =
						!self.settings_editor.preferences.restore_sessions;
					if let Some(window) = &self.settings_window {
						window.request_redraw();
					}
					return;
				}
				let decrease =
					x >= width.saturating_sub(240 * scale) && x < width.saturating_sub(192 * scale);
				let increase =
					x >= width.saturating_sub(72 * scale) && x < width.saturating_sub(24 * scale);
				if decrease || increase {
					let prefs = &mut self.settings_editor.preferences;
					if (270 * scale..310 * scale).contains(&y) {
						prefs.font_size = if increase {
							(prefs.font_size + 1).min(32)
						} else {
							prefs.font_size.saturating_sub(1).max(10)
						};
					} else if (326 * scale..366 * scale).contains(&y) {
						prefs.scrollback_lines = if increase {
							(prefs.scrollback_lines + 1000).min(100_000)
						} else {
							prefs.scrollback_lines.saturating_sub(1000)
						};
					} else if (382 * scale..422 * scale).contains(&y) {
						prefs.restore_sessions = !prefs.restore_sessions;
					} else if cfg!(target_os = "macos") && (438 * scale..478 * scale).contains(&y) {
						prefs.opacity = if increase {
							(prefs.opacity + 5).min(100)
						} else {
							prefs.opacity.saturating_sub(5).max(60)
						};
					}
					if let Some(window) = &self.settings_window {
						window.request_redraw();
					}
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
						ThemeKind::Custom,
					];
					let index = themes
						.iter()
						.position(|theme| *theme == self.settings_editor.theme)
						.unwrap_or(0);
					if x < content_x + button_w {
						self.settings_editor.theme = themes[(index + themes.len() - 1) % themes.len()];
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
					if self.settings_editor.about {
						self.settings_editor.about = false;
						if let Some(window) = &self.settings_window {
							window.request_redraw();
						}
						return;
					}
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
					ThemeKind::Custom,
				];
				let index = themes
					.iter()
					.position(|theme| *theme == self.settings_editor.theme)
					.unwrap_or(0);
				match &event.logical_key {
					Key::Named(NamedKey::ArrowLeft) => {
						self.settings_editor.theme = themes[(index + themes.len() - 1) % themes.len()]
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
		self.refresh_directories(true);
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
		let mut session = self.sessions.remove(index);
		let _ = session._child.kill();
		thread::spawn(move || {
			let _ = session._child.wait();
		});
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
		let title = self.sessions[self.active]
			.custom_title
			.clone()
			.unwrap_or_else(|| self.active_term().title.clone());
		if self.reported_title == title {
			return;
		}
		self.reported_title.clone_from(&title);
		if let Some(window) = &self.window {
			window.set_title(&format!("{title} — poopterm"));
		}
	}

	fn refresh_directories(&mut self, only_active: bool) {
		for (index, session) in self.sessions.iter_mut().enumerate() {
			if only_active && index != self.active {
				continue;
			}
			if let Some(pid) = session._child.process_id() {
				#[cfg(target_os = "linux")]
				if let Ok(path) = fs::read_link(format!("/proc/{pid}/cwd")) {
					session.term.cwd = path;
				}
				#[cfg(target_os = "macos")]
				if let Ok(output) = Command::new("/usr/sbin/lsof")
					.args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
					.output()
					&& let Some(path) = String::from_utf8_lossy(&output.stdout)
						.lines()
						.find_map(|line| line.strip_prefix('n'))
				{
					session.term.cwd = PathBuf::from(path);
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
			|| x >= (self.margin + term.cols * self.cw) as f64
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
					result.extend(cell.combining.iter().take(cell.combining_len as usize));
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
		if let Some(text) = self.selected_text()
			&& let Err(error) = clipboard_write(&text)
		{
			eprintln!("poopterm: clipboard copy failed: {error}");
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
				.flat_map(|cell| {
					std::iter::once(cell.ch).chain(
						cell
							.combining
							.iter()
							.take(cell.combining_len as usize)
							.copied(),
					)
				})
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
		let term = self.active_term();
		let explicit = term
			.row_cells(point.row)
			.and_then(|cells| cells.get(point.col))
			.and_then(|cell| term.hyperlinks.get(cell.hyperlink as usize))
			.filter(|uri| !uri.is_empty());
		let token = explicit.map(String::as_str).unwrap_or(&token);
		if token.starts_with("https://") || token.starts_with("http://") {
			if let Err(error) = open_external(token) {
				eprintln!("poopterm: could not open link: {error}");
			}
			return true;
		}
		let file_path = token.strip_prefix("file://").and_then(|uri| {
			let path = if uri.starts_with('/') {
				Some(uri)
			} else {
				uri.strip_prefix("localhost")
			};
			path.and_then(decode_uri_path)
		});
		if let Some(path) = file_path {
			if let Err(error) = open_external(&path.to_string_lossy()) {
				eprintln!("poopterm: could not open linked file: {error}");
			}
			return true;
		}
		if token.starts_with('/')
			|| token.starts_with("~/")
			|| token.starts_with("./")
			|| token.starts_with("../")
			|| token.contains('/')
		{
			if let Err(error) = clipboard_write(token) {
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

	fn send_mouse_report(&mut self, button: u8, x: f64, y: f64, release: bool, motion: bool) -> bool {
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
		if let Err(error) = save_config_theme(kind) {
			eprintln!("poopterm: could not save theme config: {error}");
		}
		self.theme = Theme::load();
		for session in &mut self.sessions {
			session.term.set_theme(self.theme);
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
		let rows = ((self.height as usize).saturating_sub(self.top + self.margin + self.status_height)
			/ self.ch.max(1))
		.max(2);

		for session in &mut self.sessions {
			session.term.cell_width = self.cw;
			session.term.cell_height = self.ch;
			if let Some(pending) = &mut session.term.synchronized {
				pending.cell_width = self.cw;
				pending.cell_height = self.ch;
			}
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

		if let Some(s) = self.surface.as_mut()
			&& let (Some(nw), Some(nh)) = (NonZeroU32::new(self.width), NonZeroU32::new(self.height))
		{
			let _ = s.resize(nw, nh);
		}
		if let Some(win) = &self.window {
			win.request_redraw();
		}
	}

	fn shortcut_matches(&self, key: &Key, config_key: &str, default: &str) -> bool {
		let binding = read_config_value(config_key).unwrap_or_else(|| default.to_owned());
		let parts: Vec<_> = binding
			.to_ascii_lowercase()
			.split('+')
			.map(str::to_owned)
			.collect();
		let Some(name) = parts.last() else {
			return false;
		};
		let expected = |modifier: &str| parts.iter().any(|part| part == modifier);
		let key_matches = matches!(key, Key::Character(value) if value.eq_ignore_ascii_case(name));
		key_matches
			&& self.ctrl == expected("ctrl")
			&& self.shift == expected("shift")
			&& self.alt == expected("alt")
			&& self.super_key == expected("super")
	}

	fn key_bytes(&self, key: &Key) -> Option<Vec<u8>> {
		if self.active_term().kitty_keyboard {
			let modifiers = 1
				+ u8::from(self.shift)
				+ 2 * u8::from(self.alt)
				+ 4 * u8::from(self.ctrl)
				+ 8 * u8::from(self.super_key);
			let letter = match key {
				Key::Named(NamedKey::ArrowUp) => Some('A'),
				Key::Named(NamedKey::ArrowDown) => Some('B'),
				Key::Named(NamedKey::ArrowRight) => Some('C'),
				Key::Named(NamedKey::ArrowLeft) => Some('D'),
				Key::Named(NamedKey::Home) => Some('H'),
				Key::Named(NamedKey::End) => Some('F'),
				_ => None,
			};
			if let Some(letter) = letter {
				if modifiers > 1 {
					return Some(format!("\x1b[1;{modifiers}{letter}").into_bytes());
				}
				return Some(match key {
					Key::Named(NamedKey::ArrowUp) if self.active_term().application_cursor => {
						b"\x1bOA".to_vec()
					}
					Key::Named(NamedKey::ArrowDown) if self.active_term().application_cursor => {
						b"\x1bOB".to_vec()
					}
					Key::Named(NamedKey::ArrowRight) if self.active_term().application_cursor => {
						b"\x1bOC".to_vec()
					}
					Key::Named(NamedKey::ArrowLeft) if self.active_term().application_cursor => {
						b"\x1bOD".to_vec()
					}
					_ => format!("\x1b[{letter}").into_bytes(),
				});
			}
			match key {
				Key::Named(NamedKey::Enter) => return Some(b"\r".to_vec()),
				Key::Named(NamedKey::Backspace) => return Some(vec![0x7f]),
				Key::Named(NamedKey::Tab) if self.shift => return Some(b"\x1b[Z".to_vec()),
				Key::Named(NamedKey::Tab) => return Some(b"\t".to_vec()),
				Key::Named(NamedKey::Escape) if modifiers > 1 => {
					return Some(format!("\x1b[27;{modifiers}u").into_bytes());
				}
				Key::Named(NamedKey::Escape) => return Some(vec![0x1b]),
				Key::Named(NamedKey::Insert) => {
					return Some(
						format!(
							"\x1b[2{}~",
							if modifiers > 1 {
								format!(";{modifiers}")
							} else {
								String::new()
							}
						)
						.into_bytes(),
					);
				}
				Key::Named(NamedKey::Delete) => {
					return Some(
						format!(
							"\x1b[3{}~",
							if modifiers > 1 {
								format!(";{modifiers}")
							} else {
								String::new()
							}
						)
						.into_bytes(),
					);
				}
				Key::Named(NamedKey::PageUp) => {
					return Some(
						format!(
							"\x1b[5{}~",
							if modifiers > 1 {
								format!(";{modifiers}")
							} else {
								String::new()
							}
						)
						.into_bytes(),
					);
				}
				Key::Named(NamedKey::PageDown) => {
					return Some(
						format!(
							"\x1b[6{}~",
							if modifiers > 1 {
								format!(";{modifiers}")
							} else {
								String::new()
							}
						)
						.into_bytes(),
					);
				}
				Key::Named(n @ (NamedKey::F1 | NamedKey::F2 | NamedKey::F3 | NamedKey::F4)) => {
					let suffix = match n {
						NamedKey::F1 => 'P',
						NamedKey::F2 => 'Q',
						NamedKey::F3 => 'R',
						_ => 'S',
					};
					return Some(if modifiers > 1 {
						format!("\x1b[1;{modifiers}{suffix}").into_bytes()
					} else {
						format!("\x1bO{suffix}").into_bytes()
					});
				}
				Key::Named(
					n @ (NamedKey::F5
					| NamedKey::F6
					| NamedKey::F7
					| NamedKey::F8
					| NamedKey::F9
					| NamedKey::F10
					| NamedKey::F11
					| NamedKey::F12),
				) => {
					let number = match n {
						NamedKey::F5 => 15,
						NamedKey::F6 => 17,
						NamedKey::F7 => 18,
						NamedKey::F8 => 19,
						NamedKey::F9 => 20,
						NamedKey::F10 => 21,
						NamedKey::F11 => 23,
						_ => 24,
					};
					return Some(
						format!(
							"\x1b[{number}{}~",
							if modifiers > 1 {
								format!(";{modifiers}")
							} else {
								String::new()
							}
						)
						.into_bytes(),
					);
				}
				Key::Character(text) if self.ctrl || self.alt || self.super_key => {
					if let Some(ch) = text.chars().next() {
						return Some(
							format!("\x1b[{};{modifiers}u", ch.to_ascii_lowercase() as u32).into_bytes(),
						);
					}
				}
				_ => {}
			}
		}
		match key {
			Key::Named(n) => match n {
				NamedKey::Enter => Some(b"\r".to_vec()),
				NamedKey::Backspace => Some(vec![0x7f]),
				NamedKey::Tab => Some(if self.shift {
					b"\x1b[Z".to_vec()
				} else {
					b"\t".to_vec()
				}),
				NamedKey::Escape => Some(vec![0x1b]),
				NamedKey::Space => Some(vec![if self.ctrl { 0 } else { b' ' }]),
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
					return match c {
						b' ' | b'@' | b'2' => Some(vec![0]),
						b'[' | b'3' => Some(vec![27]),
						b'\\' | b'4' => Some(vec![28]),
						b']' | b'5' => Some(vec![29]),
						b'^' | b'6' => Some(vec![30]),
						b'_' | b'/' | b'7' => Some(vec![31]),
						b'?' | b'8' => Some(vec![127]),
						_ => None,
					};
				}
				Some(s.as_bytes().to_vec())
			}
			_ => None,
		}
	}

	fn draw(&mut self) {
		if self.search_active && self.search_dirty {
			let previous = self
				.search_matches
				.get(self.search_match_index)
				.map(|m| m.start);
			self.search_matches = find_matches(self.active_term(), &self.search_query);
			self.search_match_index = previous
				.and_then(|point| self.search_matches.iter().position(|m| m.start == point))
				.unwrap_or(0);
			self.search_dirty = false;
		}
		if self.glyphs.len() > 4096 {
			self.glyphs.clear();
		}
		let tab_y = self.tab_y();
		let scale = self
			.window
			.as_ref()
			.map_or(1.0, |w| w.scale_factor())
			.round()
			.max(1.0) as usize;
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
		let font = &self.font;
		let fallback_font = self.fallback_font.as_ref();
		let glyphs = &mut self.glyphs;

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

		let (m, top) = (self.margin, self.top);
		let ui_px = 18.0 * scale as f32;
		let ui_metrics = self
			.font
			.horizontal_line_metrics(ui_px)
			.expect("font metrics");
		let (cw, ch, px, ascent) = (
			self.font.metrics('M', ui_px).advance_width.ceil() as usize,
			ui_metrics.new_line_size.ceil() as usize,
			ui_px,
			ui_metrics.ascent.round() as i32,
		);
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
			let title_chars = layout.width.saturating_sub(42 * scale) / cw.max(1);
			let title = session
				.custom_title
				.as_deref()
				.unwrap_or(&session.term.title);
			let mut label: String = title.chars().take(title_chars.max(1)).collect();
			if title.chars().count() > title_chars.max(1) && title_chars > 1 {
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
				x + (layout.arrow_w.saturating_sub(cw)) / 2,
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
			layout.plus_x + (layout.plus_w.saturating_sub(cw)) / 2,
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
			layout.settings_x + (layout.settings_w.saturating_sub(cw)) / 2,
			button_y + (button_h.saturating_sub(ch)) / 2,
			"⚙",
			px,
			ascent,
			self.theme.fg,
			font,
			glyphs,
		);
		let (ui_cw, ui_ch, ui_ascent) = (cw, ch, ascent);
		let (cw, ch, px, ascent) = (self.cw, self.ch, self.px, self.ascent);
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
			for (col, &cell) in row_cells.iter().enumerate().rev() {
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
				let is_cursor =
					term.cursor_visible && term.scroll_offset == 0 && row == term.cy && col == term.cx;
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
				// The frame's content area was already cleared to the terminal's
				// default background. Most terminal cells use that same color.
				if bg != self.theme.bg {
					fill_rect(pixels, w, h, x0, y0, cw, ch, bg);
				}

				if cell.attrs & 4 != 0 {
					fill_rect(
						pixels,
						w,
						h,
						x0,
						y0 + ch.saturating_sub(2 * scale),
						cw,
						scale.max(1),
						fg,
					);
				}
				if cell.attrs & 128 != 0 {
					fill_rect(
						pixels,
						w,
						h,
						x0,
						y0 + ch.saturating_sub(4 * scale),
						cw,
						scale.max(1),
						fg,
					);
				}
				if cell.attrs & 64 != 0 {
					fill_rect(pixels, w, h, x0, y0 + scale, cw, scale.max(1), fg);
				}
				if cell.attrs & 16 != 0 {
					fill_rect(pixels, w, h, x0, y0 + ch / 2, cw, scale.max(1), fg);
				}
				if (cell.ch == ' ' && cell.combining_len == 0) || cell.width == 0 {
					continue;
				}
				let use_fallback = fallback_font.is_some_and(|fallback| {
					font.lookup_glyph_index(cell.ch) == 0 && fallback.lookup_glyph_index(cell.ch) != 0
				});
				let glyph_font = if use_fallback {
					fallback_font.unwrap()
				} else {
					font
				};
				let (gm, bitmap) = glyphs
					.entry(glyph_cache_key_for(cell.ch, px, use_fallback))
					.or_insert_with(|| glyph_font.rasterize(cell.ch, px));

				let gx0 = x0 as i32 + gm.xmin;
				let gy0 = y0 as i32 + ascent - gm.ymin - gm.height as i32;
				let cell_left = x0 as i32;
				let cell_right = (x0 + cw * cell.width as usize).min(w) as i32;
				let glyph_top = gy0.max(0) as usize;
				let glyph_bottom = (gy0 + gm.height as i32).min(h as i32).max(0) as usize;
				for y in glyph_top..glyph_bottom {
					let gy = (y as i32 - gy0) as usize;
					let italic_shift = if cell.attrs & 2 != 0 {
						(gm.height.saturating_sub(gy) / 6) as i32
					} else {
						0
					};
					let glyph_left = gx0 + italic_shift;
					let draw_left = glyph_left.max(cell_left).max(0);
					let draw_right = (glyph_left + gm.width as i32).min(cell_right).min(w as i32);
					if draw_left >= draw_right {
						continue;
					}
					let src_start = gy * gm.width + (draw_left - glyph_left) as usize;
					let draw_width = (draw_right - draw_left) as usize;
					let dst_start = y * w + draw_left as usize;
					blend_coverage_row(
						&mut pixels[dst_start..dst_start + draw_width],
						&bitmap[src_start..src_start + draw_width],
						fg,
					);
				}
				if cell.hyperlink != 0 {
					fill_rect(
						pixels,
						w,
						h,
						x0,
						y0 + ch.saturating_sub(2 * scale),
						cw * cell.width as usize,
						scale.max(1),
						self.theme.palette[4],
					);
				}
				for mark in cell
					.combining
					.iter()
					.take(cell.combining_len as usize)
					.copied()
				{
					if mark == '\u{200d}' || mark == '\u{fe0f}' {
						continue;
					}
					let use_fallback = fallback_font.is_some_and(|fallback| {
						font.lookup_glyph_index(mark) == 0 && fallback.lookup_glyph_index(mark) != 0
					});
					let glyph_font = if use_fallback {
						fallback_font.unwrap()
					} else {
						font
					};
					let (mark_metrics, mark_bitmap) = glyphs
						.entry(glyph_cache_key_for(mark, px, use_fallback))
						.or_insert_with(|| glyph_font.rasterize(mark, px));
					let mark_x = x0 as i32 + (cw as i32 - mark_metrics.width as i32) / 2;
					let mark_y = y0 as i32 + ascent - mark_metrics.ymin - mark_metrics.height as i32;
					let draw_left = mark_x.max(x0 as i32).max(0);
					let draw_right = (mark_x + mark_metrics.width as i32)
						.min((x0 + cw) as i32)
						.min(w as i32);
					let draw_top = mark_y.max(0);
					let draw_bottom = (mark_y + mark_metrics.height as i32).min(h as i32);
					if draw_left < draw_right && draw_top < draw_bottom {
						let draw_width = (draw_right - draw_left) as usize;
						for py in draw_top..draw_bottom {
							let gy = (py - mark_y) as usize;
							let src_start = gy * mark_metrics.width + (draw_left - mark_x) as usize;
							let dst_start = py as usize * w + draw_left as usize;
							blend_coverage_row(
								&mut pixels[dst_start..dst_start + draw_width],
								&mark_bitmap[src_start..src_start + draw_width],
								fg,
							);
						}
					}
				}
			}
		}

		for image in &term.images {
			let image_x = m + image.col * cw;
			if image_x >= w {
				continue;
			}
			let image_y = top as isize + (image.row + term.scroll_offset as isize) * ch as isize;
			for sy in 0..image.bitmap.height {
				let y = image_y + sy as isize;
				if y < top as isize || y >= (top + term.rows * ch).min(h) as isize {
					continue;
				}
				let draw_width = image.bitmap.width.min(w.saturating_sub(image_x));
				let src_start = sy * image.bitmap.width;
				let dst_start = y as usize * w + image_x;
				if image.bitmap.opaque {
					pixels[dst_start..dst_start + draw_width]
						.copy_from_slice(&image.bitmap.pixels[src_start..src_start + draw_width]);
				} else {
					for sx in 0..draw_width {
						let pixel = image.bitmap.pixels[src_start + sx];
						if pixel != sixel::TRANSPARENT {
							pixels[dst_start + sx] = pixel;
						}
					}
				}
			}
		}

		let (cw, ch, px, ascent) = (ui_cw, ui_ch, ui_px, ui_ascent);
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
		if self.command_palette {
			let box_w = (520 * scale).min(w.saturating_sub(24 * scale));
			let box_h = 210 * scale;
			let x = (w.saturating_sub(box_w)) / 2;
			let y = top + 20 * scale;
			fill_round_rect(
				pixels,
				w,
				h,
				x,
				y,
				box_w,
				box_h,
				10 * scale,
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
				9 * scale,
				self.theme.bg,
			);
			let label = if self.renaming_tab {
				format!("Rename active tab: {}", self.command_query)
			} else {
				format!("Run command: {}", self.command_query)
			};
			draw_text(
				pixels,
				w,
				h,
				x + 16 * scale,
				y + 14 * scale,
				&label,
				px * 0.86,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
			let query = self.command_query.to_ascii_lowercase();
			if !self.renaming_tab {
				for (index, command) in [
					"New tab",
					"Settings",
					"About poopterm",
					"Cycle theme",
					"Close tab",
					"Rename active tab",
				]
				.into_iter()
				.filter(|command| command.to_ascii_lowercase().contains(&query))
				.enumerate()
				{
					draw_text(
						pixels,
						w,
						h,
						x + 20 * scale,
						y + (54 + index * 28) * scale,
						command,
						px * 0.78,
						ascent,
						self.theme.fg,
						font,
						glyphs,
					);
				}
			}
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
			"{} × {}   ·   {}{}",
			term.cols,
			term.rows,
			self.theme.kind.name(),
			if term.shell.phase == osc::Phase::Output {
				"   ·   running".to_owned()
			} else {
				term
					.shell
					.last_exit_code
					.map(|code| format!("   ·   exit {code}"))
					.unwrap_or_default()
			}
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

// ---- SIMD Accelerated Graphics & Blending Utilities -------------------------

#[allow(clippy::too_many_arguments)]
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
	let color_array: [u32; 4] = [color; 4];
	let height_end = y.saturating_add(rh).min(h);

	for yy in y..height_end {
		let row_start = yy * w + x;
		let row_end = yy * w + x_end;
		if row_end > px.len() {
			break;
		}
		let row_slice = &mut px[row_start..row_end];

		let (chunks, rem) = row_slice.as_chunks_mut::<4>();
		for chunk in chunks {
			*chunk = color_array;
		}
		rem.fill(color);
	}
}

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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
	glyphs: &mut HashMap<(char, u32, bool), (Metrics, Vec<u8>)>,
) {
	let mut cursor_x = x as i32;
	for ch in text.chars() {
		let (metrics, bitmap) = glyphs
			.entry(glyph_cache_key(ch, px))
			.or_insert_with(|| font.rasterize(ch, px));
		let gx0 = cursor_x + metrics.xmin;
		let gy0 = y as i32 + ascent - metrics.ymin - metrics.height as i32;
		let draw_left = gx0.max(0);
		let draw_right = (gx0 + metrics.width as i32).min(width as i32);
		let draw_top = gy0.max(0);
		let draw_bottom = (gy0 + metrics.height as i32).min(height as i32);
		if draw_left < draw_right && draw_top < draw_bottom {
			let draw_width = (draw_right - draw_left) as usize;
			for py in draw_top..draw_bottom {
				let gy = (py - gy0) as usize;
				let src_start = gy * metrics.width + (draw_left - gx0) as usize;
				let dst_start = py as usize * width + draw_left as usize;
				blend_coverage_row(
					&mut pixels[dst_start..dst_start + draw_width],
					&bitmap[src_start..src_start + draw_width],
					color,
				);
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
	let mut child = Command::new("open").arg(target).spawn()?;
	thread::spawn(move || {
		let _ = child.wait();
	});
	Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_external(target: &str) -> std::io::Result<()> {
	let mut child = Command::new("xdg-open").arg(target).spawn()?;
	thread::spawn(move || {
		let _ = child.wait();
	});
	Ok(())
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
	if env::var_os("WAYLAND_DISPLAY").is_some()
		&& let Ok(()) = clipboard_write_with("wl-copy", &[], text)
	{
		return Ok(());
	}
	clipboard_write_with("xclip", &["-selection", "clipboard"], text)
		.or_else(|_| clipboard_write_with("xsel", &["--clipboard", "--input"], text))
}

#[cfg(target_os = "linux")]
fn clipboard_read() -> std::io::Result<String> {
	if env::var_os("WAYLAND_DISPLAY").is_some()
		&& let Ok(text) = clipboard_read_with("wl-paste", &["--no-newline"])
	{
		return Ok(text);
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

#[inline]
fn blend(bg: u32, fg: u32, a: u32) -> u32 {
	if a == 0 {
		return bg;
	}
	if a == 255 {
		return fg;
	}
	let inverse = 255 - a;
	let red = (((fg >> 16 & 0xff) * a + (bg >> 16 & 0xff) * inverse) / 255) << 16;
	let green = (((fg >> 8 & 0xff) * a + (bg >> 8 & 0xff) * inverse) / 255) << 8;
	let blue = ((fg & 0xff) * a + (bg & 0xff) * inverse) / 255;
	red | green | blue
}

#[inline]
fn blend_coverage_row(pixels: &mut [u32], coverage: &[u8], fg: u32) {
	let (pixel_chunks, pixel_remainder) = pixels.as_chunks_mut::<4>();
	let (coverage_chunks, coverage_remainder) = coverage.as_chunks::<4>();
	for (background, alpha) in pixel_chunks.iter_mut().zip(coverage_chunks) {
		if alpha.iter().all(|&value| value == 0) {
			continue;
		}
		if alpha.iter().all(|&value| value == u8::MAX) {
			*background = [fg; 4];
		} else {
			let background_vector = u32x4::from_array(*background);
			let alpha = u32x4::from_array(alpha.map(u32::from));
			*background = blend_chunk(background_vector, u32x4::splat(fg), alpha).to_array();
		}
	}
	for (pixel, alpha) in pixel_remainder.iter_mut().zip(coverage_remainder) {
		*pixel = blend(*pixel, fg, *alpha as u32);
	}
}

#[inline]
fn blend_chunk(bg: u32x4, fg: u32x4, a: u32x4) -> u32x4 {
	let inv_a_v = u32x4::splat(255) - a;
	let max_a = u32x4::splat(255);
	let fg_r = (fg >> u32x4::splat(16)) & u32x4::splat(0xff);
	let bg_r = (bg >> u32x4::splat(16)) & u32x4::splat(0xff);
	let r = ((fg_r * a + bg_r * inv_a_v) / max_a) << u32x4::splat(16);
	let fg_g = (fg >> u32x4::splat(8)) & u32x4::splat(0xff);
	let bg_g = (bg >> u32x4::splat(8)) & u32x4::splat(0xff);
	let g = ((fg_g * a + bg_g * inv_a_v) / max_a) << u32x4::splat(8);
	let fg_b = fg & u32x4::splat(0xff);
	let bg_b = bg & u32x4::splat(0xff);
	let b = (fg_b * a + bg_b * inv_a_v) / max_a;
	r | g | b
}

fn glyph_cache_key(ch: char, px: f32) -> (char, u32, bool) {
	glyph_cache_key_for(ch, px, false)
}

fn glyph_cache_key_for(ch: char, px: f32, fallback: bool) -> (char, u32, bool) {
	(ch, px.to_bits(), fallback)
}

#[cfg(target_os = "macos")]
fn set_window_opacity(window: &Window, opacity: usize) {
	use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
	let Ok(handle) = window.window_handle() else {
		return;
	};
	let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
		return;
	};
	let view = handle.ns_view.as_ptr().cast::<objc2::runtime::AnyObject>();
	unsafe {
		let native_window: *mut objc2::runtime::AnyObject = objc2::msg_send![view, window];
		if !native_window.is_null() {
			let _: () = objc2::msg_send![native_window, setAlphaValue: opacity as f64 / 100.0];
		}
	}
}

impl ApplicationHandler<UserEvent> for App {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.window.is_some() {
			return;
		}

		let attrs = Window::default_attributes()
			.with_title("💩 poopterm")
			.with_inner_size(LogicalSize::new(900.0, 560.0))
			.with_min_inner_size(LogicalSize::new(480.0, 240.0));

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
		#[cfg(target_os = "macos")]
		set_window_opacity(&window, Preferences::load().opacity);

		let scale = window.scale_factor() as f32;
		self.px = Preferences::load().font_size as f32 * scale;
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
			(self.ch * self.active_term().rows + self.top + self.margin + self.status_height) as u32,
		);
		let _ = window.request_inner_size(size);

		let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
		let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
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
				self.refresh_directories(false);
				self.persist_sessions();
				event_loop.exit();
			}
			WindowEvent::ModifiersChanged(m) => {
				self.ctrl = m.state().control_key();
				self.shift = m.state().shift_key();
				self.alt = m.state().alt_key();
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
					} else if layout.first_visible + layout.visible_count < self.sessions.len() && !scroll_up
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
				if self.shortcut_matches(
					&event.logical_key,
					"shortcut_command_palette",
					"ctrl+shift+p",
				) {
					self.end_search();
					self.command_palette = true;
					self.command_query.clear();
					if let Some(window) = &self.window {
						window.request_redraw();
					}
					return;
				}
				if self.command_palette {
					if self.renaming_tab {
						match &event.logical_key {
							Key::Named(NamedKey::Escape) => {
								self.renaming_tab = false;
								self.command_palette = false;
							}
							Key::Named(NamedKey::Backspace) => {
								self.command_query.pop();
							}
							Key::Named(NamedKey::Enter) => {
								let title = self.command_query.trim().to_owned();
								self.sessions[self.active].custom_title = (!title.is_empty()).then_some(title);
								self.renaming_tab = false;
								self.command_palette = false;
								self.update_window_title();
							}
							Key::Character(value) if !self.ctrl && !self.super_key => {
								let available = 64usize.saturating_sub(self.command_query.chars().count());
								self
									.command_query
									.extend(value.chars().filter(|c| !c.is_control()).take(available));
							}
							_ => {}
						}
						if let Some(window) = &self.window {
							window.request_redraw();
						}
						return;
					}
					match &event.logical_key {
						Key::Named(NamedKey::Escape) => self.command_palette = false,
						Key::Named(NamedKey::Backspace) => {
							self.command_query.pop();
						}
						Key::Named(NamedKey::Enter) => {
							let query = self.command_query.to_ascii_lowercase();
							self.command_palette = false;
							let commands = [
								"New tab",
								"Settings",
								"About poopterm",
								"Cycle theme",
								"Close tab",
								"Rename active tab",
							];
							if let Some(command) = commands
								.iter()
								.find(|command| command.to_ascii_lowercase().contains(&query))
							{
								match *command {
									"Rename active tab" => {
										self.renaming_tab = true;
										self.command_query = self.active_term().title.clone();
									}
									"New tab" => {
										self.command_palette = false;
										self.add_tab();
									}
									"Settings" => {
										self.command_palette = false;
										self.open_settings_window(event_loop);
									}
									"About poopterm" => {
										self.command_palette = false;
										self.open_settings_window(event_loop);
										self.settings_editor.about = true;
										if let Some(window) = &self.settings_window {
											window.request_redraw();
										}
									}
									"Cycle theme" => {
										self.command_palette = false;
										self.cycle_theme();
									}
									"Close tab" => {
										self.command_palette = false;
										self.close_active_tab(event_loop);
									}
									_ => {}
								}
							}
						}
						Key::Character(value) if !self.ctrl && !self.super_key => {
							let available = 64usize.saturating_sub(self.command_query.chars().count());
							self
								.command_query
								.extend(value.chars().filter(|c| !c.is_control()).take(available));
						}
						_ => {}
					}
					if let Some(window) = &self.window {
						window.request_redraw();
					}
					return;
				}
				let search_shortcut = self.shortcut_matches(
					&event.logical_key,
					"shortcut_search",
					if cfg!(target_os = "macos") {
						"super+f"
					} else {
						"ctrl+shift+f"
					},
				);
				if search_shortcut {
					self.begin_search();
					return;
				}
				if self.search_active {
					if (self.super_key || (self.ctrl && self.shift))
						&& matches!(&event.logical_key, Key::Character(s) if s.eq_ignore_ascii_case("v"))
					{
						if let Ok(text) = clipboard_read() {
							let available = 128usize.saturating_sub(self.search_query.chars().count());
							self
								.search_query
								.extend(text.chars().filter(|c| !c.is_control()).take(available));
							self.search(1, false);
						}
						return;
					}
					match &event.logical_key {
						Key::Named(NamedKey::Escape) => self.end_search(),
						Key::Named(NamedKey::Enter) => self.search(if self.shift { -1 } else { 1 }, true),
						Key::Named(NamedKey::Backspace) => {
							self.search_query.pop();
							self.search(1, false);
						}
						Key::Character(value) if !self.ctrl && !self.super_key => {
							let available = 128usize.saturating_sub(self.search_query.chars().count());
							self.search_query.extend(value.chars().take(available));
							self.search(1, false);
						}
						_ => {}
					}
					return;
				}
				if self.shortcut_matches(&event.logical_key, "shortcut_cycle_theme", "ctrl+shift+t") {
					self.cycle_theme();
					return;
				}
				if self.shortcut_matches(
					&event.logical_key,
					"shortcut_new_tab",
					if cfg!(target_os = "macos") {
						"super+t"
					} else {
						"ctrl+shift+n"
					},
				) {
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
				if (self.super_key || self.ctrl)
					&& !self.shift
					&& let Key::Character(value) = &event.logical_key
					&& value.len() == 1
					&& let Some(index) = value.chars().next().and_then(|ch| ch.to_digit(10))
					&& (1..=9).contains(&index)
					&& (index as usize) <= self.sessions.len()
				{
					self.activate_tab(index as usize - 1);
					return;
				}
				if self.shortcut_matches(
					&event.logical_key,
					"shortcut_close_tab",
					if cfg!(target_os = "macos") {
						"super+w"
					} else {
						"ctrl+shift+w"
					},
				) {
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
				if (previous.1 < self.top as f64 || position.y < self.top as f64)
					&& let Some(window) = &self.window
				{
					window.request_redraw();
				}
			}
			WindowEvent::MouseInput { state, button, .. } => {
				let (x, y) = self.cursor_pos;
				self.mouse_button_down = state == ElementState::Pressed;
				let tab_y = self.tab_y() as f64;
				if state == ElementState::Pressed && button == MouseButton::Left && y < self.top as f64 {
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
									&& x >= (left + layout.width.saturating_sub(28 * layout.scale)) as f64
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
					let handled = self.send_mouse_report(code, x, y, state == ElementState::Released, false);
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

	fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
		let now = std::time::Instant::now();
		let mut changed = false;
		for session in &mut self.sessions {
			changed |= session.term.finish_sync_if_expired(now);
		}
		if changed {
			self.persist_sessions();
			self.search_dirty = true;
			self.update_window_title();
			if let Some(window) = &self.window {
				window.request_redraw();
			}
		}
		if let Some(deadline) = self
			.sessions
			.iter()
			.filter_map(|s| s.term.sync_deadline)
			.min()
		{
			event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
		} else {
			event_loop.set_control_flow(ControlFlow::Wait);
		}
	}

	fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
		match event {
			UserEvent::Output(id, bytes, ack) => {
				// Keep one parser per PTY stream to preserve partial escape sequences.
				let mut active_output = false;
				let mut became_active = false;
				let mut cwd_changed = false;
				let active_id = self.sessions.get(self.active).map(|session| session.id);
				if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
					let previous_bell = session.term.bell;
					let previous_title_generation = session.term.title_generation;
					let previous_cwd_generation = session.term.cwd_generation;

					session.parser.advance(&mut session.term, &bytes);

					cwd_changed = session.term.cwd_generation != previous_cwd_generation;
					became_active |= session.term.bell != previous_bell
						|| session.term.title_generation != previous_title_generation;
					if !session.term.responses.is_empty() {
						for response in std::mem::take(&mut session.term.responses) {
							let _ = session.writer.write_all(&response);
						}
						let _ = session.writer.flush();
					}
					if Some(id) == active_id {
						active_output = session.term.synchronized.is_none();
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
						self.search_dirty = true;
					}
					self.update_window_title();
				}
				if (active_output || became_active)
					&& let Some(win) = &self.window
				{
					win.request_redraw();
				}
				let _ = ack.send(bytes);
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
	if let Some(argument) = env::args().nth(1) {
		match argument.as_str() {
			"--version" | "-V" => println!("poopterm {}", env!("CARGO_PKG_VERSION")),
			"--help" | "-h" => println!(
				"Poopterm {}\n\nUsage: poopterm [--version | --help]\n\nLaunches your login shell. Configure appearance and session restore with Settings.\nPOOPTERM_CONFIG overrides the config path; POOPTERM_THEME overrides the theme.",
				env!("CARGO_PKG_VERSION")
			),
			_ => return Err(format!("unknown option: {argument}; try --help").into()),
		}
		return Ok(());
	}
	let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
	let proxy = event_loop.create_proxy();

	let (font, fallback_font) = load_fonts();

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
		fallback_font,
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
		alt: false,
		super_key: false,
		cursor_pos: (0.0, 0.0),
		mouse_button_down: false,
		selection: None,
		selecting: false,
		search_active: false,
		search_dirty: false,
		search_query: String::new(),
		search_matches: Vec::new(),
		search_match_index: 0,
		command_palette: false,
		command_query: String::new(),
		renaming_tab: false,
	};

	event_loop.run_app(&mut app)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::{Parser, Theme, ThemeKind, glyph_cache_key, tab_layout};

	#[test]
	fn theme_labels_and_catppuccin_palette_match_expected_values() {
		assert_eq!(ThemeKind::MacOsDark.name(), "darkmode");
		assert_eq!(ThemeKind::MacOsLight.name(), "light-mode");
		assert_eq!(ThemeKind::parse("macos-dark"), Some(ThemeKind::MacOsDark));
		assert_eq!(ThemeKind::parse("macos-light"), Some(ThemeKind::MacOsLight));

		let flavors = [
			(
				ThemeKind::CatppuccinLatte,
				0x4C4F69,
				0xEFF1F5,
				0xCCD0DA,
				[
					0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE,
				],
			),
			(
				ThemeKind::CatppuccinFrappe,
				0xC6D0F5,
				0x303446,
				0x414559,
				[
					0x51576D, 0xE78284, 0xA6D189, 0xE5C890, 0x8CAAEE, 0xCA9EE6, 0x81C8BE, 0xB5BFE2,
				],
			),
			(
				ThemeKind::CatppuccinMacchiato,
				0xCAD3F5,
				0x24273A,
				0x363A4F,
				[
					0x494D64, 0xED8796, 0xA6DA95, 0xEED49F, 0x8AADF4, 0xF5BDE6, 0x8BD5CA, 0xB8C0E0,
				],
			),
			(
				ThemeKind::CatppuccinMocha,
				0xCDD6F4,
				0x1E1E2E,
				0x45475A,
				[
					0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
				],
			),
		];
		for (kind, fg, bg, border, palette) in flavors {
			let theme = Theme::from_kind(kind);
			assert_eq!(
				(theme.fg, theme.bg, theme.border, theme.palette),
				(fg, bg, border, palette)
			);
		}
	}

	#[test]
	fn search_handles_wide_cells_and_long_queries() {
		let mut term = super::Term::new(
			20,
			2,
			super::Theme::from_kind(super::ThemeKind::Poop),
			"/tmp".into(),
		);
		Parser::new().advance(&mut term, "猫 hello HELLO".as_bytes());
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
		Parser::new().advance(&mut term, b"\x1b]7;file://localhost/tmp/my%20dir\x07\x07");
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

#[cfg(test)]
mod terminal_tests;
