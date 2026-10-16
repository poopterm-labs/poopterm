//! Bounded DEC Sixel decoder. Coordinates are pixels; transparent pixels use u32::MAX.
use std::sync::Arc;

pub(crate) const TRANSPARENT: u32 = u32::MAX;
const MAX_SIDE: usize = 2048;
const MAX_PIXELS: usize = 4 * 1024 * 1024;
pub(crate) const MAX_PAYLOAD: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Bitmap {
	pub width: usize,
	pub height: usize,
	pub pixels: Vec<u32>,
}
#[derive(Clone)]
pub(crate) struct Placement {
	pub bitmap: Arc<Bitmap>,
	pub col: usize,
	pub row: isize,
}
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
	pub fn push(&mut self, byte: u8) {
		if self.bytes.len() < MAX_PAYLOAD {
			self.bytes.push(byte);
		} else {
			self.overflow = true;
		}
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
	rows: Vec<Vec<u32>>,
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
impl Decoder {
	fn new(aspect: usize, background: u32, foreground: u32) -> Self {
		let mut colors = [foreground; 256];
		for (index, color) in colors.iter_mut().enumerate().take(16) {
			*color = [
				0, 0x3333cc, 0xcc2121, 0x33cc33, 0xcc33cc, 0x33cccc, 0xcccc33, 0x878787, 0x424242,
				0x545499, 0x995454, 0x549954, 0x995499, 0x549999, 0x999954, 0xcccccc,
			][index];
		}
		Self {
			rows: Vec::new(),
			width: 0,
			height: 0,
			x: 0,
			y: 0,
			aspect,
			background,
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
		self.width = width;
		self.height = height;
		self.rows.resize_with(height, Vec::new);
		Some(())
	}
	fn paint(&mut self, byte: u8, repeat: usize) -> Option<()> {
		self.painted = self
			.painted
			.checked_add(repeat.checked_mul(6 * self.aspect)?)?;
		if self.painted > 16 * 1024 * 1024 {
			return None;
		}
		self.size(
			self.x.checked_add(repeat)?,
			self.y.checked_add(6 * self.aspect)?,
		)?;
		for bit in 0..6 {
			if (byte - b'?') & (1 << bit) == 0 {
				continue;
			}
			for dy in 0..self.aspect {
				let row = &mut self.rows[self.y + bit * self.aspect + dy];
				row.resize(self.width, self.background);
				row[self.x..self.x + repeat].fill(self.colors[self.selected]);
			}
		}
		self.x += repeat;
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
					let mut params = vec![number(bytes, &mut i)];
					while bytes.get(i) == Some(&b';') {
						i += 1;
						if params.len() == 5 {
							return None;
						}
						params.push(number(bytes, &mut i));
					}
					if byte == b'#' {
						self.selected = params[0];
						if self.selected >= self.colors.len() {
							return None;
						}
						if params.len() == 5 {
							self.colors[self.selected] = match params[1] {
								2 => rgb(params[2], params[3], params[4]),
								1 => hls(params[2], params[3], params[4]),
								_ => return None,
							};
						}
					} else if params.len() >= 2 {
						// DEC rounds vertical/horizontal to the nearest integer.
						let numerator = params[0].max(1);
						let denominator = params[1].max(1);
						self.aspect = numerator
							.saturating_add(denominator / 2)
							.checked_div(denominator)?
							.clamp(1, 10);
						if params.len() >= 4 {
							self.size(params[2], params[3].checked_mul(self.aspect)?)?;
						}
					}
				}
				0x00..=0x20 => {}
				_ => return None,
			}
		}
		if self.width == 0 || self.height == 0 {
			return None;
		}
		let mut pixels = vec![self.background; self.width * self.height];
		for (y, row) in self.rows.iter().enumerate() {
			pixels[y * self.width..y * self.width + row.len()].copy_from_slice(row);
		}
		Some(Bitmap {
			width: self.width,
			height: self.height,
			pixels,
		})
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
	let h = ((h % 360 + 240) % 360) as f64 / 60.0; // DEC hue zero is blue.
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
mod tests {
	use super::*;
	fn decode(data: &[u8]) -> Option<Bitmap> {
		let mut t = Transfer::new(7, 1);
		for &byte in data {
			t.push(byte);
		}
		t.finish(0, 0xffffff)
	}
	#[test]
	fn repeats_planes_and_transparency() {
		let image = decode(b"#1;2;100;0;0!3~$#2;2;0;100;0?@").unwrap();
		assert_eq!((image.width, image.height), (3, 6));
		assert_eq!(image.pixels[0], 0xff0000);
		assert_eq!(image.pixels[1], 0x00ff00);
		assert_eq!(image.pixels[4], 0xff0000);
		assert_eq!(decode(b"?").unwrap().pixels[0], TRANSPARENT);
	}
	#[test]
	fn raster_attributes_and_aspect_ratio() {
		let image = decode(b"\"2;1;4;6#1;2;0;0;100@").unwrap();
		assert_eq!((image.width, image.height), (4, 12));
		assert_eq!(image.pixels[0], 0x0000ff);
		assert_eq!(image.pixels[4], 0x0000ff);
	}
	#[test]
	fn hls_primary_colors_follow_dec_hue_order() {
		assert_eq!(hls(0, 50, 100), 0x0000ff);
		assert_eq!(hls(120, 50, 100), 0xff0000);
		assert_eq!(hls(240, 50, 100), 0x00ff00);
	}
	#[test]
	fn hostile_sizes_and_bad_sequences_are_rejected() {
		for input in [
			b"!999999999999999999999999999~".as_slice(),
			b"\"1;1;999999;999999",
			b"#999~",
			b"!3x!3!",
			b"#1;3;1;2;3~",
		] {
			assert!(decode(input).is_none());
		}
		let mut t = Transfer::new(0, 0);
		for _ in 0..=MAX_PAYLOAD {
			t.push(b'?');
		}
		assert!(t.finish(0, 0).is_none());
	}
	#[test]
	fn background_and_newline() {
		let mut t = Transfer::new(7, 0);
		for &b in b"#1;2;100;0;0@-@" {
			t.push(b);
		}
		let image = t.finish(0x123456, 0).unwrap();
		assert_eq!(image.height, 12);
		assert_eq!(image.pixels[1], 0x123456);
		assert_eq!(image.pixels[6], 0xff0000);
	}
}
