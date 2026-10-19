//! Partial redraws: restoring the background under changed rows, drawing
//! only damaged rows, and presenting changed pixel rows to the window.
use super::{canvas::merge_bands, *};
use crate::render::{damage::GridDamage, painter::Painter};

impl App {
	/// Updates the previous frame's pixels: shifts scrolled rows, redraws
	/// damaged rows and the tab and status bars. Returns the changed pixel
	/// rows as `(start, end)` bands.
	pub(super) fn compose_damaged(
		&mut self,
		p: &mut Painter,
		damage: &GridDamage,
	) -> Vec<(usize, usize)> {
		if self.preferences.show_tabs && self.preferences.vertical_tabs {
			self.compose(p);
			return vec![(0, p.height)];
		}
		let scale = self.scale();
		let ui = self.ui_font(18.0 * scale as f32);
		let (top, ch, width) = (self.top, self.ch, p.width);
		let grid_bottom = (top + self.frame.rows * ch).min(p.height);
		let status_y = p.height.saturating_sub(self.status_height + self.border);
		let mut bands = vec![(0, top.min(p.height)), (status_y, p.height)];
		if damage.scroll > 0 {
			let shift = damage.scroll * ch;
			p.pixels
				.copy_within((top + shift) * width..grid_bottom * width, top * width);
			bands.push((top, grid_bottom));
		}
		for (row, _) in damage
			.rows
			.iter()
			.enumerate()
			.filter(|(_, damaged)| **damaged)
		{
			let y = top + row * ch;
			self.restore_background(p, y, ch);
			bands.push((y, (y + ch).min(p.height)));
		}
		self.restore_background(p, 0, top);
		self.draw_tab_bar(p, scale, ui);
		self.draw_terminal(p, scale, Some(&damage.rows));
		self.restore_background(p, status_y, self.status_height + self.border);
		self.draw_status_bar(p, scale, ui);
		merge_bands(&mut bands);
		bands
	}

	/// Repaints pixel rows `y..y + height` with the window background: the
	/// border rim, then the wallpaper or the theme background inside it.
	pub(super) fn restore_background(&mut self, p: &mut Painter, y: usize, height: usize) {
		let border = self.border;
		let (width, total) = (p.width, p.height);
		let inner_w = width.saturating_sub(2 * border);
		let inner_h = total.saturating_sub(2 * border);
		let (bg, strength) = (self.theme.bg, self.wallpaper_strength);
		let image = self
			.wallpaper
			.as_mut()
			.map(|wallpaper| wallpaper.pixels(inner_w as u32, inner_h as u32, bg, strength));
		let image_w = inner_w.max(1);
		for row in y..(y + height).min(total) {
			let line = &mut p.pixels[row * width..(row + 1) * width];
			if row < border || row >= border + inner_h {
				line.fill(self.theme.border);
				continue;
			}
			line[..border].fill(self.theme.border);
			line[border + inner_w..].fill(self.theme.border);
			let inner = &mut line[border..border + inner_w];
			match image.and_then(|image| image.chunks_exact(image_w).nth(row - border)) {
				Some(source) => inner.copy_from_slice(&source[..inner_w]),
				None => inner.fill(bg),
			}
		}
	}
}

/// Whether the window system keeps the window's pixels between presents.
/// X11 windows without backing store lose them when mapped or exposed, so
/// every present must send the whole image.
pub(super) fn keeps_contents(window: &Window) -> bool {
	use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
	!matches!(
		window.window_handle().map(|handle| handle.as_raw()),
		Ok(RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_))
	)
}

/// Copies the back buffer to the window. With `bands`, only those pixel rows
/// changed; they are copied alone when the window buffer still holds the
/// previous frame (age 1), and with `damage_only` reported as the only
/// damage.
pub(super) fn present(
	mut buffer: softbuffer::Buffer<'_, Arc<Window>, Arc<Window>>,
	pixels: &[u32],
	width: usize,
	bands: Option<&[(usize, usize)]>,
	damage_only: bool,
) {
	let len = buffer.len().min(pixels.len());
	match bands {
		Some(bands) if buffer.age() == 1 => {
			for &(start, end) in bands {
				let range = (start * width).min(len)..(end * width).min(len);
				buffer[range.clone()].copy_from_slice(&pixels[range]);
			}
			let rects: Vec<softbuffer::Rect> = bands
				.iter()
				.filter_map(|&(start, end)| {
					Some(softbuffer::Rect {
						x: 0,
						y: start as u32,
						width: NonZeroU32::new(width as u32)?,
						height: NonZeroU32::new(end.saturating_sub(start) as u32)?,
					})
				})
				.collect();
			if !damage_only {
				let _ = buffer.present();
			} else if !rects.is_empty() {
				let _ = buffer.present_with_damage(&rects);
			}
		}
		_ => {
			buffer[..len].copy_from_slice(&pixels[..len]);
			let _ = buffer.present();
		}
	}
}
