//! Images shown in the terminal (Sixel and the kitty graphics protocol):
//! decoded bitmaps and where they are placed on the grid.
pub(crate) mod base64;
pub(crate) mod kitty;

use std::sync::Arc;

/// Decoded pixels, `0xAARRGGBB` with straight (not premultiplied) alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bitmap {
	pub width: usize,
	pub height: usize,
	/// Every pixel has alpha 255, so drawing can copy rows.
	pub opaque: bool,
	pub pixels: Vec<u32>,
}

impl Bitmap {
	/// Builds a bitmap from `0xAARRGGBB` pixels, working out `opaque`.
	pub(crate) fn new(width: usize, height: usize, pixels: Vec<u32>) -> Self {
		debug_assert_eq!(pixels.len(), width * height);
		let opaque = pixels.iter().all(|&pixel| pixel >> 24 == 0xff);
		Self {
			width,
			height,
			opaque,
			pixels,
		}
	}
}

/// A rectangle in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rect {
	pub x: usize,
	pub y: usize,
	pub width: usize,
	pub height: usize,
}

/// How large a placement is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Extent {
	/// The source rectangle at one image pixel per screen pixel.
	Natural,
	/// Scaled to fill this many cells.
	Cells { cols: usize, rows: usize },
	/// This many columns wide; the height keeps the aspect ratio.
	Cols(usize),
	/// This many rows high; the width keeps the aspect ratio.
	Rows(usize),
}

/// Identifies a kitty placement: image id and placement id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct KittyId {
	pub image: u32,
	pub placement: u32,
}

/// One image shown at a grid position.
#[derive(Clone, Debug)]
pub(crate) struct Placement {
	pub bitmap: Arc<Bitmap>,
	pub col: usize,
	/// Screen row of the top edge; negative once scrolled into history.
	pub row: isize,
	/// Offset of the top-left corner inside the first cell, in pixels.
	pub offset: (usize, usize),
	/// The part of the bitmap shown.
	pub source: Rect,
	pub extent: Extent,
	/// Drawing order; negative values go under text.
	pub z: i32,
	/// Set for kitty placements. Sixel images (`None`) are erased by text
	/// written over them; kitty images are not.
	pub kitty: Option<KittyId>,
	/// The placement this one is relative to; it is deleted with it.
	pub parent: Option<KittyId>,
}

impl Placement {
	/// The whole bitmap at natural size, as Sixel images are shown.
	pub(crate) fn natural(bitmap: Arc<Bitmap>, col: usize, row: isize) -> Self {
		let source = Rect {
			x: 0,
			y: 0,
			width: bitmap.width,
			height: bitmap.height,
		};
		Self {
			bitmap,
			col,
			row,
			offset: (0, 0),
			source,
			extent: Extent::Natural,
			z: 0,
			kitty: None,
			parent: None,
		}
	}

	/// Size on screen in pixels for a given cell size.
	pub(crate) fn size(&self, (cell_width, cell_height): (usize, usize)) -> (usize, usize) {
		let source = self.source;
		match self.extent {
			Extent::Natural => (source.width, source.height),
			Extent::Cells { cols, rows } => (cols * cell_width, rows * cell_height),
			Extent::Cols(cols) => {
				let width = cols * cell_width;
				(width, width * source.height / source.width.max(1))
			}
			Extent::Rows(rows) => {
				let height = rows * cell_height;
				(height * source.width / source.height.max(1), height)
			}
		}
	}

	/// Columns and rows covered, counting the offset into the first cell.
	pub(crate) fn cells(&self, cell: (usize, usize)) -> (usize, usize) {
		let (width, height) = self.size(cell);
		(
			(self.offset.0 + width).div_ceil(cell.0.max(1)),
			(self.offset.1 + height).div_ceil(cell.1.max(1)),
		)
	}
}
