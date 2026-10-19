//! Drawing for the optional vertical tab sidebar.
use super::{
	draw::UiFont,
	draw_tabs::{hit, tab_label},
	*,
};
use crate::{
	render::{blend::blend, painter::Painter},
	term::status::{Progress, ProgressState},
	ui::tabs::{Marker, tab_marker},
};

impl App {
	pub(super) fn draw_vertical_tab_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let layout = self.vertical_tab_layout();
		let (bg, border, fg) = (self.theme.bg, self.theme.border, self.theme.fg);
		let accent = self.theme.palette[4];
		let sidebar_top = if cfg!(target_os = "macos") {
			self.titlebar_height
		} else {
			self.border
		};
		let sidebar_bg = blend(bg, border, 24);
		p.rect(
			self.border,
			sidebar_top,
			layout.sidebar_width.saturating_sub(self.border),
			p.height
				.saturating_sub(sidebar_top)
				.saturating_sub(self.border),
			sidebar_bg,
		);
		p.rect(
			layout.sidebar_width.saturating_sub(self.border + scale),
			sidebar_top,
			scale,
			p.height
				.saturating_sub(sidebar_top)
				.saturating_sub(self.border),
			blend(bg, border, 100),
		);
		let visible_end = self
			.sessions
			.len()
			.min(layout.first_visible + layout.visible_count);
		for index in layout.first_visible..visible_end {
			let local_index = index - layout.first_visible;
			let x = 6 * scale;
			let y = layout.tab_start_y + local_index * layout.row_height;
			let row_h = layout.row_height.saturating_sub(4 * scale);
			let active = index == self.active;
			let rect = (x, y, layout.tab_width, row_h);
			let hovered = hit(self.cursor_pos, rect);
			let show_close = self.sessions.len() > 1 && (active || hovered);
			let session = &self.sessions[index];
			let style = session.term().tab_style.clone();
			if let Some(color) = style.color {
				let shade = if active {
					150
				} else if hovered {
					110
				} else {
					80
				};
				p.round_rect(
					x,
					y,
					layout.tab_width,
					row_h,
					5 * scale,
					blend(bg, color, shade),
				);
			} else if active || hovered {
				let shade = if active { 74 } else { 40 };
				p.round_rect(
					x,
					y,
					layout.tab_width,
					row_h,
					5 * scale,
					blend(bg, border, shade),
				);
			}
			if active {
				p.round_rect(
					x,
					y + 7 * scale,
					3 * scale,
					row_h.saturating_sub(14 * scale),
					scale,
					accent,
				);
			}
			let (marker, progress) = {
				let term = session.term();
				(
					tab_marker(term.bell, term.status.summary(), session.activity),
					term.status.shown_progress(),
				)
			};
			let dot = marker
				.map(|marker| match marker {
					Marker::Bell => ("!", accent),
					Marker::Blocked => ("•", self.theme.palette[3]),
					Marker::Error => ("•", self.theme.palette[1]),
					Marker::Done => ("•", self.theme.palette[2]),
					Marker::Working => ("•", accent),
					Marker::Activity => ("•", blend(bg, fg, 150)),
				})
				.or(style.indicator.map(|color| ("•", color)));
			let text_y = y + row_h.saturating_sub(ui.ch) / 2;
			let small_ascent = (ui.ascent as f32 * 0.82) as i32;
			if let Some((glyph, color)) = dot {
				p.text(
					&mut self.fonts,
					x + 8 * scale,
					text_y,
					glyph,
					ui.px * 0.62,
					ui.ascent,
					color,
				);
			}
			let small_cw = ui.cw as f32 * 0.76;
			let left = if dot.is_some() { 20 } else { 8 } * scale;
			let right = if show_close { 24 } else { 8 } * scale;
			let room = layout.tab_width.saturating_sub(left + right);
			let max_chars = ((room as f32 / small_cw.max(1.0)) as usize).max(1);
			let (label, status) = tab_label(&session.title(), style.status.as_deref(), max_chars);
			let label_x = x + left;
			p.text_clipped(
				&mut self.fonts,
				label_x,
				text_y,
				&label,
				ui.px * 0.76,
				small_ascent,
				fg,
				(label_x, y, room, row_h),
			);
			if !status.is_empty() {
				let status_x = label_x + ((label.chars().count() + 2) as f32 * small_cw) as usize;
				p.text_clipped(
					&mut self.fonts,
					status_x,
					text_y,
					&status,
					ui.px * 0.76,
					small_ascent,
					style.status_color.unwrap_or(blend(bg, fg, 150)),
					(label_x, y, room, row_h),
				);
			}
			if show_close {
				p.text(
					&mut self.fonts,
					x + layout.tab_width.saturating_sub(20 * scale),
					text_y,
					"×",
					ui.px * 0.82,
					small_ascent,
					fg,
				);
			}
			if let Some(progress) = progress {
				self.draw_vertical_tab_progress(
					p,
					(x, y, layout.tab_width, layout.row_height),
					scale,
					progress,
				);
			}
		}

		let style = self.button_style(scale);
		let button_text_y = |y: usize| y + layout.button_height.saturating_sub(ui.ch) / 2;
		for (x, y, label, shown) in [
			(
				layout.plus_x,
				layout.plus_y.unwrap_or(0),
				"+",
				self.preferences.show_new_tab_button,
			),
			(
				layout.settings_x,
				layout.settings_y.unwrap_or(0),
				"⚙",
				self.preferences.show_settings_button,
			),
		] {
			if !shown {
				continue;
			}
			let rect = (x, y, layout.button_width, layout.button_height);
			p.button(rect, style, hit(self.cursor_pos, rect));
			p.text(
				&mut self.fonts,
				x + layout.button_width.saturating_sub(ui.cw) / 2,
				button_text_y(y),
				label,
				ui.px,
				ui.ascent,
				fg,
			);
		}
	}

	fn draw_vertical_tab_progress(
		&self,
		p: &mut Painter,
		(x, y, width, row_height): (usize, usize, usize, usize),
		scale: usize,
		progress: Progress,
	) {
		let color = match progress.state {
			ProgressState::Error => self.theme.palette[1],
			ProgressState::Paused => self.theme.palette[3],
			_ => self.theme.palette[4],
		};
		let left = x + 8 * scale;
		let track = width.saturating_sub(16 * scale);
		let bar_y = y + row_height.saturating_sub(6 * scale);
		p.rect(
			left,
			bar_y,
			track,
			2 * scale,
			blend(self.theme.bg, color, 60),
		);
		let done = progress
			.percent
			.map_or(track, |percent| track * usize::from(percent) / 100);
		let shade = if progress.percent.is_some() { 255 } else { 140 };
		p.rect(
			left,
			bar_y,
			done,
			2 * scale,
			blend(self.theme.bg, color, shade),
		);
	}
}
