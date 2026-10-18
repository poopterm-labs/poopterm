use super::*;
use crate::input::keys::Modifiers;
use winit::{
	application::ApplicationHandler,
	dpi::{LogicalSize, PhysicalSize},
	event::Ime,
	event_loop::ControlFlow,
	window::WindowId,
};

impl ApplicationHandler<UserEvent> for App {
	fn resumed(&mut self, event_loop: &ActiveEventLoop) {
		if self.window.is_some() {
			return;
		}

		let attrs = Window::default_attributes()
			.with_title("💩 poopterm")
			.with_inner_size(LogicalSize::new(900.0, 560.0))
			.with_min_inner_size(LogicalSize::new(480.0, 240.0));

		// On macOS, keep the native frame so the traffic lights exist,
		// but make the title bar transparent and hide the title text.
		#[cfg(target_os = "macos")]
		let attrs = attrs
			.with_titlebar_transparent(true)
			.with_title_hidden(true)
			.with_fullsize_content_view(true);

		let window = Arc::new(
			event_loop
				.create_window(attrs)
				.expect("failed to create window"),
		);
		#[cfg(target_os = "macos")]
		crate::platform::set_window_opacity(&window, Preferences::load().opacity);

		let scale = window.scale_factor() as f32;
		self.margin = (8.0 * scale) as usize;
		self.border = scale as usize;
		self.titlebar_height = if cfg!(target_os = "macos") {
			(44.0 * scale) as usize
		} else {
			0
		};
		self.tab_height = (if cfg!(target_os = "macos") {
			40.0
		} else {
			44.0
		} * scale) as usize;
		self.top = self.titlebar_height
			+ if cfg!(target_os = "macos") {
				0
			} else {
				self.tab_height
			}
			+ self.margin;
		self.status_height = (26.0 * scale) as usize;

		self.set_font_px(Preferences::load().font_size as f32 * scale);

		let size = PhysicalSize::new(
			(self.cw * self.active_term().cols + 2 * self.margin) as u32,
			(self.ch * self.active_term().rows + self.top + self.margin + self.status_height) as u32,
		);
		let _ = window.request_inner_size(size);

		let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
		let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
		self.surface = Some(surface);
		self._context = Some(context);
		self.window = Some(window);
		self.update_window_title();

		let s = self.window.as_ref().unwrap().inner_size();
		self.resize(s.width, s.height);
	}

	fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
		if self
			.settings_window
			.as_ref()
			.is_some_and(|window| window.id() == id)
		{
			self.settings_window_event(event);
			return;
		}
		match event {
			WindowEvent::CloseRequested => {
				self.refresh_directories(false);
				self.persist_sessions();
				event_loop.exit();
			}
			WindowEvent::ModifiersChanged(m) => {
				let state = m.state();
				self.mods = Modifiers {
					ctrl: state.control_key(),
					shift: state.shift_key(),
					alt: state.alt_key(),
					super_key: state.super_key(),
				};
			}
			WindowEvent::MouseWheel { delta, .. } => self.handle_mouse_wheel(delta),
			WindowEvent::Ime(Ime::Commit(text)) => {
				let bytes = text.into_bytes();
				self.active_session_mut().write(&bytes);
			}
			WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
				self.handle_key(event_loop, event)
			}
			WindowEvent::CursorMoved { position, .. } => self.handle_cursor_moved(position),
			WindowEvent::MouseInput { state, button, .. } => {
				self.handle_mouse_input(event_loop, state, button)
			}
			WindowEvent::Resized(size) => self.resize(size.width, size.height),
			WindowEvent::RedrawRequested => self.draw(),
			_ => {}
		}
	}

	fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
		let now = std::time::Instant::now();
		let mut changed = false;
		for session in &mut self.sessions {
			changed |= session.term.finish_sync_if_expired(now);
		}
		if changed {
			self.persist_sessions();
			self.search_dirty = true;
			self.update_window_title();
			if let Some(window) = &self.window {
				window.request_redraw();
			}
		}
		if let Some(deadline) = self
			.sessions
			.iter()
			.filter_map(|s| s.term.sync_deadline)
			.min()
		{
			event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
		} else {
			event_loop.set_control_flow(ControlFlow::Wait);
		}
	}

	fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
		match event {
			UserEvent::Output(id, bytes, ack) => {
				// Keep one parser per PTY stream to preserve partial escape sequences.
				let mut active_output = false;
				let mut became_active = false;
				let mut cwd_changed = false;
				let active_id = self.sessions.get(self.active).map(|session| session.id);
				if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
					let previous_bell = session.term.bell;
					let previous_title_generation = session.term.title_generation;
					let previous_cwd_generation = session.term.cwd_generation;

					session.parser.advance(&mut session.term, &bytes);
					if let Some(text) = session.term.clipboard_write.take()
						&& let Err(error) = clipboard_write(&text)
					{
						eprintln!("poopterm: OSC 52 clipboard write failed: {error}");
					}

					cwd_changed = session.term.cwd_generation != previous_cwd_generation;
					became_active |= session.term.bell != previous_bell
						|| session.term.title_generation != previous_title_generation;
					if !session.term.responses.is_empty() {
						for response in std::mem::take(&mut session.term.responses) {
							let _ = session.writer.write_all(&response);
						}
						let _ = session.writer.flush();
					}
					if Some(id) == active_id {
						active_output = session.term.synchronized.is_none();
						session.activity = false;
					} else if !session.activity {
						session.activity = true;
						became_active = true;
					}
				}
				if cwd_changed {
					self.persist_sessions();
				}
				if active_output {
					if self.search_active {
						self.search_dirty = true;
					}
					self.update_window_title();
				}
				if (active_output || became_active)
					&& let Some(win) = &self.window
				{
					win.request_redraw();
				}
				let _ = ack.send(bytes);
			}
			UserEvent::Closed(id) => {
				if let Some(index) = self.sessions.iter().position(|s| s.id == id) {
					self.close_tab(index, event_loop);
				}
			}
		}
	}
}
