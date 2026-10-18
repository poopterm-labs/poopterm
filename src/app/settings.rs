use super::*;
use winit::dpi::LogicalSize;

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

	pub(super) fn save_settings_config(&mut self) {
		let preferences = self.settings_editor.preferences;
		let wallpaper_path = self.settings_editor.wallpaper_path.as_ref().map_or_else(
			|| "\"\"".to_owned(),
			|path| quote_config_string(&path.to_string_lossy()),
		);
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
				("wallpaper", wallpaper_path),
				(
					"wallpaper_strength",
					self.settings_editor.wallpaper_strength.to_string(),
				),
			],
		);
		match result {
			Ok(()) => {
				self.theme = Theme::load();
				self.wallpaper_strength = self.settings_editor.wallpaper_strength;
				self.wallpaper = self
					.settings_editor
					.wallpaper_path
					.as_ref()
					.and_then(|path| match Wallpaper::load(path) {
						Ok(wallpaper) => Some(wallpaper),
						Err(error) => {
							self.settings_editor.status = format!("Wallpaper could not be loaded: {error}");
							None
						}
					});
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
				let status = if self.settings_editor.wallpaper_path.is_some() && self.wallpaper.is_none() {
					self.settings_editor.status.clone()
				} else {
					"Saved. Settings applied.".into()
				};
				self.settings_editor = SettingsEditor::load();
				self.settings_editor.status = status;
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

	pub(super) fn settings_window_event(&mut self, event: WindowEvent) {
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
				if (228 * scale..260 * scale).contains(&y) && (24 * scale..176 * scale).contains(&x) {
					let picked = self.settings_window.as_ref().and_then(|window| {
						rfd::FileDialog::new()
							.set_title("Choose a terminal wallpaper")
							.add_filter("Wallpaper images", &["png", "jpg", "jpeg", "webp"])
							.set_parent(window.as_ref())
							.pick_file()
					});
					if let Some(path) = picked {
						match Wallpaper::load(&path) {
							Ok(_) => {
								self.settings_editor.wallpaper_path = Some(path);
								self.settings_editor.status = "Wallpaper selected. Click Save to apply.".into();
							}
							Err(error) => {
								self.settings_editor.status = format!("Could not use wallpaper: {error}");
							}
						}
					}
					if let Some(window) = &self.settings_window {
						window.request_redraw();
					}
					return;
				}
				if (228 * scale..260 * scale).contains(&y) && (188 * scale..284 * scale).contains(&x) {
					self.settings_editor.wallpaper_path = None;
					self.settings_editor.status = "Wallpaper cleared. Click Save to apply.".into();
					if let Some(window) = &self.settings_window {
						window.request_redraw();
					}
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
					} else if (438 * scale..478 * scale).contains(&y) {
						self.settings_editor.wallpaper_strength = if increase {
							(self.settings_editor.wallpaper_strength + 5).min(100)
						} else {
							self.settings_editor.wallpaper_strength.saturating_sub(5)
						};
					} else if cfg!(target_os = "macos") && (494 * scale..534 * scale).contains(&y) {
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
				let is_save = (self.mods.ctrl || self.mods.super_key)
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
}
