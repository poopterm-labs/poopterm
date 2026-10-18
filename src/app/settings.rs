//! The Settings window: editable preferences, saving, and input handling.
use super::{settings_view::*, *};
use crate::input::shortcuts::Shortcuts;
use winit::{dpi::LogicalSize, event::KeyEvent};

/// Unsaved preference edits shown in the Settings window.
pub(crate) struct SettingsEditor {
	pub(crate) theme: ThemeKind,
	pub(crate) preferences: Preferences,
	pub(crate) wallpaper_path: Option<PathBuf>,
	pub(crate) wallpaper_strength: usize,
	pub(crate) status: String,
	pub(crate) about: bool,
}

impl SettingsEditor {
	pub(super) fn load() -> Self {
		Self {
			theme: Theme::load().kind,
			preferences: Preferences::load(),
			wallpaper_path: read_config_value("wallpaper")
				.filter(|path| !path.is_empty())
				.map(PathBuf::from),
			wallpaper_strength: read_config_value("wallpaper_strength")
				.and_then(|value| value.parse().ok())
				.unwrap_or(35)
				.clamp(0, 100),
			status: "Change preferences, then click Save.".into(),
			about: false,
		}
	}

	/// Applies a `−` (`step < 0`) or `+` click on the stepper in `row`.
	fn step(&mut self, row: usize, step: isize) {
		let up = step > 0;
		let prefs = &mut self.preferences;
		match row {
			FONT_ROW => {
				prefs.font_size = if up {
					(prefs.font_size + 1).min(32)
				} else {
					prefs.font_size.saturating_sub(1).max(10)
				}
			}
			SCROLLBACK_ROW => {
				prefs.scrollback_lines = if up {
					(prefs.scrollback_lines + 1000).min(100_000)
				} else {
					prefs.scrollback_lines.saturating_sub(1000)
				}
			}
			STRENGTH_ROW => {
				self.wallpaper_strength = if up {
					(self.wallpaper_strength + 5).min(100)
				} else {
					self.wallpaper_strength.saturating_sub(5)
				}
			}
			OPACITY_ROW => {
				prefs.opacity = if up {
					(prefs.opacity + 5).min(100)
				} else {
					prefs.opacity.saturating_sub(5).max(60)
				}
			}
			_ => {}
		}
	}
}

/// A clickable control in the Settings window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsTarget {
	About,
	Save,
	ChooseWallpaper,
	ClearWallpaper,
	ToggleRestore,
	/// A stepper button: the row constant and -1 or +1.
	Step(usize, isize),
	/// A theme arrow: -1 or +1.
	Theme(isize),
}

/// Maps a click at physical `(x, y)` to the control under it.
pub(super) fn settings_target(
	(x, y): (usize, usize),
	(width, height): (usize, usize),
	scale: usize,
	about_page: bool,
) -> Option<SettingsTarget> {
	let s = scale;
	let in_row = |row: usize| (row * s..(row + ROW_HEIGHT) * s).contains(&y);
	// The About button toggles into a wider "Settings" back button.
	let (about_x, about_w) = if about_page {
		(width.saturating_sub(164 * s), 140 * s)
	} else {
		(width.saturating_sub(140 * s), 116 * s)
	};
	if (20 * s..56 * s).contains(&y) && (about_x..about_x + about_w).contains(&x) {
		return Some(SettingsTarget::About);
	}
	if about_page {
		return None;
	}
	let (save_x, save_y) = (width.saturating_sub(140 * s), height.saturating_sub(52 * s));
	if (save_x..save_x + 120 * s).contains(&x) && (save_y..save_y + 36 * s).contains(&y) {
		return Some(SettingsTarget::Save);
	}
	if (WALLPAPER_ROW * s..(WALLPAPER_ROW + 32) * s).contains(&y) {
		if (24 * s..176 * s).contains(&x) {
			return Some(SettingsTarget::ChooseWallpaper);
		}
		if (188 * s..284 * s).contains(&x) {
			return Some(SettingsTarget::ClearWallpaper);
		}
	}
	let right = |offset: usize| width.saturating_sub(offset * s);
	if in_row(RESTORE_ROW) && (right(240)..right(24)).contains(&x) {
		return Some(SettingsTarget::ToggleRestore);
	}
	let step = if (right(240)..right(192)).contains(&x) {
		-1
	} else if (right(72)..right(24)).contains(&x) {
		1
	} else {
		0
	};
	if step != 0 {
		let mut rows = vec![FONT_ROW, SCROLLBACK_ROW, STRENGTH_ROW];
		if cfg!(target_os = "macos") {
			rows.push(OPACITY_ROW);
		}
		if let Some(row) = rows.into_iter().find(|&row| in_row(row)) {
			return Some(SettingsTarget::Step(row, step));
		}
	}
	if in_row(THEME_ROW) {
		let (content_x, button_w) = (24 * s, 48 * s);
		let content_end = content_x + width.saturating_sub(48 * s);
		if x < content_x + button_w {
			return Some(SettingsTarget::Theme(-1));
		}
		if x >= content_end - button_w {
			return Some(SettingsTarget::Theme(1));
		}
	}
	None
}

impl App {
	pub(super) fn open_settings_window(&mut self, event_loop: &ActiveEventLoop) {
		if let Some(window) = &self.settings_window {
			window.focus_window();
			return;
		}
		self.settings_editor = SettingsEditor::load();
		let attributes = Window::default_attributes()
			.with_title("Poopterm Settings")
			.with_inner_size(LogicalSize::new(760.0, 640.0))
			.with_min_inner_size(LogicalSize::new(680.0, 600.0));
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

	fn close_settings_window(&mut self) {
		self.settings_window = None;
		self.settings_surface = None;
		self._settings_context = None;
	}

	fn redraw_settings(&self) {
		if let Some(window) = &self.settings_window {
			window.request_redraw();
		}
	}

	pub(super) fn save_settings_config(&mut self) {
		let editor = &self.settings_editor;
		let preferences = editor.preferences;
		let wallpaper_path = editor.wallpaper_path.as_ref().map_or_else(
			|| "\"\"".to_owned(),
			|path| quote_config_string(&path.to_string_lossy()),
		);
		let result = config::save(
			&config_path(),
			&[
				("theme", format!("\"{}\"", editor.theme.name())),
				("font_size", preferences.font_size.to_string()),
				("scrollback_lines", preferences.scrollback_lines.to_string()),
				("restore_sessions", preferences.restore_sessions.to_string()),
				("opacity", preferences.opacity.to_string()),
				("wallpaper", wallpaper_path),
				("wallpaper_strength", editor.wallpaper_strength.to_string()),
			],
		);
		if let Err(error) = result {
			self.settings_editor.status = format!("Could not save: {error}");
		} else {
			self.apply_saved_settings(preferences);
		}
		self.redraw_settings();
		self.request_redraw();
	}

	fn apply_saved_settings(&mut self, preferences: Preferences) {
		self.theme = Theme::load();
		self.shortcuts = Shortcuts::load();
		self.wallpaper_strength = self.settings_editor.wallpaper_strength;
		self.wallpaper = None;
		let mut status = "Saved. Settings applied.".to_owned();
		if let Some(path) = &self.settings_editor.wallpaper_path {
			match Wallpaper::load(path) {
				Ok(wallpaper) => self.wallpaper = Some(wallpaper),
				Err(error) => status = format!("Wallpaper could not be loaded: {error}"),
			}
		}
		#[cfg(target_os = "macos")]
		if let Some(window) = &self.window {
			crate::platform::set_window_opacity(window, preferences.opacity);
		}
		for session in &self.sessions {
			let mut term = session.term();
			term.set_theme(self.theme);
			term.set_scrollback_limit(preferences.scrollback_lines);
		}
		let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor()) as f32;
		self.set_font_px(preferences.font_size as f32 * scale);
		self.selection = None;
		self.end_search();
		self.resize(self.width, self.height);
		self.settings_editor = SettingsEditor::load();
		self.settings_editor.status = status;
	}

	pub(super) fn settings_window_event(&mut self, event: WindowEvent) {
		match event {
			WindowEvent::CloseRequested => self.close_settings_window(),
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
				self.redraw_settings();
			}
			WindowEvent::MouseInput {
				state: ElementState::Pressed,
				button: MouseButton::Left,
				..
			} => self.settings_click(),
			WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
				self.settings_key(&event)
			}
			WindowEvent::RedrawRequested => self.draw_settings(),
			_ => {}
		}
	}

	fn settings_click(&mut self) {
		let scale = self
			.settings_window
			.as_ref()
			.map_or(1, |window| window.scale_factor().round() as usize)
			.max(1);
		let point = (
			self.settings_cursor_pos.0.max(0.0) as usize,
			self.settings_cursor_pos.1.max(0.0) as usize,
		);
		let size = (self.settings_width as usize, self.settings_height as usize);
		let editor = &mut self.settings_editor;
		match settings_target(point, size, scale, editor.about) {
			Some(SettingsTarget::About) => editor.about = !editor.about,
			Some(SettingsTarget::Save) => return self.save_settings_config(),
			Some(SettingsTarget::ChooseWallpaper) => self.choose_wallpaper(),
			Some(SettingsTarget::ClearWallpaper) => {
				editor.wallpaper_path = None;
				editor.status = "Wallpaper cleared. Click Save to apply.".into();
			}
			Some(SettingsTarget::ToggleRestore) => {
				editor.preferences.restore_sessions = !editor.preferences.restore_sessions
			}
			Some(SettingsTarget::Step(row, step)) => editor.step(row, step),
			Some(SettingsTarget::Theme(step)) => editor.theme = step_theme(editor.theme, step),
			None => {}
		}
		self.redraw_settings();
	}

	fn choose_wallpaper(&mut self) {
		let picked = self.settings_window.as_ref().and_then(|window| {
			rfd::FileDialog::new()
				.set_title("Choose a terminal wallpaper")
				.add_filter("Wallpaper images", &["png", "jpg", "jpeg", "webp"])
				.set_parent(window.as_ref())
				.pick_file()
		});
		let Some(path) = picked else {
			return;
		};
		let editor = &mut self.settings_editor;
		match Wallpaper::load(&path) {
			Ok(_) => {
				editor.wallpaper_path = Some(path);
				editor.status = "Wallpaper selected. Click Save to apply.".into();
			}
			Err(error) => editor.status = format!("Could not use wallpaper: {error}"),
		}
	}

	fn settings_key(&mut self, event: &KeyEvent) {
		let key = &event.logical_key;
		let is_save = (self.mods.ctrl || self.mods.super_key)
			&& matches!(key, Key::Character(s) if s.eq_ignore_ascii_case("s"));
		match key {
			_ if is_save => return self.save_settings_config(),
			Key::Named(NamedKey::Enter) => return self.save_settings_config(),
			Key::Named(NamedKey::Escape) if self.settings_editor.about => {
				self.settings_editor.about = false
			}
			Key::Named(NamedKey::Escape) => return self.close_settings_window(),
			Key::Named(NamedKey::ArrowLeft) => {
				self.settings_editor.theme = step_theme(self.settings_editor.theme, -1)
			}
			Key::Named(NamedKey::ArrowRight) => {
				self.settings_editor.theme = step_theme(self.settings_editor.theme, 1)
			}
			_ => {}
		}
		self.redraw_settings();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const SIZE: (usize, usize) = (760, 640);

	#[test]
	fn clicks_map_to_settings_controls() {
		let target = |x, y| settings_target((x, y), SIZE, 1, false);
		assert_eq!(target(630, 30), Some(SettingsTarget::About));
		assert_eq!(target(630, 600), Some(SettingsTarget::Save));
		assert_eq!(target(30, 240), Some(SettingsTarget::ChooseWallpaper));
		assert_eq!(target(200, 240), Some(SettingsTarget::ClearWallpaper));
		assert_eq!(target(530, 390), Some(SettingsTarget::ToggleRestore));
		assert_eq!(target(530, 280), Some(SettingsTarget::Step(FONT_ROW, -1)));
		assert_eq!(
			target(700, 330),
			Some(SettingsTarget::Step(SCROLLBACK_ROW, 1))
		);
		assert_eq!(target(30, 150), Some(SettingsTarget::Theme(-1)));
		assert_eq!(target(720, 150), Some(SettingsTarget::Theme(1)));
		assert_eq!(target(300, 100), None);
	}

	#[test]
	fn about_page_only_exposes_the_back_button() {
		assert_eq!(settings_target((630, 600), SIZE, 1, true), None);
		assert_eq!(settings_target((750, 30), SIZE, 1, true), None);
		assert_eq!(
			settings_target((610, 30), SIZE, 1, true),
			Some(SettingsTarget::About)
		);
	}

	#[test]
	fn hit_testing_scales_with_the_window() {
		assert_eq!(
			settings_target((530 * 2, 280 * 2), (SIZE.0 * 2, SIZE.1 * 2), 2, false),
			Some(SettingsTarget::Step(FONT_ROW, -1))
		);
	}
}
