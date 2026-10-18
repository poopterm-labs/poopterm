//! The tab strip: tabs with activity markers, scroll arrows, new-tab and settings buttons.
use super::{draw::UiFont, *};
use crate::render::painter::Painter;

fn hit(cursor: (f64, f64), (x, y, w, h): (usize, usize, usize, usize)) -> bool {
	cursor.0 >= x as f64
		&& cursor.0 < (x + w) as f64
		&& cursor.1 >= y as f64
		&& cursor.1 < (y + h) as f64
}

impl App {
	pub(super) fn draw_tab_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let tab_y = self.tab_y();
		let layout = self.tab_layout();
		let (bg, border, fg) = (self.theme.bg, self.theme.border, self.theme.fg);
		let accent = self.theme.palette[4];
		let text_y = tab_y + self.tab_height.saturating_sub(ui.ch) / 2;
		let small_ascent = (ui.ascent as f32 * 0.82) as i32;
		p.rect(0, tab_y, p.width, self.tab_height, bg);
		if !cfg!(target_os = "macos") {
			p.text(
				&mut self.fonts,
				16 * scale,
				text_y,
				"poopterm",
				ui.px * 0.82,
				ui.ascent,
				fg,
			);
		}
		p.rect(
			0,
			tab_y + self.tab_height.saturating_sub(scale),
			p.width,
			scale,
			blend(bg, border, 90),
		);

		let visible = layout.first_visible
			..self
				.sessions
				.len()
				.min(layout.first_visible + layout.visible_count);
		for index in visible {
			let x = layout.start + (index - layout.first_visible) * (layout.width + layout.gap);
			let active = index == self.active;
			let hovered = hit(
				self.cursor_pos,
				(x, tab_y, layout.width, self.top.saturating_sub(tab_y)),
			);
			if active || hovered {
				let shade = if active { 70 } else { 38 };
				p.round_rect(
					x,
					tab_y + 4 * scale,
					layout.width,
					self.tab_height.saturating_sub(6 * scale),
					5 * scale,
					blend(bg, border, shade),
				);
			}
			if active {
				p.round_rect(
					x + 8 * scale,
					tab_y + self.tab_height.saturating_sub(5 * scale),
					layout.width.saturating_sub(16 * scale),
					3 * scale,
					scale,
					accent,
				);
			}
			let session = &self.sessions[index];
			let max_chars = (layout.width.saturating_sub(42 * scale) / ui.cw.max(1)).max(1);
			let label = ellipsize(&session.title(), max_chars);
			let marker = if session.term().bell {
				Some("!")
			} else if session.activity {
				Some("•")
			} else {
				None
			};
			if let Some(marker) = marker {
				p.text(
					&mut self.fonts,
					x + 10 * scale,
					text_y,
					marker,
					ui.px * 0.62,
					ui.ascent,
					accent,
				);
			}
			let label_x = x + if session.activity { 22 } else { 12 } * scale;
			p.text(
				&mut self.fonts,
				label_x,
				text_y,
				&label,
				ui.px * 0.82,
				small_ascent,
				fg,
			);
			if self.sessions.len() > 1 {
				p.text(
					&mut self.fonts,
					x + layout.width.saturating_sub(28 * scale),
					text_y,
					"×",
					ui.px * 0.82,
					small_ascent,
					fg,
				);
			}
		}

		let style = self.button_style(scale);
		let arrow_h = 30 * scale;
		let arrow_y = tab_y + self.tab_height.saturating_sub(arrow_h) / 2;
		let arrows = [
			(layout.left_arrow_x, "‹", layout.first_visible > 0),
			(
				layout.right_arrow_x,
				"›",
				layout.first_visible + layout.visible_count < self.sessions.len(),
			),
		];
		for (x, label, enabled) in arrows {
			if !enabled {
				continue;
			}
			let rect = (x, arrow_y, layout.arrow_w, arrow_h);
			p.button(rect, style, hit(self.cursor_pos, rect));
			p.text(
				&mut self.fonts,
				x + layout.arrow_w.saturating_sub(ui.cw) / 2,
				arrow_y + arrow_h.saturating_sub(ui.ch) / 2,
				label,
				ui.px * 0.82,
				small_ascent,
				fg,
			);
		}

		let button_h = 34 * scale;
		let button_y = tab_y + self.tab_height.saturating_sub(button_h) / 2;
		for (x, w, label) in [
			(layout.plus_x, layout.plus_w, "+"),
			(layout.settings_x, layout.settings_w, "⚙"),
		] {
			let rect = (x, button_y, w, button_h);
			p.button(rect, style, hit(self.cursor_pos, rect));
			p.text(
				&mut self.fonts,
				x + w.saturating_sub(ui.cw) / 2,
				button_y + button_h.saturating_sub(ui.ch) / 2,
				label,
				ui.px,
				ui.ascent,
				fg,
			);
		}
	}
}

/// Truncates `title` to `max_chars` characters, ending in `…` when shortened.
pub(super) fn ellipsize(title: &str, max_chars: usize) -> String {
	let max_chars = max_chars.max(1);
	if title.chars().count() <= max_chars {
		return title.to_owned();
	}
	let mut label: String = title.chars().take(max_chars).collect();
	if max_chars > 1 {
		label.pop();
		label.push('…');
	}
	label
}

#[cfg(test)]
mod tests {
	use super::ellipsize;

	#[test]
	fn long_titles_are_ellipsized_to_fit() {
		assert_eq!(ellipsize("vim main.rs", 20), "vim main.rs");
		assert_eq!(ellipsize("vim main.rs", 5), "vim …");
		assert_eq!(ellipsize("vim", 1), "v");
	}
}
