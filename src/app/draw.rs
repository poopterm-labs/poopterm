//! Main-window frame composition: background, tab bar, grid, overlays, status bar.
use super::*;
use crate::{
	render::{
		grid::{GridView, draw_grid},
		painter::{ButtonStyle, Painter},
	},
	shell_integration::Phase,
};

/// Commands offered by the command palette, in display order.
pub(super) const PALETTE_COMMANDS: [&str; 6] = [
	"New tab",
	"Settings",
	"About poopterm",
	"Cycle theme",
	"Close tab",
	"Rename active tab",
];

/// Metrics for toolbar text, which keeps its size regardless of the terminal font.
#[derive(Clone, Copy)]
pub(super) struct UiFont {
	pub px: f32,
	pub cw: usize,
	pub ch: usize,
	pub ascent: i32,
}

impl App {
	pub(super) fn scale(&self) -> usize {
		self
			.window
			.as_ref()
			.map_or(1.0, |w| w.scale_factor())
			.round()
			.max(1.0) as usize
	}

	pub(super) fn ui_font(&self, px: f32) -> UiFont {
		let font = self.fonts.primary();
		let metrics = font.horizontal_line_metrics(px).expect("font metrics");
		UiFont {
			px,
			cw: font.metrics('M', px).advance_width.ceil() as usize,
			ch: metrics.new_line_size.ceil() as usize,
			ascent: metrics.ascent.round() as i32,
		}
	}

	pub(super) fn button_style(&self, scale: usize) -> ButtonStyle {
		ButtonStyle {
			scale,
			background: self.theme.bg,
			border: self.theme.border,
			accent: self.theme.palette[4],
		}
	}

	pub(super) fn draw(&mut self) {
		self.pacer.drawn(std::time::Instant::now());
		self.refresh_search_if_dirty();
		self.fonts.trim();
		let Some(mut surface) = self.surface.take() else {
			return;
		};
		if let Ok(mut buffer) = surface.buffer_mut() {
			let mut painter = Painter::new(&mut buffer, self.width as usize, self.height as usize);
			self.compose(&mut painter);
			let _ = buffer.present();
		}
		self.surface = Some(surface);
	}

	fn compose(&mut self, p: &mut Painter) {
		let scale = self.scale();
		let ui = self.ui_font(18.0 * scale as f32);
		self.draw_background(p);
		self.draw_tab_bar(p, scale, ui);
		self.draw_terminal(p, scale);
		if self.search_active {
			self.draw_search_box(p, scale, ui);
		}
		if self.command_palette {
			self.draw_command_palette(p, scale, ui);
		}
		self.draw_status_bar(p, scale, ui);
	}

	/// Border rim, then the wallpaper or the theme background inside it.
	fn draw_background(&mut self, p: &mut Painter) {
		p.fill(self.theme.border);
		let border = self.border;
		let inner_w = p.width.saturating_sub(2 * border);
		let inner_h = p.height.saturating_sub(2 * border);
		let (bg, strength) = (self.theme.bg, self.wallpaper_strength);
		match self.wallpaper.as_mut() {
			Some(wallpaper) => {
				let image = wallpaper.pixels(inner_w as u32, inner_h as u32, bg, strength);
				let inner_w = inner_w.max(1);
				for (row, src) in image.chunks_exact(inner_w).take(inner_h).enumerate() {
					let dst = (row + border) * p.width + border;
					p.pixels[dst..dst + inner_w].copy_from_slice(src);
				}
			}
			None => p.rect(border, border, inner_w, inner_h, bg),
		}
	}

	fn draw_terminal(&mut self, p: &mut Painter, scale: usize) {
		let term = &self.sessions[self.active].term;
		let view = GridView {
			term,
			origin: (self.margin, self.top),
			cell: (self.cw, self.ch),
			px: self.px,
			ascent: self.ascent,
			scale,
			default_bg: self.theme.bg,
			link_color: self.theme.palette[4],
			selection_colors: (0x1E1E2E, 0x89B4FA),
			match_colors: (self.theme.bg, self.theme.palette[3]),
			selection: self.selection.map(Selection::ordered),
			search_match: self
				.search_matches
				.get(self.search_match_index)
				.map(|selection| selection.ordered()),
		};
		draw_grid(p, &mut self.fonts, &view);
	}

	/// Draws a bordered, rounded panel and returns nothing; used by overlays.
	fn panel(&self, p: &mut Painter, (x, y, w, h): (usize, usize, usize, usize), radius: usize) {
		let scale = self.scale();
		p.round_rect(x, y, w, h, radius, self.theme.border);
		p.round_rect(
			x + scale,
			y + scale,
			w.saturating_sub(2 * scale),
			h.saturating_sub(2 * scale),
			radius.saturating_sub(scale),
			self.theme.bg,
		);
	}

	fn draw_search_box(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let box_w = (500 * scale).min(p.width.saturating_sub(24 * scale));
		let box_h = 42 * scale;
		let x = p.width.saturating_sub(box_w + 16 * scale);
		let y = self.top + 12 * scale;
		self.panel(p, (x, y, box_w, box_h), 8 * scale);
		let status = if self.search_matches.is_empty() {
			"0 matches".to_owned()
		} else {
			format!(
				"{}/{}",
				self.search_match_index + 1,
				self.search_matches.len()
			)
		};
		let available = (box_w / ui.cw.max(1)).saturating_sub(25);
		let skip = self.search_query.chars().count().saturating_sub(available);
		let query: String = self.search_query.chars().skip(skip).collect();
		let label = format!("Find: {query}  {status}  Enter / Esc");
		let text_y = y + box_h.saturating_sub(ui.ch) / 2;
		let color = self.theme.fg;
		p.text(
			&mut self.fonts,
			x + 12 * scale,
			text_y,
			&label,
			ui.px * 0.82,
			ui.ascent,
			color,
		);
	}

	fn draw_command_palette(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let box_w = (520 * scale).min(p.width.saturating_sub(24 * scale));
		let x = p.width.saturating_sub(box_w) / 2;
		let y = self.top + 20 * scale;
		self.panel(p, (x, y, box_w, 210 * scale), 10 * scale);
		let color = self.theme.fg;
		let label = if self.renaming_tab {
			format!("Rename active tab: {}", self.command_query)
		} else {
			format!("Run command: {}", self.command_query)
		};
		p.text(
			&mut self.fonts,
			x + 16 * scale,
			y + 14 * scale,
			&label,
			ui.px * 0.86,
			ui.ascent,
			color,
		);
		if self.renaming_tab {
			return;
		}
		let query = self.command_query.to_ascii_lowercase();
		let matches = PALETTE_COMMANDS
			.into_iter()
			.filter(|command| command.to_ascii_lowercase().contains(&query));
		for (index, command) in matches.enumerate() {
			p.text(
				&mut self.fonts,
				x + 20 * scale,
				y + (54 + index * 28) * scale,
				command,
				ui.px * 0.78,
				ui.ascent,
				color,
			);
		}
	}

	fn draw_status_bar(&mut self, p: &mut Painter, scale: usize, ui: UiFont) {
		let status_y = p.height.saturating_sub(self.status_height + self.border);
		let (bg, border) = (self.theme.bg, self.theme.border);
		p.rect(
			0,
			status_y,
			p.width,
			self.status_height,
			blend(bg, border, 28),
		);
		p.rect(0, status_y, p.width, scale, blend(bg, border, 100));
		let term = &self.sessions[self.active].term;
		let cwd = term
			.cwd
			.file_name()
			.and_then(|name| name.to_str())
			.unwrap_or("~");
		let left: String = cwd.chars().take(p.width / ui.cw.max(1) / 3).collect();
		let command = match term.shell.phase {
			Phase::Output => "   ·   running".to_owned(),
			_ => term
				.shell
				.last_exit_code
				.map(|code| format!("   ·   exit {code}"))
				.unwrap_or_default(),
		};
		let right = format!(
			"{} × {}   ·   {}{command}",
			term.cols,
			term.rows,
			self.theme.kind.name()
		);
		let text_y = status_y + self.status_height.saturating_sub(ui.ch) / 2;
		let right_x = p
			.width
			.saturating_sub(right.chars().count() * (ui.cw * 3 / 4) + 16 * scale);
		let color = self.theme.fg;
		let px = ui.px * 0.72;
		p.text(
			&mut self.fonts,
			16 * scale,
			text_y,
			&left,
			px,
			ui.ascent,
			color,
		);
		p.text(
			&mut self.fonts,
			right_x,
			text_y,
			&right,
			px,
			ui.ascent,
			color,
		);
	}
}
