use super::*;

impl App {
	pub(super) fn draw_settings(&mut self) {
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
			"Background wallpaper",
			px * 0.76,
			ascent,
			self.theme.fg,
			font,
			glyphs,
		);
		for (bx, bw, label) in [
			(x, 152 * scale, "Choose image…"),
			(x + 164 * scale, 96 * scale, "Clear"),
		] {
			draw_kde_button(
				pixels,
				width,
				height,
				bx,
				228 * scale,
				bw,
				32 * scale,
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
				bx + 12 * scale,
				234 * scale,
				label,
				px * 0.68,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
		}
		let wallpaper_name = self
			.settings_editor
			.wallpaper_path
			.as_ref()
			.and_then(|path| path.file_name())
			.and_then(|name| name.to_str())
			.unwrap_or("No image selected");
		let wallpaper_name: String = wallpaper_name.chars().take(32).collect();
		draw_text(
			pixels,
			width,
			height,
			x + 272 * scale,
			234 * scale,
			&wallpaper_name,
			px * 0.66,
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
		{
			let row = 438;
			draw_text(
				pixels,
				width,
				height,
				x,
				(row + 8) * scale,
				"Wallpaper strength",
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
				&format!("{}%", self.settings_editor.wallpaper_strength),
				px * 0.82,
				ascent,
				self.theme.fg,
				font,
				glyphs,
			);
		}
		#[cfg(target_os = "macos")]
		{
			let row = 494;
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
}
