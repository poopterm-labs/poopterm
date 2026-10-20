//! Bounded DEC Sixel decoder. Coordinates are pixels. Colors get their alpha
//! when they are chosen, so the decoder paints final `0xAARRGGBB` pixels into
//! one flat buffer, which becomes the bitmap as is.
use std::simd::u32x4;

use crate::graphics::Bitmap;

const TRANSPARENT: u32 = u32::MAX;
const MAX_SIDE: usize = 4096;
const MAX_PIXELS: usize = 16 * 1024 * 1024;
pub(crate) const MAX_PAYLOAD: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Transfer {
	pub bytes: Vec<u8>,
	pub aspect: usize,
	pub transparent: bool,
	pub overflow: bool,
}
impl Transfer {
	pub fn new(macro_param: u16, background: u16) -> Self {
		Self {
			bytes: Vec::new(),
			aspect: match macro_param {
				2 => 5,
				3 | 4 => 3,
				7..=9 => 1,
				_ => 2,
			},
			transparent: background == 1,
			overflow: false,
		}
	}
	/// One byte at a time: the reference [`Transfer::extend`] is tested
	/// against.
	#[cfg(test)]
	pub fn push(&mut self, byte: u8) {
		if self.bytes.len() < MAX_PAYLOAD {
			self.bytes.push(byte);
		} else {
			self.overflow = true;
		}
	}
	/// Appends payload bytes in bulk, as [`Transfer::push`] would one by one.
	pub fn extend(&mut self, bytes: &[u8]) {
		let room = MAX_PAYLOAD.saturating_sub(self.bytes.len());
		self
			.bytes
			.extend_from_slice(&bytes[..bytes.len().min(room)]);
		self.overflow |= bytes.len() > room;
	}
	pub fn finish(self, background: u32, foreground: u32) -> Option<Bitmap> {
		if self.overflow {
			return None;
		}
		Decoder::new(
			self.aspect,
			if self.transparent {
				TRANSPARENT
			} else {
				background
			},
			foreground,
		)
		.decode(&self.bytes)
	}
}

struct Decoder {
	/// Rows of `stride` pixels; columns from `width` on are unused.
	pixels: Vec<u32>,
	stride: usize,
	width: usize,
	height: usize,
	x: usize,
	y: usize,
	aspect: usize,
	background: u32,
	colors: [u32; 256],
	selected: usize,
	painted: usize,
}
/// A decoding color as a pixel: [`TRANSPARENT`] becomes clear, the rest opaque.
fn pixel(color: u32) -> u32 {
	if color == TRANSPARENT {
		0
	} else {
		color | 0xff00_0000
	}
}

/// Fills `span` with `color` in vector stores, the last one overlapping
/// the one before, so short spans need no scalar tail.
#[inline(always)]
fn fill(span: &mut [u32], color: u32) {
	let lanes = u32x4::splat(color);
	match span.len() {
		0..4 => span.fill(color),
		len => {
			let (chunks, _) = span.as_chunks_mut::<4>();
			for chunk in chunks {
				*chunk = lanes.to_array();
			}
			lanes.copy_to_slice(&mut span[len - 4..]);
		}
	}
}

impl Decoder {
	fn new(aspect: usize, background: u32, foreground: u32) -> Self {
		let mut colors = [pixel(foreground); 256];
		for (index, color) in colors.iter_mut().enumerate().take(16) {
			*color = pixel(
				[
					0, 0x3333cc, 0xcc2121, 0x33cc33, 0xcc33cc, 0x33cccc, 0xcccc33, 0x878787, 0x424242,
					0x545499, 0x995454, 0x549954, 0x995499, 0x549999, 0x999954, 0xcccccc,
				][index],
			);
		}
		Self {
			pixels: Vec::new(),
			stride: 0,
			width: 0,
			height: 0,
			x: 0,
			y: 0,
			aspect,
			background: pixel(background),
			colors,
			selected: 0,
			painted: 0,
		}
	}
	fn size(&mut self, width: usize, height: usize) -> Option<()> {
		let width = self.width.max(width);
		let height = self.height.max(height);
		if width > MAX_SIDE || height > MAX_SIDE || width.checked_mul(height)? > MAX_PIXELS {
			return None;
		}
		if width > self.stride {
			self.widen(width);
		}
		self.width = width;
		self.height = height;
		self.pixels.resize(height * self.stride, self.background);
		Some(())
	}
	/// Moves the rows to a stride of at least `width`, doubling it so images
	/// that grow a column at a time are copied a few times only.
	#[cold]
	fn widen(&mut self, width: usize) {
		let stride = (self.stride * 2).min(MAX_SIDE).max(width);
		let mut pixels = vec![self.background; self.height * stride];
		// Before the first resize there are no rows, and no stride.
		let rows = self.pixels.chunks_exact(self.stride.max(1));
		for (to, from) in pixels.chunks_exact_mut(stride).zip(rows) {
			to[..from.len()].copy_from_slice(from);
		}
		(self.pixels, self.stride) = (pixels, stride);
	}
	#[inline(always)]
	fn paint(&mut self, byte: u8, repeat: usize) -> Option<()> {
		let band = 6 * self.aspect;
		self.painted = self.painted.checked_add(repeat.checked_mul(band)?)?;
		if self.painted > 64 * 1024 * 1024 {
			return None;
		}
		let end = self.x.checked_add(repeat)?;
		if end > self.width || self.y + band > self.height {
			self.size(end, self.y + band)?;
		}
		let color = self.colors[self.selected];
		// Only the set bits paint: each is `aspect` rows of the band.
		let mut bits = byte - b'?';
		while bits != 0 {
			let top = self.y + bits.trailing_zeros() as usize * self.aspect;
			bits &= bits - 1;
			for row in top..top + self.aspect {
				let start = row * self.stride + self.x;
				fill(&mut self.pixels[start..start + repeat], color);
			}
		}
		self.x = end;
		Some(())
	}
	fn decode(mut self, bytes: &[u8]) -> Option<Bitmap> {
		let mut i = 0;
		while i < bytes.len() {
			let byte = bytes[i];
			i += 1;
			match byte {
				b'?'..=b'~' => self.paint(byte, 1)?,
				b'!' => {
					let count = number(bytes, &mut i).max(1);
					let byte = *bytes.get(i)?;
					i += 1;
					if !(b'?'..=b'~').contains(&byte) {
						return None;
					}
					self.paint(byte, count)?;
				}
				b'$' => self.x = 0,
				b'-' => {
					self.x = 0;
					self.y = self.y.checked_add(6 * self.aspect)?;
					if self.y > MAX_SIDE {
						return None;
					}
				}
				b'#' | b'"' => {
					let (mut params, mut count) = ([0; 6], 1);
					params[0] = number(bytes, &mut i);
					while bytes.get(i) == Some(&b';') {
						i += 1;
						if count == 6 {
							return None;
						}
						params[count] = number(bytes, &mut i);
						count += 1;
					}
					if byte == b'#' {
						self.selected = params[0];
						if self.selected >= self.colors.len() {
							return None;
						}
						if count >= 5 {
							self.colors[self.selected] = match params[1] {
								2 => pixel(rgb(params[2], params[3], params[4])),
								1 => pixel(hls(params[2], params[3], params[4])),
								_ => self.colors[self.selected],
							};
						}
					} else if count >= 2 {
						let numerator = params[0].max(1);
						let denominator = params[1].max(1);
						self.aspect = numerator
							.saturating_add(denominator / 2)
							.checked_div(denominator)?
							.clamp(1, 10);
						if count >= 4 {
							self.size(params[2], params[3].checked_mul(self.aspect)?)?;
						}
					}
				}
				0x00..=0x20 => {}
				_ => {}
			}
		}
		if self.width == 0 || self.height == 0 {
			return None;
		}
		let (width, stride) = (self.width, self.stride);
		let mut pixels = self.pixels;
		if stride != width {
			for row in 1..self.height {
				pixels.copy_within(row * stride..row * stride + width, row * width);
			}
		}
		pixels.truncate(width * self.height);
		Some(Bitmap::new(width, self.height, pixels))
	}
}
fn number(bytes: &[u8], i: &mut usize) -> usize {
	let mut value = 0usize;
	while let Some(&byte) = bytes.get(*i).filter(|b| b.is_ascii_digit()) {
		value = value
			.saturating_mul(10)
			.saturating_add((byte - b'0') as usize);
		*i += 1;
	}
	value
}
fn rgb(r: usize, g: usize, b: usize) -> u32 {
	let channel = |v: usize| ((v.min(100) * 255 + 50) / 100) as u32;
	(channel(r) << 16) | (channel(g) << 8) | channel(b)
}
fn hls(h: usize, l: usize, s: usize) -> u32 {
	let h = ((h % 360 + 240) % 360) as f64 / 60.0;
	let l = l.min(100) as f64 / 100.0;
	let s = s.min(100) as f64 / 100.0;
	let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
	let x = c * (1.0 - (h % 2.0 - 1.0).abs());
	let (r, g, b) = match h as usize {
		0 => (c, x, 0.0),
		1 => (x, c, 0.0),
		2 => (0.0, c, x),
		3 => (0.0, x, c),
		4 => (x, 0.0, c),
		_ => (c, 0.0, x),
	};
	let m = l - c / 2.0;
	let channel = |v: f64| ((v + m) * 255.0).round() as u32;
	(channel(r) << 16) | (channel(g) << 8) | channel(b)
}

#[cfg(test)]
#[path = "sixel_reference_tests.rs"]
mod reference;
#[cfg(test)]
#[path = "sixel_tests.rs"]
mod tests;
