//! Application state and the winit event loop.
mod draw;
mod draw_tabs;
mod frame_pacer;
mod handler;
mod keyboard;
mod mouse;
mod output;
mod search;
mod settings;
mod settings_view;
mod tabs;

use portable_pty::PtySize;
use std::{error::Error, num::NonZeroU32, path::PathBuf, sync::Arc};

use winit::{
	event::{ElementState, MouseButton, WindowEvent},
	event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
	keyboard::{Key, NamedKey},
	window::Window,
};

use crate::{
	clipboard::*,
	config::{
		self, Preferences, config_path, quote_config_string, read_config_value, save_config_theme,
	},
	fonts::load_fonts,
	input::{
		keys::Modifiers, mouse::MouseButtonCode, scroll::ScrollAccumulator, shortcuts::Shortcuts,
	},
	pty::{Session, UserEvent, spawn_session},
	render::{blend::blend, glyphs::FontSet},
	session_store::restored_session_dirs,
	term::Term,
	theme::{Theme, ThemeKind},
	ui::{
		search::find_matches,
		selection::{CellPoint, Selection},
		tabs::{TabLayout, tab_layout},
	},
	wallpaper::Wallpaper,
};

pub(crate) use settings::SettingsEditor;

pub(crate) struct App {
	theme: Theme,
	wallpaper: Option<Wallpaper>,
	wallpaper_strength: usize,
	fonts: FontSet,
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
	mods: Modifiers,
	shortcuts: Shortcuts,
	cursor_pos: (f64, f64),
	held_button: Option<MouseButtonCode>,
	scroll: ScrollAccumulator,
	/// Whether the main window has keyboard focus.
	focused: bool,
	/// Hyperlink target under the pointer, shown in the status bar.
	hovered_link: Option<String>,
	pacer: frame_pacer::FramePacer,
	/// Reused buffer for bytes taken from a session's output queue.
	read_buffer: Vec<u8>,
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

impl App {
	pub(super) fn active_term(&self) -> &Term {
		&self.sessions[self.active].term
	}

	pub(super) fn active_session_mut(&mut self) -> &mut Session {
		&mut self.sessions[self.active]
	}

	pub(super) fn cycle_theme(&mut self) {
		self.apply_theme(self.theme.kind.next());
	}

	pub(super) fn apply_theme(&mut self, kind: ThemeKind) {
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

	pub(super) fn request_redraw(&self) {
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	/// Sets the terminal font size in physical pixels and recomputes cell metrics.
	pub(super) fn set_font_px(&mut self, px: f32) {
		self.px = px;
		let font = self.fonts.primary();
		if let Some(metrics) = font.horizontal_line_metrics(px) {
			self.ascent = metrics.ascent.round() as i32;
			self.ch = metrics.new_line_size.ceil() as usize;
		}
		self.cw = font.metrics('M', px).advance_width.ceil() as usize;
		self.fonts.clear();
	}

	pub(super) fn resize(&mut self, w: u32, h: u32) {
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
}

/// Opens the main window and runs the event loop until the last tab closes.
pub(crate) fn run() -> Result<(), Box<dyn Error>> {
	let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
	let proxy = event_loop.create_proxy();

	let (font, fallback_fonts) = load_fonts();

	let theme = Theme::load();
	let settings_editor = SettingsEditor::load();
	let wallpaper_strength = settings_editor.wallpaper_strength;
	let wallpaper = read_config_value("wallpaper")
		.filter(|path| !path.is_empty())
		.and_then(|path| match Wallpaper::load(&PathBuf::from(path)) {
			Ok(wallpaper) => Some(wallpaper),
			Err(error) => {
				eprintln!("poopterm: wallpaper ignored: {error}");
				None
			}
		});
	let mut sessions = Vec::new();
	let (initial_cols, initial_rows) = (100, 30);
	for (index, cwd) in restored_session_dirs().into_iter().enumerate() {
		sessions.push(spawn_session(
			index as u64 + 1,
			initial_cols,
			initial_rows,
			theme,
			proxy.clone(),
			cwd,
		)?);
	}
	let next_session_id = sessions.len() as u64 + 1;
	let mut app = App {
		theme,
		wallpaper,
		wallpaper_strength,
		fonts: FontSet::new(font, fallback_fonts),
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
		settings_editor,
		width: 0,
		height: 0,
		mods: Modifiers::default(),
		shortcuts: Shortcuts::load(),
		cursor_pos: (0.0, 0.0),
		held_button: None,
		scroll: ScrollAccumulator::default(),
		focused: true,
		hovered_link: None,
		pacer: frame_pacer::FramePacer::new(std::time::Duration::from_micros(16_667)),
		read_buffer: Vec::new(),
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
