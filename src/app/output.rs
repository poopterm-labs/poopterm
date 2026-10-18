//! Parsing PTY output and deciding when the screen needs a new frame.
use super::*;
use std::time::Instant;

impl App {
	/// Reacts to output the parser thread applied to session `id`: clipboard
	/// writes, tab markers, and a paced redraw if anything visible changed.
	pub(super) fn process_output(&mut self, id: u64) {
		let active_id = self.sessions.get(self.active).map(|session| session.id);
		let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) else {
			return;
		};
		session.acknowledge_output();
		let unseen = Some(id) != active_id || !self.focused;
		let (clipboard, long_command_finished, now, synchronized) = {
			let mut term = session.term();
			let clipboard = term.clipboard_write.take();
			// A long command finished where the user may not be looking: mark
			// the tab and ask the window manager for attention.
			let long_command_finished = term.shell.take_long_command_finished() && unseen;
			if long_command_finished {
				term.bell = true;
			}
			(
				clipboard,
				long_command_finished,
				Seen::of(&term),
				term.synchronized.is_some(),
			)
		};
		if let Some(text) = clipboard
			&& let Err(error) = clipboard_write(&text)
		{
			eprintln!("poopterm: OSC 52 clipboard write failed: {error}");
		}
		if long_command_finished
			&& !self.focused
			&& let Some(window) = &self.window
		{
			window.request_user_attention(Some(winit::window::UserAttentionType::Informational));
		}
		let seen = std::mem::replace(&mut session.seen, now);
		let cwd_changed = now.cwd != seen.cwd;
		let mut tab_changed = now.bell != seen.bell || now.title != seen.title;
		let is_active = Some(id) == active_id;
		let visible = is_active && !synchronized;
		if is_active {
			self.pacer.set_flooding(session.flooding());
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
