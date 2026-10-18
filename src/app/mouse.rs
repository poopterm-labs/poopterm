use super::*;
use crate::input::paste::paste_bytes;
use crate::input::mouse::{MouseAction, MouseButtonCode, MouseReport, encode_mouse};
use winit::{dpi::PhysicalPosition, event::MouseScrollDelta};

impl App {
	pub(super) fn point_at(&self, x: f64, y: f64) -> Option<CellPoint> {
		let term = self.active_term();
		if x < self.margin as f64
			|| x >= (self.margin + term.cols * self.cw) as f64
			|| y < self.top as f64
			|| y >= (self.height as usize).saturating_sub(self.status_height + self.margin) as f64
		{
			return None;
		}
		let col = ((x as usize - self.margin) / self.cw.max(1)).min(term.cols - 1);
		let row = ((y as usize - self.top) / self.ch.max(1)).min(term.rows - 1);
		Some(CellPoint {
			row: term.scrollback.len().saturating_sub(term.scroll_offset) + row,
			col,
		})
	}

	pub(super) fn selected_text(&self) -> Option<String> {
		let selection = self.selection?;
		let (start, end) = selection.ordered();
		let term = self.active_term();
		let history_len = term.scrollback.len();
		let mut result = String::new();
		for row in start.row..=end.row {
			let cells = if row < history_len {
				term.scrollback.get(row)?.as_slice()
			} else {
				let screen_row = row - history_len;
				if screen_row >= term.rows {
					break;
				}
				let begin = screen_row * term.cols;
				&term.cells[begin..begin + term.cols]
			};
			let from = if row == start.row { start.col } else { 0 };
			let to = if row == end.row {
				end.col
			} else {
				cells.len().saturating_sub(1)
			};
			for cell in cells.iter().take(to + 1).skip(from) {
				if cell.width != 0 {
					result.push(cell.ch);
					result.extend(cell.combining.iter().take(cell.combining_len as usize));
				}
			}
			if row != end.row {
				while result.ends_with(' ') {
					result.pop();
				}
				result.push('\n');
			}
		}
		Some(result)
	}

	pub(super) fn copy_selection(&self) {
		if let Some(text) = self.selected_text()
			&& let Err(error) = clipboard_write(&text)
		{
			eprintln!("poopterm: clipboard copy failed: {error}");
		}
	}

	pub(super) fn token_at(&self, point: CellPoint) -> Option<String> {
		let cells = self.active_term().row_cells(point.row)?;
		if point.col >= cells.len() || cells[point.col].ch.is_whitespace() {
			return None;
		}
		let is_token_char = |ch: char| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '<' | '>');
		let mut start = point.col;
		let mut end = point.col + 1;
		while start > 0 && is_token_char(cells[start - 1].ch) {
			start -= 1;
		}
		while end < cells.len() && is_token_char(cells[end].ch) {
			end += 1;
		}
		Some(
			cells[start..end]
				.iter()
				.filter(|cell| cell.width != 0)
				.flat_map(|cell| {
					std::iter::once(cell.ch).chain(
						cell
							.combining
							.iter()
							.take(cell.combining_len as usize)
							.copied(),
					)
				})
				.collect::<String>()
				.trim_matches(|ch: char| matches!(ch, ')' | ']' | '}' | ',' | '.' | ';' | ':'))
				.to_owned(),
		)
	}

	pub(super) fn open_or_copy_token(&self, x: f64, y: f64) -> bool {
		let Some(point) = self.point_at(x, y) else {
			return false;
		};
		let Some(token) = self.token_at(point) else {
			return false;
		};
		let term = self.active_term();
		let explicit = term
			.row_cells(point.row)
			.and_then(|cells| cells.get(point.col))
			.and_then(|cell| term.hyperlinks.get(cell.hyperlink as usize))
			.filter(|uri| !uri.is_empty());
		let token = explicit.map(String::as_str).unwrap_or(&token);
		if token.starts_with("https://") || token.starts_with("http://") {
			if let Err(error) = open_external(token) {
				eprintln!("poopterm: could not open link: {error}");
			}
			return true;
		}
		let file_path = token.strip_prefix("file://").and_then(|uri| {
			let path = if uri.starts_with('/') {
				Some(uri)
			} else {
				uri.strip_prefix("localhost")
			};
			path.and_then(decode_uri_path)
		});
		if let Some(path) = file_path {
			if let Err(error) = open_external(&path.to_string_lossy()) {
				eprintln!("poopterm: could not open linked file: {error}");
			}
			return true;
		}
		if token.starts_with('/')
			|| token.starts_with("~/")
			|| token.starts_with("./")
			|| token.starts_with("../")
			|| token.contains('/')
		{
			if let Err(error) = clipboard_write(token) {
				eprintln!("poopterm: could not copy path: {error}");
			}
			return true;
		}
		false
	}

	pub(super) fn paste_clipboard(&mut self) {
		match clipboard_read() {
			Ok(text) => {
				let bytes = paste_bytes(&text, self.active_term().bracketed_paste);
				self.active_session_mut().write(&bytes);
			}
			Err(error) => eprintln!("poopterm: clipboard paste failed: {error}"),
		}
	}

	/// Reports a mouse event to the application when it enabled mouse tracking.
	/// Returns true when the event belongs to the application (even if it could
	/// not be encoded), so the caller skips local selection and scrolling.
	pub(super) fn send_mouse_report(
		&mut self,
		button: MouseButtonCode,
		kind: MouseAction,
		(x, y): (f64, f64),
	) -> bool {
		let Some(point) = self.point_at(x, y) else {
			return false;
		};
		let term = self.active_term();
		let wants = match kind {
			MouseAction::Motion if button == MouseButtonCode::None => term.mouse_tracking == 1003,
			MouseAction::Motion => matches!(term.mouse_tracking, 1002 | 1003),
			_ => term.mouse_tracking != 0,
		};
		if !wants {
			return false;
		}
		let visible_start = term.scrollback.len().saturating_sub(term.scroll_offset);
		let report = MouseReport {
			button,
			kind,
			col: point.col + 1,
			row: point.row.saturating_sub(visible_start) + 1,
			mods: self.mods,
		};
		if let Some(bytes) = encode_mouse(report, term.mouse_sgr) {
			self.active_session_mut().write(&bytes);
		}
		true
	}

	pub(super) fn begin_selection(&mut self, x: f64, y: f64) {
		if let Some(point) = self.point_at(x, y) {
			self.selection = Some(Selection {
				start: point,
				end: point,
			});
			self.selecting = true;
			if let Some(window) = &self.window {
				window.request_redraw();
			}
		}
	}

	pub(super) fn extend_selection(&mut self, x: f64, y: f64) {
		if !self.selecting {
			return;
		}
		if let Some(point) = self.point_at(x, y) {
			if let Some(selection) = &mut self.selection {
				selection.end = point;
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
		}
	}
}

impl App {
	pub(super) fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
		let (x, y) = self.cursor_pos;
		let scroll_up = match &delta {
			MouseScrollDelta::LineDelta(_, n) => *n > 0.0,
			MouseScrollDelta::PixelDelta(p) => p.y > 0.0,
		};
		if y >= self.tab_y() as f64 && y < self.top as f64 {
			let layout = self.tab_layout();
			if layout.first_visible > 0 && scroll_up {
				self.tab_scroll -= 1;
			} else if layout.first_visible + layout.visible_count < self.sessions.len() && !scroll_up {
				self.tab_scroll += 1;
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		if self.send_mouse_report(
			if scroll_up {
				MouseButtonCode::WheelUp
			} else {
				MouseButtonCode::WheelDown
			},
			MouseAction::Press,
			(x, y),
		) {
			return;
		}
		let lines = match delta {
			MouseScrollDelta::LineDelta(_, y) => y.round() as isize,
			MouseScrollDelta::PixelDelta(p) => (p.y / 24.0).round() as isize,
		};
		self.active_session_mut().term.scroll_view(lines);
		if let Some(win) = &self.window {
			win.request_redraw();
		}
	}

	pub(super) fn handle_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
		let previous = self.cursor_pos;
		self.cursor_pos = (position.x, position.y);
		if self.selecting {
			self.extend_selection(position.x, position.y);
		} else {
			let button = self.held_button.unwrap_or(MouseButtonCode::None);
			self.send_mouse_report(button, MouseAction::Motion, self.cursor_pos);
		}
		if (previous.1 < self.top as f64 || position.y < self.top as f64)
			&& let Some(window) = &self.window
		{
			window.request_redraw();
		}
	}

	pub(super) fn handle_mouse_input(
		&mut self,
		event_loop: &ActiveEventLoop,
		state: ElementState,
		button: MouseButton,
	) {
		let (x, y) = self.cursor_pos;
		let code = match button {
			MouseButton::Left => MouseButtonCode::Left,
			MouseButton::Middle => MouseButtonCode::Middle,
			MouseButton::Right => MouseButtonCode::Right,
			_ => return,
		};
		self.held_button = (state == ElementState::Pressed).then_some(code);
		let tab_y = self.tab_y() as f64;
		if state == ElementState::Pressed && button == MouseButton::Left && y < self.top as f64 {
			let layout = self.tab_layout();
			let in_tabbar = y >= tab_y && y < self.top as f64;
			if in_tabbar
				&& x >= layout.settings_x as f64
				&& x < (layout.settings_x + layout.settings_w) as f64
			{
				self.open_settings_window(event_loop);
			} else if in_tabbar && x >= layout.plus_x as f64 && x < (layout.plus_x + layout.plus_w) as f64
			{
				self.add_tab();
			} else if in_tabbar {
				if x >= layout.left_arrow_x as f64
					&& x < (layout.left_arrow_x + layout.arrow_w) as f64
					&& layout.first_visible > 0
				{
					self.tab_scroll = self.tab_scroll.saturating_sub(1);
				} else if x >= layout.right_arrow_x as f64
					&& x < (layout.right_arrow_x + layout.arrow_w) as f64
					&& layout.first_visible + layout.visible_count < self.sessions.len()
				{
					self.tab_scroll += 1;
				}
				for local_index in 0..layout.visible_count {
					let index = layout.first_visible + local_index;
					if index >= self.sessions.len() {
						break;
					}
					let left = layout.start + local_index * (layout.width + layout.gap);
					if x >= left as f64 && x < (left + layout.width) as f64 {
						if self.sessions.len() > 1
							&& x >= (left + layout.width.saturating_sub(28 * layout.scale)) as f64
						{
							self.close_tab(index, event_loop);
						} else {
							self.activate_tab(index);
						}
						break;
					}
				}
			}
			if let Some(window) = &self.window {
				window.request_redraw();
			}
		} else if state == ElementState::Pressed
			&& button == MouseButton::Left
			&& y < self.titlebar_height as f64
		{
			if let Some(window) = &self.window {
				let _ = window.drag_window();
			}
		} else {
			let kind = if state == ElementState::Pressed {
				MouseAction::Press
			} else {
				MouseAction::Release
			};
			let handled = self.send_mouse_report(code, kind, (x, y));
			if state == ElementState::Pressed && button == MouseButton::Left && !handled {
				let link_click = (self.mods.ctrl || self.mods.super_key)
					&& self.active_term().mouse_tracking == 0
					&& self.open_or_copy_token(x, y);
				if !link_click {
					self.begin_selection(x, y);
				}
			}
			if state == ElementState::Released {
				self.selecting = false;
			}
		}
	}
}
