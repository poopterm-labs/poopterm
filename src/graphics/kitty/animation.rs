//! Kitty animations: frames composed from partial data, and the timeline
//! that picks the frame shown (`a=f`, `a=a`, `a=c`).
use std::{
	sync::Arc,
	time::{Duration, Instant},
};

use super::Error;
use crate::graphics::Bitmap;

/// Gap after a new frame when the client gives none, in milliseconds.
pub(crate) const DEFAULT_GAP: i32 = 40;
/// Most frames per image.
pub(crate) const MAX_FRAMES: usize = 1 << 16;
/// Bytes counted per frame on top of its pixels, so floods of tiny frames
/// still meet the storage quota.
pub(crate) const FRAME_OVERHEAD: usize = 64;
/// A clock this far behind restarts the timeline instead of catching up.
const STALL: Duration = Duration::from_secs(1);

/// Whether and how the timeline runs (`a=a,s=…`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Playback {
	#[default]
	Stopped,
	/// Plays to the last frame, then waits for more (`s=2`).
	Loading,
	/// Plays and loops (`s=3`).
	Looping,
}

#[derive(Clone)]
struct Frame {
	bitmap: Arc<Bitmap>,
	/// Milliseconds before the next frame; frames with 0 or less are
	/// skipped over.
	gap: i32,
}

/// The frames of one image (frame 0 is the image itself) and which one is
/// shown.
#[derive(Clone)]
pub(crate) struct Animation {
	frames: Vec<Frame>,
	current: usize,
	playback: Playback,
	/// Times to go back to the first frame; `None` loops forever.
	loops: Option<u32>,
	loops_done: u32,
	/// When the current frame ends; `None` while stopped or waiting for
	/// frames.
	due: Option<Instant>,
}

impl Animation {
	pub(crate) fn new(root: Arc<Bitmap>) -> Self {
		Self {
			frames: vec![Frame {
				bitmap: root,
				gap: 0,
			}],
			current: 0,
			playback: Playback::Stopped,
			loops: None,
			loops_done: 0,
			due: None,
		}
	}

	pub(crate) fn len(&self) -> usize {
		self.frames.len()
	}

	pub(crate) fn frame(&self, index: usize) -> Option<&Arc<Bitmap>> {
		self.frames.get(index).map(|frame| &frame.bitmap)
	}

	pub(crate) fn current_bitmap(&self) -> &Arc<Bitmap> {
		&self.frames[self.current].bitmap
	}

	#[cfg(test)]
	pub(crate) fn playback(&self) -> Playback {
		self.playback
	}

	pub(crate) fn bytes(&self) -> usize {
		let pixels: usize = self
			.frames
			.iter()
			.map(|frame| frame.bitmap.pixels.len())
			.sum();
		pixels * 4 + self.frames.len() * FRAME_OVERHEAD
	}

	/// Appends a frame; a `gap` of 0 means the default.
	pub(crate) fn push(&mut self, bitmap: Arc<Bitmap>, gap: i32, now: Instant) -> Result<(), Error> {
		if self.frames.len() >= MAX_FRAMES {
			return Err(Error::new("ENOSPC", "too many frames"));
		}
		let gap = if gap == 0 { DEFAULT_GAP } else { gap };
		self.frames.push(Frame { bitmap, gap });
		// A loading timeline waiting at the last frame goes on.
		if self.playback == Playback::Loading && self.due.is_none() {
			self.due = Some(now);
		}
		Ok(())
	}

	/// Drops the last frame, unless it is the only one.
	pub(crate) fn pop(&mut self) {
		if self.frames.len() > 1 {
			self.frames.pop();
			self.current = self.current.min(self.frames.len() - 1);
		}
	}

	/// Replaces the pixels of frame `index`.
	pub(crate) fn replace(&mut self, index: usize, bitmap: Arc<Bitmap>) {
		self.frames[index].bitmap = bitmap;
	}

	/// Sets the gap of frame `index`; 0 is ignored.
	pub(crate) fn set_gap(&mut self, index: usize, gap: i32) {
		if gap != 0 {
			self.frames[index].gap = gap;
		}
	}

	/// Shows frame `index`, which then lasts its full gap. False when there
	/// is no such frame.
	pub(crate) fn set_current(&mut self, index: usize, now: Instant) -> bool {
		if index >= self.frames.len() {
			return false;
		}
		self.current = index;
		if self.playback != Playback::Stopped {
			self.due = Some(now + self.gap(index));
		}
		true
	}

	/// Sets the loop count as `v` gives it: 1 loops forever, `n` plays `n`
	/// times; 0 is ignored.
	pub(crate) fn set_loops(&mut self, value: u32) {
		match value {
			0 => {}
			1 => self.loops = None,
			value => self.loops = Some(value - 1),
		}
	}

	pub(crate) fn set_playback(&mut self, playback: Playback, now: Instant) {
		if playback == Playback::Stopped {
			self.stop();
			return;
		}
		if self.playback == Playback::Stopped || self.due.is_none() {
			self.due = Some(now + self.gap(self.current));
		}
		self.playback = playback;
	}

	/// Drops every frame but the first and stops (`a=d,d=f`).
	pub(crate) fn truncate(&mut self) {
		self.frames.truncate(1);
		self.current = 0;
		self.stop();
	}

	fn stop(&mut self) {
		self.playback = Playback::Stopped;
		self.loops_done = 0;
		self.due = None;
	}

	fn gap(&self, index: usize) -> Duration {
		Duration::from_millis(self.frames[index].gap.max(0) as u64)
	}

	/// When the shown frame changes next, if the timeline runs.
	pub(crate) fn deadline(&self) -> Option<Instant> {
		self.due
	}

	/// Moves the timeline to `now`. Returns whether the shown frame changed.
	pub(crate) fn advance(&mut self, now: Instant) -> bool {
		let before = self.current;
		// Frames without a gap in a row; a whole loop of them would spin.
		let mut instant = 0;
		while let Some(due) = self.due
			&& due <= now
		{
			let Some(next) = self.next_frame() else {
				break;
			};
			let gap = self.gap(next);
			instant = if gap.is_zero() { instant + 1 } else { 0 };
			if instant > self.frames.len() {
				self.stop();
				break;
			}
			self.current = next;
			let start = if now - due > STALL { now } else { due };
			self.due = Some(start + gap);
		}
		self.current != before
	}

	/// The frame after the current one, counting loops; at the end, stops
	/// or waits for frames (clearing `due`).
	fn next_frame(&mut self) -> Option<usize> {
		if self.current + 1 < self.frames.len() {
			return Some(self.current + 1);
		}
		match (self.playback, self.loops) {
			(Playback::Looping, None) => Some(0),
			(Playback::Looping, Some(loops)) if self.loops_done < loops => {
				self.loops_done += 1;
				Some(0)
			}
			(Playback::Loading, _) => {
				self.due = None;
				None
			}
			_ => {
				self.stop();
				None
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	fn bitmap(width: usize, height: usize, pixel: u32) -> Bitmap {
		Bitmap::new(width, height, vec![pixel; width * height])
	}

	fn ms(millis: u64) -> Duration {
		Duration::from_millis(millis)
	}

	/// An animation of `gaps.len()` 1×1 frames whose pixel is the frame index.
	fn timeline(gaps: &[i32]) -> Animation {
		let mut animation = Animation::new(Arc::new(bitmap(1, 1, 0xff00_0000)));
		animation.set_gap(0, gaps[0]);
		for (index, &gap) in gaps.iter().enumerate().skip(1) {
			let frame = bitmap(1, 1, 0xff00_0000 | index as u32);
			animation
				.push(Arc::new(frame), gap, Instant::now())
				.unwrap();
		}
		animation
	}

	fn shown(animation: &Animation) -> u32 {
		animation.current_bitmap().pixels[0] & 0xff_ffff
	}

	#[test]
	fn looping_follows_the_gaps_and_skips_gapless_frames() {
		let start = Instant::now();
		let mut animation = timeline(&[100, -1, 50]);
		animation.set_playback(Playback::Looping, start);
		assert_eq!(animation.deadline(), Some(start + ms(100)));
		assert!(!animation.advance(start + ms(99)));
		assert!(animation.advance(start + ms(100)));
		assert_eq!(shown(&animation), 2, "frame 2 is gapless");
		assert!(animation.advance(start + ms(150)));
		assert_eq!(shown(&animation), 0);
		assert_eq!(animation.deadline(), Some(start + ms(250)));
	}

	#[test]
	fn loop_counts_stop_at_the_last_frame_and_stopping_resets_them() {
		let start = Instant::now();
		let mut animation = timeline(&[10, 10]);
		// v=2: play, then loop once more.
		animation.set_loops(2);
		animation.set_playback(Playback::Looping, start);
		for (time, frame) in [(10, 1), (20, 0), (30, 1), (40, 1), (50, 1)] {
			animation.advance(start + ms(time));
			assert_eq!(shown(&animation), frame, "at {time} ms");
		}
		assert_eq!(animation.playback(), Playback::Stopped);
		assert_eq!(animation.deadline(), None);
		animation.set_playback(Playback::Looping, start + ms(50));
		animation.advance(start + ms(60));
		assert_eq!(shown(&animation), 0, "the loop counter was reset");
	}

	#[test]
	fn loading_waits_at_the_last_frame_for_more() {
		let start = Instant::now();
		let mut animation = timeline(&[10, 10]);
		animation.set_playback(Playback::Loading, start);
		animation.advance(start + ms(10));
		animation.advance(start + ms(20));
		assert_eq!(shown(&animation), 1);
		assert_eq!(animation.deadline(), None);
		let frame = Arc::new(bitmap(1, 1, 0xff00_0002));
		animation.push(frame, 0, start + ms(35)).unwrap();
		assert_eq!(animation.deadline(), Some(start + ms(35)));
		assert!(animation.advance(start + ms(35)));
		assert_eq!(shown(&animation), 2);
		assert_eq!(
			animation.deadline(),
			Some(start + ms(35 + DEFAULT_GAP as u64))
		);
	}

	#[test]
	fn choosing_a_frame_restarts_its_gap_and_an_all_gapless_loop_stops() {
		let start = Instant::now();
		let mut animation = timeline(&[10, 30, 10]);
		animation.set_playback(Playback::Looping, start);
		assert!(animation.set_current(1, start + ms(5)));
		assert_eq!(shown(&animation), 1);
		assert_eq!(animation.deadline(), Some(start + ms(35)));
		assert!(!animation.set_current(3, start));
		let mut gapless = animation_with_gaps_only(-1);
		gapless.set_playback(Playback::Looping, start);
		gapless.advance(start);
		assert_eq!(gapless.playback(), Playback::Stopped);
	}

	fn animation_with_gaps_only(gap: i32) -> Animation {
		timeline(&[gap, gap, gap])
	}

	#[test]
	fn a_stalled_clock_restarts_instead_of_racing_through_frames() {
		let start = Instant::now();
		let mut animation = timeline(&[10, 10, 10]);
		animation.set_playback(Playback::Looping, start);
		animation.advance(start + Duration::from_secs(5));
		assert_eq!(shown(&animation), 1);
		let deadline = start + Duration::from_secs(5) + ms(10);
		assert_eq!(animation.deadline(), Some(deadline));
	}

	#[test]
	fn frames_are_counted_capped_and_truncated() {
		let mut animation = timeline(&[0, 10]);
		assert_eq!(animation.len(), 2);
		assert_eq!(animation.bytes(), 2 * (4 + FRAME_OVERHEAD));
		animation.set_playback(Playback::Looping, Instant::now());
		animation.set_current(1, Instant::now());
		animation.truncate();
		assert_eq!((animation.len(), shown(&animation)), (1, 0));
		assert_eq!(animation.playback(), Playback::Stopped);
		let mut animation = timeline(&[0]);
		for _ in 1..MAX_FRAMES {
			animation
				.push(animation.frame(0).unwrap().clone(), 0, Instant::now())
				.unwrap();
		}
		let full = animation.push(animation.frame(0).unwrap().clone(), 0, Instant::now());
		assert_eq!(full.unwrap_err().code, "ENOSPC");
	}
}
