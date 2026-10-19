//! iTerm2's inline images on the grid. Like Sixel images, text written over
//! them erases them, and they share Sixel's quota.
use std::sync::Arc;

use super::Term;
use crate::graphics::{
	Extent, Placement,
	iterm::{FileArgs, MAX_FILE, Upload, fit, split_file},
	kitty::decode::decode_file,
};

impl Term {
	/// `File=args:base64`, a whole file in one sequence.
	pub(crate) fn iterm_file(&mut self, text: &[u8]) {
		let (args, data) = split_file(text);
		let mut upload = Upload::new(FileArgs::parse(args));
		upload.push(data, MAX_FILE);
		self.show_upload(upload);
	}

	/// `MultipartFile=args` starts a file sent in `FilePart`s; an
	/// unfinished one is dropped.
	pub(crate) fn iterm_multipart_file(&mut self, args: &[u8]) {
		self.iterm.upload = Some(Box::new(Upload::new(FileArgs::parse(args))));
	}

	pub(crate) fn iterm_file_part(&mut self, base64: &[u8]) {
		if let Some(upload) = &mut self.iterm.upload {
			upload.push(base64, MAX_FILE);
		}
	}

	pub(crate) fn iterm_file_end(&mut self) {
		if let Some(upload) = self.iterm.upload.take() {
			self.show_upload(*upload);
		}
	}

	/// Shows an inline image at the cursor. The cursor ends after the
	/// image on its last row, scrolling if needed, unless asked to stay.
	fn show_upload(&mut self, upload: Upload) {
		let args = upload.args.clone();
		if !args.inline {
			return;
		}
		let Some(bitmap) = upload
			.finish(MAX_FILE)
			.and_then(|file| decode_file(&file).ok())
		else {
			return;
		};
		let fit = fit(
			&args,
			(bitmap.width, bitmap.height),
			self.cell_size(),
			(self.cols, self.rows),
		);
		self.make_room_for_image(bitmap.pixels.len());
		let (col, mut row) = (self.cx, self.cy as isize);
		if args.move_cursor {
			for _ in 1..fit.rows {
				if self.cy == self.scroll_bottom {
					row -= 1;
				}
				self.linefeed();
			}
			let end = col + fit.cols;
			self.wrap_pending = end >= self.cols;
			self.cx = end.min(self.cols - 1);
		}
		let mut placement = Placement::natural(Arc::new(bitmap), col, row);
		placement.extent = Extent::Pixels {
			width: fit.width,
			height: fit.height,
		};
		self.images.push(placement);
	}
}
