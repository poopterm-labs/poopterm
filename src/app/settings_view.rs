//! Drawing the Settings window. Row positions are logical pixels, multiplied by
//! the window scale; `settings_hit.rs` hit-tests clicks against the same
//! constants.
use super::*;
use super::{
	settings_hit::{HEADER_BUTTONS, SettingsPage, font_list_rows},
	settings_pen::Pen,
};
use crate::input::shortcuts::ShortcutAction;
use crate::render::painter::{ButtonStyle, Painter};

pub(super) const THEME_ROW: usize = 138;
pub(super) const WALLPAPER_ROW: usize = 228;
pub(super) const FONT_ROW: usize = 270;
pub(super) const SCROLLBACK_ROW: usize = 326;
pub(super) const RESTORE_ROW: usize = 382;
pub(super) const STRENGTH_ROW: usize = 438;
pub(super) const OPACITY_ROW: usize = 494;
pub(super) const SHOW_TABS_ROW: usize = 104;
pub(super) const VERTICAL_TABS_ROW: usize = 152;
pub(super) const NEW_TAB_BUTTON_ROW: usize = 200;
pub(super) const SETTINGS_BUTTON_ROW: usize = 248;
pub(super) const BOTTOM_BAR_ROW: usize = 296;
pub(super) const COMPACT_TABS_ROW: usize = 344;
pub(super) const PADDING_ROW: usize = 392;
pub(super) const SHORTCUT_ROWS: [(ShortcutAction, usize); 7] = [
	(ShortcutAction::NewTab, 104),
	(ShortcutAction::CommandPalette, 152),
	(ShortcutAction::CycleTheme, 200),
	(ShortcutAction::Search, 248),
	(ShortcutAction::CloseTab, 296),
	(ShortcutAction::PreviousPrompt, 344),
	(ShortcutAction::NextPrompt, 392),
];
/// Top of the Fonts page's list and the distance between its rows.
pub(super) const FONT_LIST_TOP: usize = 104;
pub(super) const FONT_LIST_STRIDE: usize = 44;
/// Default and minimum Settings window size, in logical pixels.
pub(super) const DEFAULT_SIZE: (usize, usize) = (760, 640);
pub(super) const MIN_SIZE: (usize, usize) = (680, 600);
/// Height of a settings row's controls.
pub(super) const ROW_HEIGHT: usize = 40;

/// Themes in the order the Settings picker cycles through them.
pub(super) const THEMES: [ThemeKind; 10] = [
	ThemeKind::MacOsDark,
	ThemeKind::MacOsLight,
	ThemeKind::CatppuccinLatte,
	ThemeKind::CatppuccinFrappe,
	ThemeKind::CatppuccinMacchiato,
	ThemeKind::CatppuccinMocha,
	ThemeKind::Bark,
	ThemeKind::AmoledBlack,
	ThemeKind::Poop,
	ThemeKind::Custom,
];

/// Returns the theme `step` positions away from `current` in [`THEMES`].
pub(super) fn step_theme(current: ThemeKind, step: isize) -> ThemeKind {
	let index = THEMES
		.iter()
		.position(|&theme| theme == current)
		.unwrap_or(0);
	THEMES[(index as isize + step).rem_euclid(THEMES.len() as isize) as usize]
}

fn theme_label(kind: ThemeKind) -> String {
	match kind {
		ThemeKind::MacOsDark => "Dark Mode".to_owned(),
		ThemeKind::MacOsLight => "Light Mode".to_owned(),
		ThemeKind::Bark => "Bark".to_owned(),
		ThemeKind::AmoledBlack => "AMOLED Black".to_owned(),
		ThemeKind::Custom => "Custom".to_owned(),
		_ => kind.name().replace("catppuccin-", "Catppuccin "),
	}
}

fn about_release_line(version: &str, codename: &str) -> String {
	format!("Version {version} \"{codename}\"")
}

impl App {
	pub(super) fn draw_settings(&mut self) {
		let scale = self
			.settings_window
			.as_ref()
			.map_or(1, |window| window.scale_factor().round() as usize)
			.max(1);
		let Some(mut surface) = self.settings_surface.take() else {
			return;
		};
		if let Ok(mut buffer) = surface.buffer_mut() {
			let (width, height) = (self.settings_width as usize, self.settings_height as usize);
			let mut painter = Painter::new(&mut buffer, width, height);
			self.compose_settings(&mut painter, scale);
			let _ = buffer.present();
		}
		self.settings_surface = Some(surface);
	}

	fn compose_settings(&mut self, p: &mut Painter, scale: usize) {
		let (width, height) = (p.width, p.height);
		p.fill(self.theme.border);
		p.rect(
			2 * scale,
			2 * scale,
			width.saturating_sub(4 * scale),
			height.saturating_sub(4 * scale),
			self.theme.bg,
		);
		let px = 18.0 * scale as f32;
		let ascent = self
			.fonts
			.primary()
			.horizontal_line_metrics(px)
			.map_or(self.ascent, |metrics| metrics.ascent.round() as i32);
		let style = self.button_style(scale);
		let cursor = self.settings_cursor_pos;
		let hovered = |x: usize, y: usize, w: usize, h: usize| {
			cursor.0 >= x as f64
				&& cursor.0 < (x + w) as f64
				&& cursor.1 >= y as f64
				&& cursor.1 < (y + h) as f64
		};
		let editor = &self.settings_editor;
		let mut pen = Pen {
			fonts: &mut self.fonts,
			px,
			ascent,
			color: self.theme.fg,
			scale,
		};
		let x = 24 * scale;
		match editor.page {
			SettingsPage::About => {
				pen.text(p, x, 28, "About poopterm", 1.3);
				pen.text(
					p,
					x,
					104,
					&about_release_line(env!("CARGO_PKG_VERSION"), env!("POOPTERM_CODENAME")),
					0.82,
				);
				for (line, text) in [
					(148, "A small, native terminal emulator written in Rust."),
					(184, "Copyright © 2026 poopware-labs"),
					(220, "Licensed under the MIT License."),
					(256, "Source: github.com/poopterm-labs/poopterm"),
					(292, "Built with Rust, winit, and softbuffer."),
				] {
					pen.text(p, x, line, text, 0.82);
				}
				pen.back_button(p, style, &hovered);
				return;
			}
			SettingsPage::Shortcuts => {
				pen.text(p, x, 28, "Keyboard shortcuts", 1.0);
				pen.text(
					p,
					x,
					68,
					"Click a binding, then press the new key chord. Esc cancels.",
					0.72,
				);
				pen.back_button(p, style, &hovered);
				for (action, row) in SHORTCUT_ROWS {
					let recording = editor.recording_shortcut == Some(action);
					let label = if recording {
						"Press keys…".to_owned()
					} else {
						editor
							.shortcuts
							.get(action)
							.display(cfg!(target_os = "macos"))
					};
					pen.text(p, x, row + 10, action.label(), 0.78);
					pen.wide_button(p, style, row, &label, recording, &hovered);
				}
			}
			SettingsPage::Layout => {
				pen.text(p, x, 28, "Layout", 1.0);
				pen.text(p, x, 68, "Tabs, toolbar buttons, and the bottom bar.", 0.72);
				pen.back_button(p, style, &hovered);
				let prefs = editor.preferences;
				for (row, label, enabled) in [
					(SHOW_TABS_ROW, "Show tabs", prefs.show_tabs),
					(VERTICAL_TABS_ROW, "Vertical tabs", prefs.vertical_tabs),
					(
						NEW_TAB_BUTTON_ROW,
						"Show new tab button",
						prefs.show_new_tab_button,
					),
					(
						SETTINGS_BUTTON_ROW,
						"Show settings button",
						prefs.show_settings_button,
					),
					(BOTTOM_BAR_ROW, "Show bottom bar", prefs.show_bottom_bar),
					(COMPACT_TABS_ROW, "Compact tabs", prefs.compact_tabs),
				] {
					pen.text(p, x, row + 8, label, 0.82);
					pen.toggle(p, style, row, enabled);
				}
				pen.text(p, x, PADDING_ROW + 8, "Padding", 0.82);
				pen.stepper(p, style, PADDING_ROW, &format!("{} px", prefs.padding));
			}
			SettingsPage::Fonts => {
				pen.text(p, x, 28, "Fonts", 1.0);
				let visible = font_list_rows(p.height, scale);
				let fonts = &editor.fonts;
				let hint = if fonts.len() > visible {
					"Monospace fonts on this system. Scroll or use ↑ ↓ for more."
				} else {
					"Monospace fonts installed on this system."
				};
				pen.text(p, x, 68, hint, 0.72);
				pen.back_button(p, style, &hovered);
				let selected = fonts.selected();
				for row in 0..visible {
					let Some(entry) = fonts.entry_at(row) else {
						break;
					};
					let top = FONT_LIST_TOP + row * FONT_LIST_STRIDE;
					let rect = (
						x,
						top * scale,
						p.width.saturating_sub(48 * scale),
						ROW_HEIGHT * scale,
					);
					let lit = selected == Some(entry) || hovered(rect.0, rect.1, rect.2, rect.3);
					p.button(rect, style, lit);
					pen.text(p, x + 16 * scale, top + 9, fonts.label(entry), 0.82);
				}
			}
			SettingsPage::Main => compose_main_settings(editor, style, p, &mut pen, &hovered),
		}
		pen.footer(p, style, &editor.status, &hovered);
	}
}

/// The main page: theme, wallpaper, steppers and session restore.
fn compose_main_settings(
	editor: &SettingsEditor,
	style: ButtonStyle,
	p: &mut Painter,
	pen: &mut Pen,
	hovered: &dyn Fn(usize, usize, usize, usize) -> bool,
) {
	let (width, s) = (p.width, pen.scale);
	let x = 24 * s;
	let content_w = width.saturating_sub(48 * s);
	pen.text(p, x, 24, "Poopterm Settings", 1.0);
	for (_, offset, button_w, label) in HEADER_BUTTONS {
		let rect = (
			width.saturating_sub(offset * s),
			20 * s,
			button_w * s,
			36 * s,
		);
		p.button(rect, style, hovered(rect.0, rect.1, rect.2, rect.3));
		pen.centered(p, rect, label, 0.76);
	}
	pen.text(p, x, 62, "Appearance", 0.82);
	pen.text(p, x, 112, "Color theme", 0.76);

	let button_w = 48 * s;
	let theme_y = THEME_ROW * s;
	for (bx, label) in [(x, "‹"), (x + content_w - button_w, "›")] {
		p.button((bx, theme_y, button_w, ROW_HEIGHT * s), style, false);
		pen.text(p, bx + 18 * s, THEME_ROW + 7, label, 1.0);
	}
	let theme_x = x + button_w + 8 * s;
	let theme_w = content_w.saturating_sub(2 * button_w + 16 * s);
	p.button((theme_x, theme_y, theme_w, ROW_HEIGHT * s), style, false);
	pen.text(
		p,
		theme_x + 12 * s,
		THEME_ROW + 9,
		&theme_label(editor.theme),
		0.82,
	);

	pen.text(p, x, 210, "Background wallpaper", 0.76);
	for (bx, bw, label) in [
		(x, 152 * s, "Choose image…"),
		(x + 164 * s, 96 * s, "Clear"),
	] {
		p.button((bx, WALLPAPER_ROW * s, bw, 32 * s), style, false);
		pen.text(p, bx + 12 * s, WALLPAPER_ROW + 6, label, 0.68);
	}
	let wallpaper_name: String = editor
		.wallpaper_path
		.as_ref()
		.and_then(|path| path.file_name())
		.and_then(|name| name.to_str())
		.unwrap_or("No image selected")
		.chars()
		.take(32)
		.collect();
	pen.text(p, x + 272 * s, WALLPAPER_ROW + 6, &wallpaper_name, 0.66);

	let prefs = editor.preferences;
	let mut steppers = vec![
		(FONT_ROW, "Font size", prefs.font_size.to_string()),
		(
			SCROLLBACK_ROW,
			"Scrollback lines",
			prefs.scrollback_lines.to_string(),
		),
		(
			STRENGTH_ROW,
			"Wallpaper strength",
			format!("{}%", editor.wallpaper_strength),
		),
	];
	if cfg!(target_os = "macos") {
		steppers.push((OPACITY_ROW, "Window opacity", format!("{}%", prefs.opacity)));
	}
	for (row, label, value) in steppers {
		pen.text(p, x, row + 8, label, 0.82);
		pen.stepper(p, style, row, &value);
	}

	pen.text(p, x, RESTORE_ROW + 8, "Restore tabs on launch", 0.82);
	pen.toggle(p, style, RESTORE_ROW, prefs.restore_sessions);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn theme_picker_wraps_in_both_directions() {
		assert_eq!(step_theme(ThemeKind::MacOsDark, -1), ThemeKind::Custom);
		assert_eq!(step_theme(ThemeKind::Custom, 1), ThemeKind::MacOsDark);
		assert_eq!(step_theme(ThemeKind::Poop, 1), ThemeKind::Custom);
		assert_eq!(step_theme(ThemeKind::CatppuccinMocha, 1), ThemeKind::Bark);
		assert_eq!(step_theme(ThemeKind::Bark, 1), ThemeKind::AmoledBlack);
	}

	#[test]
	fn about_release_line_includes_the_version_and_codename() {
		assert_eq!(
			about_release_line("26.10.8", "Giga Fly"),
			"Version 26.10.8 \"Giga Fly\""
		);
	}
}
