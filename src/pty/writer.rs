//! Non-blocking input to the shell.
//!
//! Writes go through a channel to a dedicated thread. A blocking write on
//! the UI thread could deadlock: a large paste into a program that is busy
//! writing output fills the PTY in both directions while the UI thread,
//! the only consumer of that output, waits on the write.
use std::{
	io::Write,
	sync::mpsc::{self, Sender},
	thread,
};

/// Cloneable: the UI thread sends input and the parser thread sends replies.
#[derive(Clone)]
pub(crate) struct PtyWriter {
	sender: Sender<Vec<u8>>,
}

impl PtyWriter {
	pub(crate) fn spawn(mut sink: Box<dyn Write + Send>) -> Self {
		let (sender, receiver) = mpsc::channel::<Vec<u8>>();
		thread::Builder::new()
			.name("pty-writer".into())
			.spawn(move || {
				for bytes in receiver {
					if sink.write_all(&bytes).and_then(|()| sink.flush()).is_err() {
						break;
					}
				}
			})
			.expect("spawn PTY writer thread");
		Self { sender }
	}

	/// Queues `bytes` for the shell without waiting for it to read them.
	pub(crate) fn write(&self, bytes: &[u8]) {
		if !bytes.is_empty() {
			let _ = self.sender.send(bytes.to_vec());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{
		io::Read,
		time::{Duration, Instant},
	};

	#[test]
	fn writes_return_immediately_even_when_the_shell_is_not_reading() {
		let (mut reader, sink) = std::io::pipe().unwrap();
		let writer = PtyWriter::spawn(Box::new(sink));
		let payload: Vec<u8> = (0..1 << 20).map(|i| (i % 251) as u8).collect();
		let start = Instant::now();
		writer.write(&payload);
		writer.write(b"tail");
		assert!(
			start.elapsed() < Duration::from_millis(100),
			"write blocked the caller"
		);
		let mut received = vec![0; payload.len() + 4];
		reader.read_exact(&mut received).unwrap();
		assert_eq!(&received[..payload.len()], payload.as_slice());
		assert_eq!(&received[payload.len()..], b"tail");
	}
}
