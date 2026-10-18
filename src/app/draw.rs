use super::*;
use crate::{
	render::{blend::blend_coverage_row, glyphs::glyph_cache_key_for},
	shell_integration, sixel,
};

impl App {
	pub(super) fn draw(&mut self) {
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
		let inner_width = self.width.saturating_sub((2 * self.border) as u32);
		let inner_height = self.height.saturating_sub((2 * self.border) as u32);
		let background = self.theme.bg;
		let wallpaper_strength = self.wallpaper_strength;
		let border = self.border;
		let wallpaper_pixels = self
			.wallpaper
			.as_mut()
			.map(|wallpaper| wallpaper.pixels(inner_width, inner_height, background, wallpaper_strength));
		let Some(surface) = self.surface.as_mut() else {
			return;
		};
		let Ok(mut buf) = surface.buffer_mut() else {
			return;
		};
		let pixels: &mut [u32] = &mut buf;
		let (w, h) = (self.width as usize, self.height as usize);
		let font = &self.font;
		let fallback_fonts = &self.fallback_fonts;
		let font_choices = &mut self.font_choices;
		let glyphs = &mut self.glyphs;

		// Caramel rim, then chocolate interior.
		pixels.fill(self.theme.border);
		if let Some(wallpaper_pixels) = wallpaper_pixels {
			let inner_width = inner_width as usize;
			let inner_height = inner_height as usize;
			for row in 0..inner_height {
				let src = row * inner_width;
				let dst = (row + border) * w + border;
				pixels[dst..dst + inner_width].copy_from_slice(&wallpaper_pixels[src..src + inner_width]);
			}
		} else {
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
		}

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
				let font_index = if font.lookup_glyph_index(cell.ch) == 0 {
					*font_choices.entry(cell.ch).or_insert_with(|| {
						fallback_fonts
							.iter()
							.position(|fallback| fallback.lookup_glyph_index(cell.ch) != 0)
							.map_or(0, |index| index + 1)
					})
				} else {
					0
				};
				let glyph_font = font_index
					.checked_sub(1)
					.and_then(|index| fallback_fonts.get(index))
					.unwrap_or(font);
				let (gm, bitmap) = glyphs
					.entry(glyph_cache_key_for(cell.ch, px, font_index))
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
					let font_index = if font.lookup_glyph_index(mark) == 0 {
						*font_choices.entry(mark).or_insert_with(|| {
							fallback_fonts
								.iter()
								.position(|fallback| fallback.lookup_glyph_index(mark) != 0)
								.map_or(0, |index| index + 1)
						})
					} else {
						0
					};
					let glyph_font = font_index
						.checked_sub(1)
						.and_then(|index| fallback_fonts.get(index))
						.unwrap_or(font);
					let (mark_metrics, mark_bitmap) = glyphs
						.entry(glyph_cache_key_for(mark, px, font_index))
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
			if term.shell.phase == shell_integration::Phase::Output {
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
