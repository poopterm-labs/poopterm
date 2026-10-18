use super::*;

impl App {
	pub(super) fn search(&mut self, direction: isize, advance: bool) {
		self.search_dirty = false;
		let matches = find_matches(self.active_term(), &self.search_query);
		self.search_matches = matches;
		if self.search_matches.is_empty() {
			self.search_match_index = 0;
			if let Some(window) = &self.window {
				window.request_redraw();
			}
			return;
		}
		let len = self.search_matches.len();
		if advance {
			self.search_match_index =
				(self.search_match_index as isize + direction).rem_euclid(len as isize) as usize;
		} else {
			self.search_match_index = 0;
		}
		let match_range = self.search_matches[self.search_match_index];
		let term = &mut self.active_session_mut().term;
		let visible_top = match_range.start.row.saturating_sub(term.rows / 2);
		term.scroll_offset = term
			.scrollback
			.len()
			.saturating_sub(visible_top.min(term.scrollback.len()));
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}

	pub(super) fn begin_search(&mut self) {
		self.search_active = true;
		self.search_query.clear();
		self.search_matches.clear();
		self.search_match_index = 0;
		self.search(1, false);
	}

	pub(super) fn end_search(&mut self) {
		self.search_active = false;
		self.search_matches.clear();
		self.search_query.clear();
		if let Some(window) = &self.window {
			window.request_redraw();
		}
	}
}
