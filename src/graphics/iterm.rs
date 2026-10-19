//! iTerm2's inline images (OSC 1337 `File=`): the arguments, and the size
//! an image is drawn at, and files received in parts.
use super::base64;

/// Largest file accepted, decoded.
pub(crate) const MAX_FILE: usize = 32 << 20;
/// Largest width or height drawn, in pixels.
const MAX_DRAWN: usize = 16384;

/// A requested width or height.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Dim {
	/// From the image's own size.
	#[default]
	Auto,
	Cells(usize),
	Pixels(usize),
	/// Of the terminal's width or height.
	Percent(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileArgs {
	/// Decoded size the sender announced, if any.
	pub size: Option<usize>,
	pub width: Dim,
	pub height: Dim,
	pub preserve_aspect_ratio: bool,
	/// Shown in the terminal; other files would be downloads, which a
	/// program's output may not make.
	pub inline: bool,
	/// WezTerm's `doNotMoveCursor=1` leaves the cursor where it was.
	pub move_cursor: bool,
}

impl Default for FileArgs {
	fn default() -> Self {
		Self {
			size: None,
			width: Dim::Auto,
			height: Dim::Auto,
			preserve_aspect_ratio: true,
			inline: false,
			move_cursor: true,
		}
	}
}

impl FileArgs {
	/// Parses `key=value;…`; unknown keys and bad values are ignored.
	pub(crate) fn parse(args: &[u8]) -> Self {
		let mut parsed = Self::default();
		for field in args.split(|&byte| byte == b';') {
			let Some(at) = field.iter().position(|&byte| byte == b'=') else {
				continue;
			};
			let (key, value) = (&field[..at], &field[at + 1..]);
			match key {
				b"size" => parsed.size = number(value),
				b"width" => parsed.width = dim(value),
				b"height" => parsed.height = dim(value),
				b"preserveAspectRatio" => parsed.preserve_aspect_ratio = value != b"0",
				b"inline" => parsed.inline = value == b"1",
				b"doNotMoveCursor" => parsed.move_cursor = value != b"1",
				_ => {}
			}
		}
		parsed
	}
}

fn number(digits: &[u8]) -> Option<usize> {
	if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
		return None;
	}
	std::str::from_utf8(digits).ok()?.parse().ok()
}

/// `N`, `Npx`, `N%` or `auto`; anything else is automatic.
fn dim(value: &[u8]) -> Dim {
	let parsed = if let Some(digits) = value.strip_suffix(b"px") {
		number(digits).map(Dim::Pixels)
	} else if let Some(digits) = value.strip_suffix(b"%") {
		number(digits).map(Dim::Percent)
	} else {
		number(value).map(Dim::Cells)
	};
	parsed.unwrap_or_default()
}

/// Splits `args:base64` at its first colon; without one it is all arguments.
pub(crate) fn split_file(text: &[u8]) -> (&[u8], &[u8]) {
	match text.iter().position(|&byte| byte == b':') {
		Some(at) => (&text[..at], &text[at + 1..]),
		None => (text, &[]),
	}
}

/// Where an image goes: its drawn size in pixels, and the cells it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fit {
	pub width: usize,
	pub height: usize,
	pub cols: usize,
	pub rows: usize,
}

/// Sizes an `image` of `(width, height)` pixels as `args` ask, for cells
/// of `cell` pixels on a screen of `screen` cells. Automatic sizes keep the
/// aspect ratio and shrink images wider than the screen.
pub(crate) fn fit(
	args: &FileArgs,
	image: (usize, usize),
	cell: (usize, usize),
	screen: (usize, usize),
) -> Fit {
	let (image_width, image_height) = (image.0.max(1), image.1.max(1));
	let (cell_width, cell_height) = (cell.0.max(1), cell.1.max(1));
	let pixels = |dim: Dim, cell: usize, screen: usize| match dim {
		Dim::Auto => None,
		Dim::Cells(cells) => Some(cells.saturating_mul(cell)),
		Dim::Pixels(pixels) => Some(pixels),
		Dim::Percent(percent) => Some(percent.saturating_mul(screen * cell) / 100),
	};
	let bound = |pixels: usize| pixels.clamp(1, MAX_DRAWN);
	let wanted_width = pixels(args.width, cell_width, screen.0).map(bound);
	let wanted_height = pixels(args.height, cell_height, screen.1).map(bound);
	let scaled = |side: usize, num: usize, den: usize| bound(side * num / den);
	// Two given sides make a box the image is fitted into.
	let (width, height, area) = match (wanted_width, wanted_height) {
		(None, None) => {
			let width = bound(image_width).min(screen.0 * cell_width).max(1);
			(width, scaled(width, image_height, image_width), None)
		}
		(Some(width), None) => (width, scaled(width, image_height, image_width), None),
		(None, Some(height)) => (scaled(height, image_width, image_height), height, None),
		(Some(width), Some(height)) if !args.preserve_aspect_ratio => (width, height, None),
		(Some(width), Some(height)) if width * image_height <= height * image_width => (
			width,
			scaled(width, image_height, image_width),
			Some((width, height)),
		),
		(Some(width), Some(height)) => (
			scaled(height, image_width, image_height),
			height,
			Some((width, height)),
		),
	};
	let area = area.unwrap_or((width, height));
	Fit {
		width,
		height,
		cols: area.0.div_ceil(cell_width),
		rows: area.1.div_ceil(cell_height),
	}
}

/// A file being received, in one piece (`File=`) or several (`FilePart=`).
#[derive(Clone)]
pub(crate) struct Upload {
	pub args: FileArgs,
	decoder: base64::Decoder,
	data: Vec<u8>,
	failed: bool,
}

impl Upload {
	pub(crate) fn new(args: FileArgs) -> Self {
		let failed = args.size.is_some_and(|size| size > MAX_FILE);
		Self {
			args,
			decoder: base64::Decoder::default(),
			data: Vec::new(),
			failed,
		}
	}

	/// Adds base64 text; bad text or more than `limit` bytes fail the upload.
	pub(crate) fn push(&mut self, base64: &[u8], limit: usize) {
		if self.failed {
			return;
		}
		// A part padded on its own ends a quantum; the next starts afresh.
		if self.decoder.padded() {
			self.decoder = base64::Decoder::default();
		}
		let ok = self.decoder.push(base64, &mut self.data).is_ok();
		if !ok || self.data.len() > limit {
			self.fail();
		}
	}

	fn fail(&mut self) {
		self.failed = true;
		self.data = Vec::new();
	}

	/// The file, unless the upload failed or ends up over `limit` bytes.
	pub(crate) fn finish(mut self, limit: usize) -> Option<Vec<u8>> {
		if self.failed || self.decoder.finish(&mut self.data).is_err() || self.data.len() > limit {
			return None;
		}
		Some(self.data)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn upload(parts: &[&str], limit: usize) -> Option<Vec<u8>> {
		let mut upload = Upload::new(FileArgs::default());
		for part in parts {
			upload.push(part.as_bytes(), limit);
		}
		upload.finish(limit)
	}

	#[test]
	fn uploads_decode_parts_of_any_length() {
		assert_eq!(
			upload(&["aGVs", "bG8", "gd29y", "bGQ="], 100).unwrap(),
			b"hello world"
		);
		// Parts each padded on their own still join.
		assert_eq!(upload(&["aGk=", "aGk="], 100).unwrap(), b"hihi");
		assert_eq!(upload(&[], 100).unwrap(), b"");
	}

	#[test]
	fn bad_or_oversized_uploads_fail() {
		assert_eq!(upload(&["aGVs", "@@"], 100), None);
		assert_eq!(upload(&["a"], 100), None);
		assert_eq!(upload(&["aGVsbG8="], 5).unwrap(), b"hello");
		assert_eq!(upload(&["aGVsbG8="], 4), None);
		assert_eq!(upload(&["aGVs", "bG8="], 4), None);
		// Unpadded, the last bytes only come out at the end.
		assert_eq!(upload(&["aGVsbG8"], 4), None);
		// Announcing too large a file fails it at once.
		let mut upload = Upload::new(FileArgs::parse(b"size=33554433"));
		upload.push(b"aGk=", 100);
		assert_eq!(upload.finish(100), None);
	}

	#[test]
	fn arguments_are_parsed() {
		let args = FileArgs::parse(
			b"name=Zm9vLnBuZw==;size=1234;width=5;height=10px;preserveAspectRatio=0;inline=1;\
			  doNotMoveCursor=1",
		);
		assert_eq!(
			args,
			FileArgs {
				size: Some(1234),
				width: Dim::Cells(5),
				height: Dim::Pixels(10),
				preserve_aspect_ratio: false,
				inline: true,
				move_cursor: false,
			}
		);
		let args = FileArgs::parse(b"width=50%;height=auto;inline=2;bogus;x=y");
		assert_eq!((args.width, args.height), (Dim::Percent(50), Dim::Auto));
		assert!(!args.inline && args.move_cursor && args.preserve_aspect_ratio);
		// Bad sizes fall back to automatic ones.
		for bad in ["width=", "width=-3", "width=3em", "width=px", "width=1.5"] {
			assert_eq!(FileArgs::parse(bad.as_bytes()).width, Dim::Auto, "{bad}");
		}
		assert_eq!(FileArgs::default(), FileArgs::parse(b""));
	}

	#[test]
	fn data_follows_the_first_colon() {
		assert_eq!(
			split_file(b"inline=1;width=2:QUJD"),
			(&b"inline=1;width=2"[..], &b"QUJD"[..])
		);
		assert_eq!(split_file(b":QUJD"), (&b""[..], &b"QUJD"[..]));
		assert_eq!(split_file(b"inline=1"), (&b"inline=1"[..], &b""[..]));
	}

	fn sized(args: &str, image: (usize, usize)) -> Fit {
		fit(&FileArgs::parse(args.as_bytes()), image, (10, 20), (80, 24))
	}

	fn of(width: usize, height: usize, cols: usize, rows: usize) -> Fit {
		Fit {
			width,
			height,
			cols,
			rows,
		}
	}

	#[test]
	fn automatic_sizes_keep_the_image_but_fit_the_screen() {
		assert_eq!(sized("", (100, 50)), of(100, 50, 10, 3));
		assert_eq!(sized("", (1600, 100)), of(800, 50, 80, 3));
		// Tall images are not shrunk; they scroll.
		assert_eq!(sized("", (10, 1000)), of(10, 1000, 1, 50));
	}

	#[test]
	fn one_given_side_scales_the_other() {
		assert_eq!(sized("width=20", (100, 50)), of(200, 100, 20, 5));
		assert_eq!(sized("height=50%", (100, 50)), of(480, 240, 48, 12));
		assert_eq!(sized("width=25%", (100, 50)), of(200, 100, 20, 5));
		assert_eq!(sized("width=15px", (100, 50)), of(15, 7, 2, 1));
	}

	#[test]
	fn two_given_sides_fit_or_stretch() {
		assert_eq!(sized("width=30px;height=30px", (100, 50)), of(30, 15, 3, 2));
		assert_eq!(sized("width=30px;height=30px", (50, 100)), of(15, 30, 3, 2));
		assert_eq!(
			sized("width=30px;height=30px;preserveAspectRatio=0", (100, 50)),
			of(30, 30, 3, 2)
		);
	}

	#[test]
	fn sizes_are_bounded() {
		assert_eq!(sized("width=999999999", (100, 50)).width, MAX_DRAWN);
		assert_eq!(sized("height=999999999%", (100, 50)).height, MAX_DRAWN);
		let tiny = sized("width=1px", (1000, 1));
		assert_eq!(
			(tiny.width, tiny.height, tiny.cols, tiny.rows),
			(1, 1, 1, 1)
		);
		assert_eq!(sized("", (0, 0)), of(1, 1, 1, 1));
	}
}
