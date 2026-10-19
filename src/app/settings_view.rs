//! Drawing the Settings window. Row positions are logical pixels, multiplied by
//! the window scale; `settings.rs` hit-tests clicks against the same constants.
use super::*;
use crate::render::painter::Painter;

pub(super) const THEME_ROW: usize = 138;
pub(super) const WALLPAPER_ROW: usize = 228;
pub(super) const FONT_ROW: usize = 270;
pub(super) const SCROLLBACK_ROW: usize = 326;
pub(super) const RESTORE_ROW: usize = 382;
pub(super) const STRENGTH_ROW: usize = 438;
pub(super) const OPACITY_ROW: usize = 494;
/// Height of a settings row's controls.
pub(super) const ROW_HEIGHT: usize = 40;

/// Themes in the order the Settings picker cycles through them.
pub(super) const THEMES: [ThemeKind; 8] = [
	ThemeKind::MacOsDark,
	ThemeKind::MacOsLight,
	ThemeKind::CatppuccinLatte,
	ThemeKind::CatppuccinFrappe,
	ThemeKind::CatppuccinMacchiato,
	ThemeKind::CatppuccinMocha,
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
		ThemeKind::Custom => "Custom".to_owned(),
		_ => kind.name().replace("catppuccin-", "Catppuccin "),
	}
}

/// Text drawing for one Settings frame; every label shares the base ascent.
struct Pen<'a> {
	fonts: &'a mut crate::render::glyphs::FontSet,
	px: f32,
	ascent: i32,
	color: u32,
	scale: usize,
}

impl Pen<'_> {
	/// Draws `text` at logical `(x, y)` scaled by `size` relative to the base px.
	fn text(&mut self, p: &mut Painter, x: usize, y: usize, text: &str, size: f32) {
		p.text(
			self.fonts,
			x,
			y * self.scale,
			text,
			self.px * size,
			self.ascent,
			self.color,
		);
	}
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
		let cell_width = self.fonts.primary().metrics('M', px).advance_width.ceil() as usize;
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
		let content_w = width.saturating_sub(48 * scale);
		let s = scale;

		if editor.about {
			pen.text(p, x, 28, "About poopterm", 1.3);
			for (line, text) in [
				(104, concat!("Version ", env!("CARGO_PKG_VERSION"))),
				(148, "A small, native terminal emulator written in Rust."),
				(184, "Copyright © 2026 poopware-labs"),
				(220, "Licensed under the MIT License."),
				(256, "Source: github.com/poopterm-labs/poopterm"),
				(292, "Built with Rust, winit, softbuffer, and fontdue."),
			] {
				pen.text(p, x, line, text, 0.82);
			}
			let back_x = width.saturating_sub(164 * s);
			p.button((back_x, 20 * s, 140 * s, 36 * s), style, false);
			pen.text(p, back_x + 22 * s, 27, "Settings", 0.8);
			return;
		}

		pen.text(p, x, 24, "Poopterm Settings", 1.0);
		let about_x = width.saturating_sub(140 * s);
		let about = (about_x, 20 * s, 116 * s, 36 * s);
		p.button(about, style, hovered(about.0, about.1, about.2, about.3));
		pen.text(p, about_x + 25 * s, 27, "About", 0.8);
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
			for (bx, sign) in [
				(width.saturating_sub(240 * s), "−"),
				(width.saturating_sub(72 * s), "+"),
			] {
				p.button((bx, row * s, button_w, ROW_HEIGHT * s), style, false);
				pen.text(p, bx + 16 * s, row + 7, sign, 1.0);
			}
			pen.text(p, width.saturating_sub(180 * s), row + 8, &value, 0.82);
		}

		pen.text(p, x, RESTORE_ROW + 8, "Restore tabs on launch", 0.82);
		let toggle_x = width.saturating_sub(240 * s);
		let restore = prefs.restore_sessions;
		p.button(
			(toggle_x, RESTORE_ROW * s, 216 * s, ROW_HEIGHT * s),
			style,
			restore,
		);
		let state = if restore { "Enabled" } else { "Disabled" };
		pen.text(p, toggle_x + 16 * s, RESTORE_ROW + 8, state, 0.82);

		let (save_x, save_y) = (width.saturating_sub(140 * s), height.saturating_sub(52 * s));
		let (save_w, save_h) = (120 * s, 36 * s);
		p.button(
			(save_x, save_y, save_w, save_h),
			style,
			hovered(save_x, save_y, save_w, save_h),
		);
		let save_text_x = save_x + save_w.saturating_sub(4 * cell_width) / 2;
		p.text(
			pen.fonts,
			save_text_x,
			save_y + 6 * s,
			"Save",
			px,
			ascent,
			pen.color,
		);
		let status_y = height.saturating_sub(40 * s);
		p.text(
			pen.fonts,
			x,
			status_y,
			&editor.status,
			px * 0.68,
			ascent,
			pen.color,
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn theme_picker_wraps_in_both_directions() {
		assert_eq!(step_theme(ThemeKind::MacOsDark, -1), ThemeKind::Custom);
		assert_eq!(step_theme(ThemeKind::Custom, 1), ThemeKind::MacOsDark);
		assert_eq!(step_theme(ThemeKind::Poop, 1), ThemeKind::Custom);
	}
}
