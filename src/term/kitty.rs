//! Applies kitty graphics commands: transmit, place, query and delete.
use std::sync::Arc;

use super::Term;
use crate::graphics::{
	Bitmap, Extent, KittyId, Placement, Rect,
	kitty::{self, Command, Error, Transmission},
};

/// Longest chain of relative placements (the protocol asks for at least 8).
const MAX_DEPTH: usize = 8;
/// Largest placement extent in cells (`c`, `r`).
const MAX_CELLS: usize = 4096;

impl Term {
	/// Runs one command and returns the reply to send, if any.
	pub(crate) fn apply_kitty(&mut self, transmission: Transmission) -> Option<Vec<u8>> {
		let Transmission { mut command, data } = transmission;
		let action = command.letter(b'a', b't');
		let result = if command.uint(b'i') != 0 && command.uint(b'I') != 0 {
			Err(Error::new("EINVAL", "both i and I given"))
		} else {
			match action {
				b't' | b'T' | b'q' => self.kitty_transmit(&mut command, data, action),
				b'p' => self.kitty_place(&mut command),
				b'd' => {
					self.kitty_delete(&command);
					return None;
				}
				_ => Err(Error::new("EINVAL", "unknown action")),
			}
		};
		command.reply(&result)
	}

	fn kitty_transmit(
		&mut self,
		command: &mut Command,
		data: Result<Vec<u8>, Error>,
		action: u8,
	) -> Result<(), Error> {
		let data = kitty::load(command, data?)?;
		let bitmap = Arc::new(kitty::decode(command, &data)?);
		drop(data);
		if action == b'q' {
			return Ok(());
		}
		let (id, number) = (command.uint(b'i'), command.uint(b'I'));
		// A new image under an existing id replaces it and its placements.
		self.remove_kitty_placements(|placement| placement.image == id);
		let shown: Vec<u32> = self.kitty_placements().map(|id| id.image).collect();
		let (id, evicted) = self
			.kitty_images
			.insert(id, number, bitmap.clone(), |id| shown.contains(&id));
		self.remove_kitty_placements(|placement| evicted.contains(&placement.image));
		if number != 0 {
			// The reply tells the client which id its number got.
			command.set(b'i', i64::from(id));
		}
		if action == b'T' {
			self.place_kitty_image(command, id, bitmap)?;
		}
		Ok(())
	}

	fn kitty_place(&mut self, command: &mut Command) -> Result<(), Error> {
		let number = command.uint(b'I');
		let image = if number != 0 {
			self.kitty_images.by_number(number)
		} else {
			self.kitty_images.get(command.uint(b'i'))
		};
		let image = image.ok_or_else(|| Error::new("ENOENT", "image not found"))?;
		let (id, bitmap) = (image.id, image.bitmap.clone());
		if number != 0 {
			command.set(b'i', i64::from(id));
		}
		self.kitty_images.touch(id);
		self.place_kitty_image(command, id, bitmap)
	}

	fn place_kitty_image(
		&mut self,
		command: &Command,
		image: u32,
		bitmap: Arc<Bitmap>,
	) -> Result<(), Error> {
		let key = KittyId {
			image,
			placement: command.uint(b'p'),
		};
		let parent = (command.uint(b'P') != 0).then(|| KittyId {
			image: command.uint(b'P'),
			placement: command.uint(b'Q'),
		});
		let (col, row) = match parent {
			Some(parent) => self.relative_position(key, parent, command)?,
			None => (self.cx, self.cy as isize),
		};
		let mut placement = Placement {
			col,
			row,
			offset: (0, 0),
			source: source_rect(command, &bitmap),
			extent: extent(command),
			z: command.int(b'z').clamp(i32::MIN.into(), i32::MAX.into()) as i32,
			kitty: Some(key),
			parent,
			bitmap,
		};
		let cell = self.cell_size();
		placement.offset = (
			(command.uint(b'X') as usize).min(cell.0 - 1),
			(command.uint(b'Y') as usize).min(cell.1 - 1),
		);
		if key.placement != 0 {
			self.images.retain(|other| other.kitty != Some(key));
		}
		// Relative placements never move the cursor; others end up right of
		// the image on its last row unless `C=1`.
		if parent.is_none() && command.uint(b'C') != 1 {
			let (cols, rows) = placement.cells(cell);
			for _ in 1..rows {
				if self.cy == self.scroll_bottom {
					placement.row -= 1;
				}
				self.linefeed();
			}
			self.cx = (self.cx + cols).min(self.cols - 1);
			self.wrap_pending = false;
		}
		self.images.push(placement);
		Ok(())
	}

	/// Where a placement relative to `parent` goes, after checking that the
	/// parent exists and that the chain stays short and acyclic.
	fn relative_position(
		&self,
		key: KittyId,
		parent: KittyId,
		command: &Command,
	) -> Result<(usize, isize), Error> {
		let find = |id: KittyId| {
			self
				.images
				.iter()
				.find(|placement| placement.kitty == Some(id))
		};
		let anchor =
			find(parent).ok_or_else(|| Error::new("ENOPARENT", "parent placement not found"))?;
		let mut depth = 1;
		let mut link = Some(anchor);
		while let Some(placement) = link {
			if key.placement != 0 && placement.kitty == Some(key) {
				return Err(Error::new("ECYCLE", "placement would be its own ancestor"));
			}
			link = placement.parent.and_then(find);
			if link.is_some() {
				depth += 1;
			}
		}
		if depth > MAX_DEPTH {
			return Err(Error::new(
				"ETOODEEP",
				"relative placements nested too deeply",
			));
		}
		let col = (anchor.col as i64 + command.int(b'H')).max(0) as usize;
		let row = anchor.row + command.int(b'V') as isize;
		Ok((col, row))
	}

	fn kitty_delete(&mut self, command: &Command) {
		let which = command.letter(b'd', b'a');
		let free = which.is_ascii_uppercase();
		let cell = self.cell_size();
		// Cells are 1-based in the protocol.
		let x = command.int(b'x') - 1;
		let y = command.int(b'y') - 1;
		let z = command.int(b'z');
		let (cursor_x, cursor_y) = (self.cx as i64, self.cy as i64);
		let by_id = |id: u32| {
			let placement = command.uint(b'p');
			move |p: &Placement| {
				let key = p.kitty.unwrap();
				key.image == id && (placement == 0 || key.placement == placement)
			}
		};
		let covers = move |p: &Placement, x: i64, y: i64| {
			let (cols, rows) = p.cells(cell);
			(p.col as i64..(p.col + cols) as i64).contains(&x)
				&& (p.row as i64..p.row as i64 + rows as i64).contains(&y)
		};
		let mut targeted = Vec::new();
		let selected: Box<dyn Fn(&Placement) -> bool> = match which.to_ascii_lowercase() {
			b'a' => Box::new(|_| true),
			b'i' => {
				targeted.push(command.uint(b'i'));
				Box::new(by_id(command.uint(b'i')))
			}
			b'n' => {
				let id = self
					.kitty_images
					.by_number(command.uint(b'I'))
					.map(|image| image.id);
				targeted.extend(id);
				match id {
					Some(id) => Box::new(by_id(id)),
					None => Box::new(|_| false),
				}
			}
			b'c' => Box::new(move |p| covers(p, cursor_x, cursor_y)),
			b'p' => Box::new(move |p| covers(p, x, y)),
			b'q' => Box::new(move |p| covers(p, x, y) && i64::from(p.z) == z),
			b'r' => {
				let range = command.uint(b'x')..=command.uint(b'y');
				targeted.extend(self.kitty_images.ids().filter(|id| range.contains(id)));
				Box::new(move |p| range.contains(&p.kitty.unwrap().image))
			}
			b'x' => Box::new(move |p| {
				let cols = p.cells(cell).0 as i64;
				(p.col as i64..p.col as i64 + cols).contains(&x)
			}),
			b'y' => Box::new(move |p| {
				let rows = p.cells(cell).1 as i64;
				(p.row as i64..p.row as i64 + rows).contains(&y)
			}),
			b'z' => Box::new(move |p| i64::from(p.z) == z),
			_ => return,
		};
		for placement in &self.images {
			if let Some(key) = placement.kitty
				&& selected(placement)
			{
				targeted.push(key.image);
			}
		}
		self.images.retain(|p| p.kitty.is_none() || !selected(p));
		drop(selected);
		self.remove_orphans();
		if free {
			let shown: Vec<u32> = self.kitty_placements().map(|id| id.image).collect();
			for id in targeted.into_iter().filter(|id| !shown.contains(id)) {
				self.kitty_images.remove(id);
			}
		}
	}

	/// Kitty placements on both screens.
	fn kitty_placements(&self) -> impl Iterator<Item = KittyId> {
		self
			.images
			.iter()
			.chain(&self.primary_images)
			.filter_map(|placement| placement.kitty)
	}

	/// Removes kitty placements on both screens, and their children.
	fn remove_kitty_placements(&mut self, remove: impl Fn(KittyId) -> bool) {
		let keep = |placement: &Placement| !placement.kitty.is_some_and(&remove);
		self.images.retain(keep);
		self.primary_images.retain(keep);
		self.remove_orphans();
	}

	/// Removes relative placements whose parent is gone, transitively.
	fn remove_orphans(&mut self) {
		for images in [&mut self.images, &mut self.primary_images] {
			loop {
				let ids: Vec<KittyId> = images.iter().filter_map(|p| p.kitty).collect();
				let before = images.len();
				images.retain(|p| p.parent.is_none_or(|parent| ids.contains(&parent)));
				if images.len() == before {
					break;
				}
			}
		}
	}

	/// Clear screen (ED 2) removes the kitty placements on screen; clearing
	/// the scrollback (ED 3) those above it.
	pub(crate) fn clear_kitty_placements(&mut self, history: bool) {
		let cell = self.cell_size();
		self.images.retain(|placement| {
			let bottom = placement.row + placement.cells(cell).1 as isize;
			let in_history = bottom <= 0;
			placement.kitty.is_none() || in_history != history
		});
		self.remove_orphans();
	}
}

/// The `x`, `y`, `w`, `h` source rectangle, clamped to the bitmap.
fn source_rect(command: &Command, bitmap: &Bitmap) -> Rect {
	let x = (command.uint(b'x') as usize).min(bitmap.width);
	let y = (command.uint(b'y') as usize).min(bitmap.height);
	let clamp = |value: u32, rest: usize| match value as usize {
		0 => rest,
		value => value.min(rest),
	};
	Rect {
		x,
		y,
		width: clamp(command.uint(b'w'), bitmap.width - x),
		height: clamp(command.uint(b'h'), bitmap.height - y),
	}
}

fn extent(command: &Command) -> Extent {
	let cols = (command.uint(b'c') as usize).min(MAX_CELLS);
	let rows = (command.uint(b'r') as usize).min(MAX_CELLS);
	match (cols, rows) {
		(0, 0) => Extent::Natural,
		(cols, 0) => Extent::Cols(cols),
		(0, rows) => Extent::Rows(rows),
		(cols, rows) => Extent::Cells { cols, rows },
	}
}
