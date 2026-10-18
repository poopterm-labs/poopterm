//! PTY sessions: shell spawning and the background output reader.
mod queue;
mod writer;

use std::{env, error::Error, io::Read, path::PathBuf, sync::Arc, thread};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use winit::event_loop::EventLoopProxy;

use crate::{
	config::Preferences, parser::Parser, session_store::default_cwd, term::Term, theme::Theme,
};

pub(crate) use queue::OutputQueue;
use writer::PtyWriter;

/// Bytes a session may buffer before its reader waits for the UI thread.
const OUTPUT_LIMIT: usize = 1 << 20;
const READ_SIZE: usize = 1 << 16;

#[derive(Debug)]
pub(crate) enum UserEvent {
	/// The session with this id has new bytes in its [`OutputQueue`].
	Output(u64),
	Closed(u64),
}

pub(crate) struct Session {
	pub(crate) id: u64,
	pub(crate) term: Term,
	pub(crate) parser: Parser,
	pub(crate) custom_title: Option<String>,
	pub(crate) master: Box<dyn MasterPty + Send>,
	writer: PtyWriter,
	pub(crate) output: Arc<OutputQueue>,
	pub(crate) _child: Box<dyn Child + Send + Sync>,
	pub(crate) activity: bool,
}

impl Drop for Session {
	fn drop(&mut self) {
		// Unblock the reader thread if it is waiting for queue space.
		self.output.close();
	}
}

impl Session {
	/// Sends input bytes to the shell.
	pub(crate) fn write(&mut self, bytes: &[u8]) {
		self.writer.write(bytes);
	}

	/// The tab title: a user-chosen name, else the title set by the shell (OSC 0/2).
	pub(crate) fn title(&self) -> &str {
		self.custom_title.as_deref().unwrap_or(&self.term.title)
	}
}

pub(crate) fn spawn_session(
	id: u64,
	cols: usize,
	rows: usize,
	theme: Theme,
	proxy: EventLoopProxy<UserEvent>,
	cwd: PathBuf,
) -> Result<Session, Box<dyn Error>> {
	let cwd = if cwd.is_dir() { cwd } else { default_cwd() };
	let pty = native_pty_system();
	let pair = pty.openpty(PtySize {
		rows: rows as u16,
		cols: cols as u16,
		pixel_width: 0,
		pixel_height: 0,
	})?;
	let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
	let mut cmd = CommandBuilder::new(shell);
	cmd.env("TERM", "xterm-256color");
	cmd.env("TERM_PROGRAM", "poopterm");
	cmd.arg("-l");
	cmd.cwd(&cwd);
	let child = pair.slave.spawn_command(cmd)?;
	drop(pair.slave);

	let mut reader = pair.master.try_clone_reader()?;
	let writer = PtyWriter::spawn(pair.master.take_writer()?);
	let output = Arc::new(OutputQueue::new(OUTPUT_LIMIT));
	let queue = output.clone();
	thread::Builder::new()
		.name(format!("pty-reader-{id}"))
		.spawn(move || {
			let mut buf = vec![0u8; READ_SIZE];
			loop {
				match reader.read(&mut buf) {
					Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
					Ok(0) | Err(_) => {
						let _ = proxy.send_event(UserEvent::Closed(id));
						break;
					}
					Ok(n) => {
						if queue.push(&buf[..n]) && proxy.send_event(UserEvent::Output(id)).is_err() {
							break;
						}
					}
				}
			}
		})?;

	let mut term = Term::new(cols, rows, theme, cwd);
	term.scrollback_limit = Preferences::load().scrollback_lines;
	Ok(Session {
		id,
		term,
		parser: Parser::new(),
		custom_title: None,
		master: pair.master,
		writer,
		output,
		_child: child,
		activity: false,
	})
}
