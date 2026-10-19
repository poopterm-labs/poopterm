//! Clipboard writes programs request (OSC 52, kitty OSC 5522), queued for
//! the UI thread, and the state of a chunked OSC 5522 write.
use std::sync::Arc;

use crate::graphics::base64::Decoder;

/// Most bytes one OSC 5522 write may carry; the protocol asks terminals to
/// accept at least 64 MiB.
pub(crate) const MAX_WRITE: usize = if cfg!(test) { 1 << 16 } else { 64 << 20 };
/// Most MIME types, aliases included, in one write.
const MAX_TYPES: usize = 64;
/// Writes waiting for the UI thread; older ones are dropped.
const MAX_QUEUED: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
	Clipboard,
	Primary,
}

/// One clipboard change: the same content in one or more MIME types.
#[derive(Clone, Debug)]
pub(crate) struct ClipboardWrite {
	pub selection: Selection,
	pub items: Vec<(String, Arc<[u8]>)>,
}

/// Why a write failed, as the `status` OSC 5522 reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteError {
	Invalid,
	TooBig,
}

impl WriteError {
	pub(crate) fn status(self) -> &'static str {
		match self {
			Self::Invalid => "EINVAL",
			Self::TooBig => "EFBIG",
		}
	}
}

/// An OSC 5522 write in progress.
#[derive(Clone, Default)]
pub(crate) struct Upload {
	pub selection: Option<Selection>,
	pub id: String,
	items: Vec<(String, Vec<u8>)>,
	/// The MIME type being received, its decoder and encoded length.
	current: Option<(String, Decoder, usize)>,
	aliases: Vec<(String, String)>,
	bytes: usize,
}

impl Upload {
	pub(crate) fn new(selection: Selection, id: String) -> Self {
		Self {
			selection: Some(selection),
			id,
			..Self::default()
		}
	}

	/// Adds a chunk of `mime` data; types arrive one after another.
	pub(crate) fn data(&mut self, mime: String, payload: &[u8]) -> Result<(), WriteError> {
		if self
			.current
			.as_ref()
			.is_none_or(|(current, ..)| *current != mime)
		{
			self.finish_type()?;
			if self.items.len() + self.aliases.len() >= MAX_TYPES {
				return Err(WriteError::TooBig);
			}
			self.items.push((mime.clone(), Vec::new()));
			self.current = Some((mime, Decoder::default(), 0));
		}
		let (_, decoder, encoded) = self.current.as_mut().unwrap();
		let data = &mut self.items.last_mut().unwrap().1;
		let before = data.len();
		decoder
			.push(payload, data)
			.map_err(|_| WriteError::Invalid)?;
		*encoded += payload.len();
		self.bytes += data.len() - before;
		if self.bytes > MAX_WRITE {
			return Err(WriteError::TooBig);
		}
		Ok(())
	}

	/// Makes `aliases` offer the same data as `target`.
	pub(crate) fn alias(&mut self, target: String, aliases: &str) {
		for alias in aliases.split(' ').filter(|alias| !alias.is_empty()) {
			if self.items.len() + self.aliases.len() >= MAX_TYPES {
				return;
			}
			self.aliases.push((target.clone(), alias.to_owned()));
		}
	}

	/// Ends the write.
	pub(crate) fn finish(mut self) -> Result<ClipboardWrite, WriteError> {
		self.finish_type()?;
		let mut items: Vec<(String, Arc<[u8]>)> = self
			.items
			.into_iter()
			.map(|(mime, data)| (mime, Arc::from(data)))
			.collect();
		for (target, alias) in self.aliases {
			if let Some(data) = items
				.iter()
				.find(|(mime, _)| *mime == target)
				.map(|(_, data)| data.clone())
			{
				items.push((alias, data));
			}
		}
		Ok(ClipboardWrite {
			selection: self.selection.unwrap_or(Selection::Clipboard),
			items,
		})
	}

	/// Ends the current type's data, which must be complete and padded.
	fn finish_type(&mut self) -> Result<(), WriteError> {
		let Some((_, mut decoder, encoded)) = self.current.take() else {
			return Ok(());
		};
		let data = &mut self.items.last_mut().unwrap().1;
		if encoded % 4 != 0 || decoder.finish(data).is_err() {
			return Err(WriteError::Invalid);
		}
		Ok(())
	}
}

impl super::Term {
	/// Queues `write` for the UI thread.
	pub(crate) fn queue_clipboard_write(&mut self, write: ClipboardWrite) {
		if self.clipboard_writes.len() >= MAX_QUEUED {
			self.clipboard_writes.remove(0);
		}
		self.clipboard_writes.push(write);
	}
}
