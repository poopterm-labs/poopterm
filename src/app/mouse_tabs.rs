//! Mouse handling for horizontal tab scrolling and the vertical tab sidebar.
use super::*;
use winit::event::{ElementState, MouseButton};

impl App {
	pub(super) fn is_over_vertical_sidebar(&self, x: f64) -> bool {
		x < self.sidebar_width() as f64
	}

	pub(super) fn handle_vertical_tab_wheel(&mut self, x: f64, y: f64, scroll_up: bool) -> bool {
		if !self.preferences.show_tabs
			|| !self.preferences.vertical_tabs
			|| x >= self.vertical_tab_layout().sidebar_width as f64
			|| y < self.titlebar_height as f64
		{
			return false;
		}
		let layout = self.vertical_tab_layout();
		if layout.first_visible > 0 && scroll_up {
			self.tab_scroll -= 1;
		} else if layout.first_visible + layout.visible_count < self.sessions.len() && !scroll_up {
			self.tab_scroll += 1;
		}
		self.request_redraw();
		true
	}

	pub(super) fn handle_vertical_tab_mouse(
		&mut self,
		event_loop: &ActiveEventLoop,
		state: ElementState,
		button: MouseButton,
		(x, y): (f64, f64),
	) -> bool {
		if !self.preferences.show_tabs || !self.preferences.vertical_tabs {
			return false;
		}
		let layout = self.vertical_tab_layout();
		if x >= layout.sidebar_width as f64 || y < self.titlebar_height as f64 {
			return false;
		}
		if state == ElementState::Pressed && button == MouseButton::Left {
			let inside = |(left, top, width, height): (usize, usize, usize, usize)| {
				x >= left as f64
					&& x < (left + width) as f64
					&& y >= top as f64
					&& y < (top + height) as f64
			};
			if self.preferences.show_settings_button
				&& layout.settings_y.is_some_and(|y| {
					inside((
						layout.settings_x,
						y,
						layout.button_width,
						layout.button_height,
					))
				}) {
				self.open_settings_window(event_loop);
			} else if self.preferences.show_new_tab_button
				&& layout
					.plus_y
					.is_some_and(|y| inside((layout.plus_x, y, layout.button_width, layout.button_height)))
			{
				self.add_tab();
			} else if y >= layout.tab_start_y as f64 {
				let local = (y as usize - layout.tab_start_y) / layout.row_height;
				let index = layout.first_visible + local;
				if local < layout.visible_count && index < self.sessions.len() {
					let row_x = 6 * layout.scale;
					if self.sessions.len() > 1
						&& x >= (row_x + layout.tab_width.saturating_sub(24 * layout.scale)) as f64
					{
						self.close_tab(index, event_loop);
					} else {
						self.activate_tab(index);
					}
				}
			} else if let Some(window) = &self.window {
				let _ = window.drag_window();
			}
			self.request_redraw();
		}
		true
	}
}
