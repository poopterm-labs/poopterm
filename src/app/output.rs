//! Parsing PTY output and deciding when the screen needs a new frame.
use super::*;
use std::time::Instant;

impl App {
	/// Parses everything queued for session `id`, sends terminal replies, and
	/// schedules a paced redraw if anything visible changed.
	pub(super) fn process_output(&mut self, id: u64) {
		let active_id = self.sessions.get(self.active).map(|session| session.id);
		let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) else {
			return;
		};
		session.output.take(&mut self.read_buffer);
		let term = &session.term;
		let (bell, title, cwd) = (term.bell, term.title_generation, term.cwd_generation);
		session.parser.advance(&mut session.term, &self.read_buffer);
		if let Some(text) = session.term.clipboard_write.take()
			&& let Err(error) = clipboard_write(&text)
		{
			eprintln!("poopterm: OSC 52 clipboard write failed: {error}");
		}
		for response in std::mem::take(&mut session.term.responses) {
			session.write(&response);
		}
		let term = &session.term;
		let cwd_changed = term.cwd_generation != cwd;
		let mut tab_changed = term.bell != bell || term.title_generation != title;
		let is_active = Some(id) == active_id;
		let visible = is_active && term.synchronized.is_none();
		if is_active {
			session.activity = false;
		} else if !session.activity {
			session.activity = true;
			tab_changed = true;
		}
		if cwd_changed {
			self.persist_sessions();
		}
		if visible {
			self.update_hovered_link();
			self.search_dirty |= self.search_active;
			self.update_window_title();
		}
		if visible || tab_changed {
			self.schedule_redraw();
		}
	}

	/// Requests a frame now, or at the next frame boundary if one was just drawn.
	pub(super) fn schedule_redraw(&mut self) {
		if self.pacer.request(Instant::now()) {
			self.request_redraw();
		}
	}
}
